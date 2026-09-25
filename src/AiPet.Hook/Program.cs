using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AiPet;

/// aipet-hook --agent claude|codex   (aipet-hook.exe on Windows)
///
/// Run by each coding agent on every hook event. Reads the event JSON from stdin and hands it to the running AiPet
/// app over a local socket (see Ipc); the app keeps the chats' state. When the pet isn't running it does nothing at
/// all: it writes nothing anywhere and starts nothing. Claude events are described in ClaudeHook, Codex's in CodexHook.
/// It only observes: it never prints anything back to the agent (so nothing can be injected into a chat
/// through it), and it never fails, so it can't disturb the agent.
///
/// Also: aipet-hook --install|--uninstall claude|codex, and aipet-hook --doctor claude|codex [--probe].
static class Program
{
    public static int Main(string[] args)
    {
        launched = DateTime.UtcNow;
        if (args.Length == 1 && args[0] is "--install" or "--uninstall" or "--doctor")
        {
            // without an agent this would otherwise run as a hook and wait for an event on stdin
            Console.Error.WriteLine(args[0] == "--doctor" ? "usage: aipet-hook --doctor claude|codex [--probe]"
                                                          : "usage: aipet-hook --install|--uninstall claude|codex");
            return 2;
        }
        if (args.Length >= 2 && args[0] is "--install" or "--uninstall") return Install.Run(args[0], args[1].ToLowerInvariant());
        if (args.Length >= 2 && args[0] == "--doctor") return Doctor.Run(args[1].ToLowerInvariant(), args.Contains("--probe"));
        var agent = ArgValue(args, "--agent") ?? "claude";
        NamedPipeClientStream pet = null;
        try
        {
            JsonObject payload;
            string unreadable = null;
            try { payload = ReadPayload(); }
            catch (Exception ex) { payload = new JsonObject(); unreadable = $"{ex.GetType().Name}: {ex.Message}"; }
            // nothing is written before this: with the pet closed the hook leaves no trace. A pet whose instances are
            // all taken is waited for with what the run's budget has left once the reply's time is set aside.
            int busyMs = Math.Min(Ipc.ConnectMs, Ipc.HookBudgetMs - Ipc.TimeoutMs - (int)(DateTime.UtcNow - launched).TotalMilliseconds);
            pet = Ipc.Connect(busyMs, out bool busy);
            if (pet == null)
            {
                // the pet runs (its pipe is there), so this may be written: the one trace of an event it never got
                if (busy) Trace($"{agent} {Str(payload, "hook_event_name") ?? "?"}: the pet's pipe stayed busy for {Math.Max(0, busyMs)} ms, so the event is lost");
                return 0;
            }
            if (unreadable != null) Trace($"{agent}: the event isn't JSON, so the pet gets an empty one: {unreadable}");
            var envelope = agent == "codex" ? CodexHook.Envelope(payload) : ClaudeHook.Envelope(payload);
            // the reply means the pet has the event
            var reply = Ipc.Ask(pet, Request(envelope));
            if (!(JsonNode.Parse(reply ?? "null") is JsonObject r && r[Ipc.Ok] is JsonValue ok && ok.TryGetValue(out bool yes) && yes))
                Trace($"{agent} {lastEvent}: the pet didn't take it: {reply ?? "no reply"}");
        }
        catch (Exception ex) { if (pet != null) Trace($"{agent} {lastEvent}: {ex.GetType().Name}: {ex.Message}"); }
        finally { pet?.Dispose(); }
        return 0;
    }

    internal static string lastEvent = "?";

    /// When Main started, before stdin was read: the event's time `at` (the hook doesn't look at its own or any other
    /// process), and where the run's time budget starts.
    internal static DateTime launched;

    /// The event JSON from stdin. Gives up after a few seconds so a hook can never hang, even if stdin stays open.
    static JsonObject ReadPayload()
    {
        var read = Task.Run(() =>
        {
            using var stdin = Console.OpenStandardInput();
            using var reader = new StreamReader(stdin, Encoding.UTF8);  // also drops a UTF-8 byte order mark
            return reader.ReadToEnd();
        });
        var input = read.Wait(TimeSpan.FromSeconds(3)) ? read.Result : "";
        return JsonNode.Parse(string.IsNullOrWhiteSpace(input) ? "{}" : input) as JsonObject ?? new JsonObject();
    }

    /// The envelope around an event (see Ipc); the agent's hook adds its own fields and the payload. at is when the
    /// hook started, which orders events that finish out of order (for Codex only after its own ids, see AgentSessions).
    internal static JsonObject Envelope(string agent) => new()
    {
        [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = agent,
        [Ipc.At] = Ipc.UnixTime(launched), [Ipc.Sent] = Ipc.UnixTime(DateTime.UtcNow), [Ipc.Pid] = Environment.ProcessId,
    };

    /// The event as the pet needs it: without the tool's output (often large, and never used), and with every string
    /// cut to Ipc.MaxString so the request usually stays well within the pet's limit (see Request for the rest).
    internal static JsonObject Trimmed(JsonObject payload)
    {
        payload.Remove("tool_response");
        Cap(payload);
        return payload;
    }

    static void Cap(JsonNode node)
    {
        switch (node)
        {
            case JsonObject o:
                foreach (var key in o.Select(kv => kv.Key).ToList())
                    if (Cut(o[key]) is { } s) o[key] = s; else Cap(o[key]);
                break;
            case JsonArray a:
                for (int i = 0; i < a.Count; i++)
                    if (Cut(a[i]) is { } s) a[i] = s; else Cap(a[i]);
                break;
        }
    }

    /// A string value longer than Ipc.MaxString, cut to it (not in the middle of a surrogate pair); null otherwise.
    static string Cut(JsonNode n)
    {
        if (n is not JsonValue v || !v.TryGetValue(out string s) || s.Length <= Ipc.MaxString) return null;
        int len = char.IsHighSurrogate(s[Ipc.MaxString - 1]) ? Ipc.MaxString - 1 : Ipc.MaxString;
        return s[..len];
    }

    /// Non-ASCII text and quotes as they are: the default encoder writes each as a 6-byte \uXXXX, which can make one
    /// cut string 1.5 MiB. The depth leaves room for the envelope around an event nested as deep as stdin allows.
    static readonly JsonSerializerOptions Wire = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping, MaxDepth = Ipc.MaxDepth };

    /// The envelope as the request line. Cut strings can still add up past the pet's limit (a MultiEdit of several
    /// big files, say); then the tool call keeps only what the pet reads of it, rather than being dropped whole.
    static string Request(JsonObject envelope)
    {
        var line = envelope.ToJsonString(Wire);
        if (Encoding.UTF8.GetByteCount(line) <= Ipc.MaxRequest || envelope[Ipc.Payload]?["tool_input"] is not JsonObject input) return line;
        foreach (var key in input.Select(kv => kv.Key).Where(k => !Ipc.ToolInputKeys.Contains(k)).ToList()) input.Remove(key);
        return envelope.ToJsonString(Wire);
    }

    /// Problems go to a small log in the temp folder, which is writable even when an agent runs hooks in a
    /// restricted sandbox user. Only once the hook has reached the pet: with the pet closed it writes nothing.
    internal static void Trace(string line)
    {
        try
        {
            // not Path.GetTempPath() on Windows: it never returns when TMP is longer than MAX_PATH
            var temp = OperatingSystem.IsWindows() && (Environment.GetEnvironmentVariable("TMP") ?? Environment.GetEnvironmentVariable("TEMP")) is { Length: > 0 } t
                ? t : Path.GetTempPath();
            var path = Path.Combine(temp, "aipet-hook.log");
            if (File.Exists(path) && new FileInfo(path).Length > 64 * 1024) File.Delete(path);
            File.AppendAllText(path, $"{DateTime.Now:HH:mm:ss} user={Environment.UserName} pipe={Ipc.Endpoint} {line}{Environment.NewLine}");
        }
        catch { }
    }

    internal static string Str(JsonObject o, string key) => o[key] is JsonValue v && v.TryGetValue(out string s) ? s : null;

    static string ArgValue(string[] args, string name)
    {
        int i = Array.IndexOf(args, name);
        return i >= 0 && i + 1 < args.Length ? args[i + 1].ToLowerInvariant() : null;
    }
}

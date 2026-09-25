using System.Globalization;
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
/// Also: aipet-hook --install|--uninstall claude|codex, aipet-hook --doctor claude|codex [--probe], and
/// aipet-hook --print-plugin-hooks claude|codex (see PluginHooks).
static class Program
{
    public static int Main(string[] args)
    {
        launched = DateTime.UtcNow;
        if (args.Length == 1 && args[0] is "--install" or "--uninstall" or "--doctor" or "--print-plugin-hooks")
        {
            // without an agent this would otherwise run as a hook and wait for an event on stdin
            Console.Error.WriteLine(args[0] switch
            {
                "--doctor" => "usage: aipet-hook --doctor claude|codex [--probe]",
                "--print-plugin-hooks" => PluginHooks.Usage,
                _ => "usage: aipet-hook --install|--uninstall claude|codex",
            });
            return 2;
        }
        if (args.Length >= 2 && args[0] is "--install" or "--uninstall") return Install.Run(args[0], args[1].ToLowerInvariant());
        if (args.Length >= 2 && args[0] == "--doctor") return Doctor.Run(args[1].ToLowerInvariant(), args.Contains("--probe"));
        if (args.Length >= 2 && args[0] == "--print-plugin-hooks") return PluginHooks.Print(args[1].ToLowerInvariant());
        var agent = ArgValue(args, "--agent") ?? "claude";
        NamedPipeClientStream pet = null;
        try
        {
            JsonObject payload;
            string unreadable = null;
            try { payload = ReadPayload(); }
            catch (Exception ex) { payload = new JsonObject(); unreadable = $"{ex.GetType().Name}: {ex.Message}"; }
            // no event came in time: the pet would ignore an empty one anyway
            if (payload == null) return 0;
            // the request is made before the pet is reached, so a failure never takes an instance for nothing. It's
            // only traced once the pet is reached, like everything else.
            string request = null, failed = null;
            try { request = Request(agent == "codex" ? CodexHook.Envelope(payload) : ClaudeHook.Envelope(payload)); }
            catch (Exception ex) { failed = $"{ex.GetType().Name}: {ex.Message}"; }
            // nothing is written before this: with the pet closed the hook leaves no trace. A pet whose instances are
            // all taken is waited for with what the run's budget has left once the reply's time is set aside.
            int busyMs = Math.Min(Ipc.ConnectMs, Ipc.HookBudgetMs - Ipc.TimeoutMs - (int)(DateTime.UtcNow - launched).TotalMilliseconds);
            pet = Ipc.Connect(busyMs, out bool busy);
            if (pet == null)
            {
                // the pet runs (its pipe is there), so this may be written: the one trace of an event it never got
                if (busy) Trace($"{agent} {lastEvent}: the pet's pipe stayed busy for {Math.Max(0, busyMs)} ms, so the event is lost");
                return 0;
            }
            if (unreadable != null) Trace($"{agent}: the event isn't JSON, so the pet gets an empty one: {unreadable}");
            if (failed != null)
            {
                Trace($"{agent} {lastEvent}: {failed}");
                return 0;
            }
            // the reply means the pet has the event
            var reply = Ipc.Ask(pet, Sent(request));
            if (!(JsonNode.Parse(reply ?? "null") is JsonObject r && r[Ipc.Ok] is JsonValue ok && ok.TryGetValue(out bool yes) && yes))
                Trace($"{agent} {lastEvent}: the pet didn't take it: {reply ?? "no reply"}");
        }
        catch (Exception ex) { if (pet != null) Trace($"{agent} {lastEvent}: {ex.GetType().Name}: {ex.Message}"); }
        finally { pet?.Dispose(); }
        return 0;
    }

    internal static string lastEvent = "?";

    /// When Main started, before stdin was read: the event's time `at` (the hook doesn't look at its own or any other
    /// process), and where the run's time budget starts. For a hook registered directly (exec form) that is about when
    /// Claude started it; the plugin's goes through bash, sh and the launcher first, a few ms later on Linux and maybe
    /// tens of ms on Windows (Git Bash), so `at` doesn't order events Claude starts within that of each other.
    internal static DateTime launched;

    /// The event JSON from stdin, or null when none came within Ipc.StdinMs: a hook can never hang, even if stdin
    /// stays open. Throws when what came isn't a JSON object's text.
    static JsonObject ReadPayload()
    {
        var read = Task.Run(() =>
        {
            using var stdin = Console.OpenStandardInput();
            using var reader = new StreamReader(stdin, Encoding.UTF8);  // also drops a UTF-8 byte order mark
            return reader.ReadToEnd();
        });
        if (!read.Wait(Ipc.StdinMs)) return null;
        var input = read.Result;
        return JsonNode.Parse(string.IsNullOrWhiteSpace(input) ? "{}" : Mended(input)) as JsonObject ?? new JsonObject();
    }

    /// The JSON with each escaped lone surrogate (half of an emoji: Node's JSON.stringify, so Claude's, writes one for
    /// text cut in the middle of it) as U+FFFD. System.Text.Json parses one, but then can neither read nor write back
    /// the string or name that holds it, and that would lose the whole event. The text itself is UTF-8 decoded, so it
    /// has no lone surrogate of its own.
    static string Mended(string json)
    {
        StringBuilder mended = null;
        int done = 0, i = json.IndexOf('\\');
        while (i >= 0 && i + 1 < json.Length)
        {
            // a backslash is only valid in a string, where it starts an escape: two characters, or six for \uXXXX
            int len = json[i + 1] != 'u' || Hex(json, i + 2) is not { } c ? 2
                : c is < 0xD800 or > 0xDFFF ? 6
                : c <= 0xDBFF && i + 11 < json.Length && json[i + 6] == '\\' && json[i + 7] == 'u' && Hex(json, i + 8) is >= 0xDC00 and <= 0xDFFF ? 12
                : 0;
            if (len == 0)
            {
                (mended ??= new StringBuilder(json.Length)).Append(json, done, i - done).Append("\\uFFFD");
                done = i + 6;
                len = 6;
            }
            i = i + len < json.Length ? json.IndexOf('\\', i + len) : -1;
        }
        return mended == null ? json : mended.Append(json, done, json.Length - done).ToString();
    }

    /// The UTF-16 code unit of the 4 hex digits at i, or null when there aren't 4.
    static int? Hex(string s, int i) =>
        i + 4 <= s.Length && int.TryParse(s.AsSpan(i, 4), NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out int c) ? c : null;

    /// The envelope around an event (see Ipc); the agent's hook adds its own fields and the payload. at is when the
    /// hook started, which orders events that finish out of order (for Codex only after its own ids, see AgentSessions).
    /// sent is added to the request line when it's sent (Sent).
    internal static JsonObject Envelope(string agent) => new()
    {
        [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = agent, [Ipc.At] = Ipc.UnixTime(launched), [Ipc.Pid] = Environment.ProcessId,
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
    /// big files, say); then the tool call keeps only what the pet reads of it, rather than being dropped whole. The
    /// limit leaves room for sent, which is added later (Sent: ,"sent":1758800000.123 is 22 bytes).
    static string Request(JsonObject envelope)
    {
        var line = envelope.ToJsonString(Wire);
        if (Encoding.UTF8.GetByteCount(line) <= Ipc.MaxRequest - 32 || envelope[Ipc.Payload]?["tool_input"] is not JsonObject input) return line;
        foreach (var key in input.Select(kv => kv.Key).Where(k => !Ipc.ToolInputKeys.Contains(k)).ToList()) input.Remove(key);
        return envelope.ToJsonString(Wire);
    }

    /// The request line with sent, now: the line is made before the pet is reached, and sent is when it got the event.
    static string Sent(string request) =>
        $"{request[..^1]},\"{Ipc.Sent}\":{Ipc.UnixTime(DateTime.UtcNow).ToString(CultureInfo.InvariantCulture)}}}";

    /// Problems go to a small log in the temp folder, which is writable even when an agent runs hooks in a
    /// restricted sandbox user. Only once the hook has reached the pet: with the pet closed it writes nothing.
    /// On Unix the temp folder is shared (/tmp), so each user has a log of their own there, which only they can read.
    /// Another user can put a file or a link at that name first (where fs.protected_regular or protected_symlinks is
    /// off), so the hook never opens a link there (a dangling one would create its target), and writes only to a file
    /// of mode 0600 it opened by that name: a file someone else owns is only writable here through its group or other
    /// bits, and a link put there meanwhile would lead to a file of another name.
    internal static void Trace(string line)
    {
        try
        {
            // not Path.GetTempPath() on Windows: it never returns when TMP is longer than MAX_PATH
            var temp = OperatingSystem.IsWindows() && (Environment.GetEnvironmentVariable("TMP") ?? Environment.GetEnvironmentVariable("TEMP")) is { Length: > 0 } t
                ? t : Path.GetTempPath();
            var path = Path.Combine(temp, OperatingSystem.IsWindows() ? "aipet-hook.log" : $"aipet-hook-{Ipc.EffectiveUid() ?? Environment.UserName}.log");
            if (!OperatingSystem.IsWindows() && new FileInfo(path).LinkTarget != null) return;
            if (File.Exists(path) && new FileInfo(path).Length > 64 * 1024) File.Delete(path);
            var options = new FileStreamOptions { Mode = FileMode.Append, Access = FileAccess.Write, Share = FileShare.ReadWrite | FileShare.Delete };
            // not on Windows, where it throws
            if (!OperatingSystem.IsWindows()) options.UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite;
            using var log = new FileStream(path, options);
            if (!OperatingSystem.IsWindows() && (File.GetUnixFileMode(log.SafeFileHandle) != (UnixFileMode.UserRead | UnixFileMode.UserWrite)
                || new FileInfo($"/proc/self/fd/{log.SafeFileHandle.DangerousGetHandle()}").LinkTarget is { } opened && Path.GetFileName(opened) != Path.GetFileName(path)))
                return;
            log.Write(Encoding.UTF8.GetBytes($"{DateTime.Now:HH:mm:ss} user={Environment.UserName} pipe={Ipc.Endpoint} {line}{Environment.NewLine}"));
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

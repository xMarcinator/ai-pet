using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json.Nodes;
using Xunit;

namespace AiPet.Tests;

/// Where the tests run: a temp endpoint and temp folders, set before anything reads Ipc.Endpoint or Paths.DataDir
/// (both are read once, at type initialization), and given to every hook the tests start. Nothing here reaches the
/// user's pet, config or data, and nothing starts the GUI.
static class TestEnv
{
    public static string Root, Pipe, DataDir, TempDir, CodexHome;

    /// Claude's variables that say where a chat runs: the tests run inside agents too, so a hook never inherits them.
    static readonly string[] AgentEnv = { "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID" };

#pragma warning disable CA2255  // the endpoint and data folder must be set before any test touches Ipc or Paths
    [ModuleInitializer]
#pragma warning restore CA2255
    internal static void Init()
    {
        Root = Directory.CreateTempSubdirectory("aipet-tests-").FullName;
        DataDir = Sub("data");
        TempDir = Sub("tmp");
        CodexHome = Sub("codex");
        // a socket path must stay under 104 bytes, so it isn't under Root (TMPDIR can be long)
        Pipe = OperatingSystem.IsWindows() ? "AiPet-test-" + Guid.NewGuid().ToString("N") : $"/tmp/aipet-t-{Guid.NewGuid().ToString("N")[..8]}.sock";
        foreach (var (name, value) in Vars(Pipe, DataDir, TempDir)) Environment.SetEnvironmentVariable(name, value);
        AppDomain.CurrentDomain.ProcessExit += (_, _) =>
        {
            try { Directory.Delete(Root, true); } catch { }
            if (!OperatingSystem.IsWindows()) try { File.Delete(Pipe); } catch { }
        };
    }

    static string Sub(string name) => Directory.CreateDirectory(Path.Combine(Root, name)).FullName;

    /// A fresh empty folder under Root.
    public static string NewDir(string prefix) => Directory.CreateDirectory(Path.Combine(Root, prefix + "-" + Guid.NewGuid().ToString("N")[..8])).FullName;

    /// An endpoint nothing listens on.
    public static string NoPet() =>
        OperatingSystem.IsWindows() ? "AiPet-test-none-" + Guid.NewGuid().ToString("N") : $"/tmp/aipet-n-{Guid.NewGuid().ToString("N")[..8]}.sock";

    /// What points the pet and a hook at the test's endpoint and folders. TMPDIR is where the hook traces on Unix,
    /// TMP/TEMP on Windows.
    static IEnumerable<(string, string)> Vars(string pipe, string dataDir, string tempDir) => new[]
    {
        ("AIPET_PIPE", pipe), ("AIPET_DATA_DIR", dataDir), ("TMPDIR", tempDir), ("TMP", tempDir), ("TEMP", tempDir),
        ("CODEX_HOME", CodexHome),
    };

    // ------------------------------------------------------------------ the hook
    /// hook/aipet-hook.dll in the output (the csproj copies it there), run with `dotnet` as the antivirus rule asks.
    public static readonly string HookDll = Path.Combine(AppContext.BaseDirectory, "hook", "aipet-hook.dll");

    /// The dotnet that runs these tests: <root>/shared/Microsoft.NETCore.App/<version>/ is its runtime folder.
    public static readonly string Dotnet = FindDotnet();

    static string FindDotnet()
    {
        var exe = OperatingSystem.IsWindows() ? "dotnet.exe" : "dotnet";
        var root = Path.GetFullPath(Path.Combine(RuntimeEnvironment.GetRuntimeDirectory(), "..", "..", ".."));
        if (File.Exists(Path.Combine(root, exe))) return Path.Combine(root, exe);
        if (Environment.GetEnvironmentVariable("DOTNET_HOST_PATH") is { Length: > 0 } host && File.Exists(host)) return host;
        return exe;
    }

    public sealed record HookResult(int Exit, string Stdout, string Stderr, double Started, double Ended);

    /// Starts `dotnet aipet-hook.dll --agent <agent>` with the event on stdin, already closed. Only synchronous I/O,
    /// so a starved thread pool in this process can't hold it up.
    public static (Process Process, double Started) StartHook(string agent, string stdin, string pipe = null, string dataDir = null,
                                                             string tempDir = null, IDictionary<string, string> env = null)
    {
        Assert.True(File.Exists(HookDll), "the hook isn't built at " + HookDll);
        var psi = new ProcessStartInfo(Dotnet)
        {
            UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardInputEncoding = new UTF8Encoding(false),
        };
        psi.ArgumentList.Add(HookDll);
        psi.ArgumentList.Add("--agent");
        psi.ArgumentList.Add(agent);
        foreach (var name in AgentEnv) psi.Environment.Remove(name);
        foreach (var (name, value) in Vars(pipe ?? Pipe, dataDir ?? DataDir, tempDir ?? TempDir)) psi.Environment[name] = value;
        if (env != null) foreach (var (name, value) in env) psi.Environment[name] = value;
        double started = Board.Unix;
        var p = Process.Start(psi);
        p.StandardInput.Write(stdin);
        p.StandardInput.Close();
        return (p, started);
    }

    public static HookResult Finish((Process Process, double Started) run, int timeoutMs = 20000)
    {
        var p = run.Process;
        try
        {
            if (!p.WaitForExit(timeoutMs))
            {
                try { p.Kill(true); } catch { }
                Assert.Fail($"the hook didn't exit within {timeoutMs} ms");
            }
            // the hook prints nothing, so reading after the exit can't fill a pipe and block it
            return new HookResult(p.ExitCode, p.StandardOutput.ReadToEnd(), p.StandardError.ReadToEnd(), run.Started, Board.Unix);
        }
        finally { p.Dispose(); }
    }

    public static HookResult RunHook(string agent, JsonObject payload, string tempDir = null, IDictionary<string, string> env = null) =>
        Finish(StartHook(agent, payload.ToJsonString(), tempDir: tempDir, env: env));

    /// Every file and folder under dir.
    public static string[] Contents(string dir) =>
        Directory.Exists(dir) ? Directory.GetFileSystemEntries(dir, "*", SearchOption.AllDirectories) : Array.Empty<string>();

    // ------------------------------------------------------------------ the pet
    /// A HookServer on the test endpoint that is known to serve: the one before it may still be going away (Windows
    /// keeps a pipe name while any instance is open), so it checks with an event of its own and tries again.
    public static HookServer StartServer(AgentSessions sessions)
    {
        var until = Stopwatch.StartNew();
        while (true)
        {
            var server = new HookServer(sessions);
            server.Start();
            var probe = "probe-" + Guid.NewGuid();
            try
            {
                var reply = Ask(Events.Envelope("claude", Board.Unix, Events.Payload(probe, "SessionStart")));
                if (reply?[Ipc.Ok]?.GetValue<bool>() == true && sessions.Snapshot().Any(e => e.Id == "claude:" + probe)) return server;
            }
            catch (Exception) { }  // the old one's last instance, or nothing yet
            server.Stop();
            if (until.ElapsedMilliseconds > 15000) Assert.Fail("no HookServer could listen on " + Ipc.Endpoint);
            Thread.Sleep(200);
        }
    }

    /// One request over the socket; the reply, or null when the pet closed without one (or isn't there).
    public static JsonObject Ask(JsonObject request) => Ask(request.ToJsonString());

    public static JsonObject Ask(string line)
    {
        using var pipe = Ipc.Connect();
        if (pipe == null) return null;
        var reply = Ipc.Ask(pipe, line);
        return reply == null ? null : JsonNode.Parse(reply) as JsonObject;
    }

    /// A connection that sends nothing (a silent client).
    public static NamedPipeClientStream ConnectSilent()
    {
        var c = new NamedPipeClientStream(".", Ipc.Endpoint, PipeDirection.InOut, PipeOptions.Asynchronous);
        c.Connect(5000);
        return c;
    }

    /// The outcomes hook-events.log recorded for a session, in the order the pet got them. Lines name a session by
    /// its first 13 characters.
    public static List<string> Outcomes(string sid) => Outcomes(File.Exists(Paths.HookEventsLog) ? File.ReadAllLines(Paths.HookEventsLog) : Array.Empty<string>(), sid);

    public static List<string> Outcomes(IEnumerable<string> lines, string sid) =>
        lines.Where(l => l.Contains(" " + sid[..13] + " ")).Select(l => l[(l.LastIndexOf("-> ", StringComparison.Ordinal) + 3)..].Trim()).ToList();
}

/// Hook events as the hook sends them (see Ipc).
static class Events
{
    public static JsonObject Envelope(string agent, double at, JsonObject payload, JsonObject env = null) => new()
    {
        [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = agent, [Ipc.At] = at, [Ipc.Sent] = at + 0.01,
        [Ipc.Pid] = 4242, [Ipc.Env] = env ?? new JsonObject(), [Ipc.Payload] = payload,
    };

    /// A payload: hook_event_name and session_id, a turn_id when given, and extra fields.
    public static JsonObject Payload(string sid, string ev, string turn = null, JsonObject extra = null)
    {
        var p = new JsonObject { ["hook_event_name"] = ev, ["session_id"] = sid };
        if (turn != null) p["turn_id"] = turn;
        if (extra != null)
            foreach (var (k, v) in extra) p[k] = v?.DeepClone();
        return p;
    }

    public static JsonObject Bash(string command, string toolUseId = null, string description = null)
    {
        var input = new JsonObject { ["command"] = command };
        if (description != null) input["description"] = description;
        var o = new JsonObject { ["tool_name"] = "Bash", ["tool_input"] = input };
        if (toolUseId != null) o["tool_use_id"] = toolUseId;
        return o;
    }
}

/// The tests that run a HookServer. The endpoint is one per test process (Ipc.Endpoint is read once) and the
/// starved-pool test changes the whole process's thread pool, so these run alone and one at a time.
[CollectionDefinition(Name, DisableParallelization = true)]
public sealed class PetCollection
{
    public const string Name = "pet";
}

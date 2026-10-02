using System.Diagnostics;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet.Golden;

/// Mode doctor: golden data for the Rust hook's `--doctor claude|codex [--probe]`. src/AiPet.Hook/Doctor.cs doesn't
/// compile outside the hook's project, so each scenario runs the built C# hook (`dotnet aipet-hook.dll`, built here
/// first) in a sandbox of its own, and what it prints and its exit code are recorded. Replayed by
/// rust/crates/aipet-hook/tests/doctor.rs, which runs the Rust hook in the same sandbox.
///
///   dotnet build rust/golden -c Release -p:UseAppHost=false
///   dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll doctor [DIR]
///
/// writes DIR/doctor.json (by default rust/crates/aipet-hook/tests/golden/doctor/).
///
/// A scenario's sandbox is a folder of its own ({root}), which the hook gets as every folder it could read or write:
/// home/ (HOME, USERPROFILE), claude/ (CLAUDE_CONFIG_DIR), codex/ (CODEX_HOME), data/ (AIPET_DATA_DIR), tmp/ (TMPDIR,
/// TMP, TEMP) and bin/, the first folder on PATH, which the fixture's fake `codex` script is in. The pet is at an
/// endpoint of this run's (AIPET_PIPE), never the user's: a C# HookServer when the scenario has one, else nothing
/// listens there. bin/aipet-hook (aipet-hook.cmd on Windows) runs the hook under test: this side's C# hook, the Rust
/// test's Rust hook; the fixtures register it, and the plugin's hook for Linux is a copy of it. Nothing is written
/// outside the sandbox, but for the organisation's managed settings, which only a CI runner may have written (see
/// Managed).
///
/// What differs between runs is a token in what is recorded: the sandbox as {root} (and {root-fwd} with `/`, as the
/// Claude registration writes paths), the pet's endpoint {endpoint} and pid {pid}, the pet's clock {time}, and
/// durations {ms}. In the fixtures' JSON files the sandbox is {root-json}, escaped as a JSON string. A scenario runs
/// until two runs agree, as the registration corpus does, but for the slow one (Codex that never answers), which runs
/// once.
static class DoctorMode
{
    static readonly JsonSerializerOptions Out = new()
    { WriteIndented = true, NewLine = "\n", Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    static readonly string Os = OperatingSystem.IsWindows() ? "windows" : OperatingSystem.IsMacOS() ? "macos" : "linux";
    static readonly char Sep = Path.DirectorySeparatorChar;

    /// The hook the fixtures register for Claude, as --install writes it (with `/`). Codex's command has it relative to
    /// the sandbox, where the doctor runs, so that a command cut short to fit its line doesn't depend on where the
    /// sandbox is.
    static readonly string HookFwd = OperatingSystem.IsWindows() ? "{root-fwd}/bin/aipet-hook.cmd" : "{root-fwd}/bin/aipet-hook";
    static readonly string CodexCommand = OperatingSystem.IsWindows() ? @".\bin\aipet-hook.cmd --agent codex" : "./bin/aipet-hook --agent codex";

    const string PluginRoot = "codex/plugins/cache/aipet/aipet/0.2.0";

    /// Set in the environment to run the scenarios with the organisation's managed Claude settings, which this run
    /// then writes (and removes) where Claude Code reads them: only on a CI runner, never on anyone's own machine.
    const string ManagedVar = "AIPET_TEST_MANAGED_SETTINGS";

    public static int Run(string repo, string data, string[] args)
    {
        if (args.Length > 1) return Program.Usage();
        string dir = args.Length == 1
            ? Path.GetFullPath(args[0])
            : Path.Combine(repo, "rust", "crates", "aipet-hook", "tests", "golden", "doctor");
        // the pet's endpoint, before anything reads Ipc.Endpoint (read once)
        string pet = Endpoint("g"), none = Endpoint("n");
        Environment.SetEnvironmentVariable("AIPET_PIPE", pet);
        if (ManagedPath() is var managed && File.Exists(managed))
            throw new InvalidOperationException($"{managed} is there: the scenarios would record this machine's managed Claude settings");
        var dotnet = Path.GetFileNameWithoutExtension(Environment.ProcessPath) == "dotnet" ? Environment.ProcessPath : "dotnet";
        var hook = BuildHook(repo, dotnet);
        var launcher = File.ReadAllText(Path.Combine(repo, "plugins", "aipet", "native", "aipet-hook.sh")).ReplaceLineEndings("\n");
        var root = Path.GetFullPath(Path.Combine(data, "doctor"));

        var cases = new JsonArray();
        int n = 0;
        foreach (var c in Cases(launcher))
        {
            if (c.Skipped() is { } why)
            {
                Console.Error.WriteLine($"{c.Name}: left out, {why}");
                continue;
            }
            var caseRoot = Path.Combine(root, $"{n++:D2}");
            cases.Add(Settled(caseRoot, c, r => RunCase(r, c, dotnet, hook, pet, none)));
        }
        Directory.CreateDirectory(dir);
        var corpus = new JsonObject
        {
            ["about"] = "aipet-hook --doctor as the C# hook answers each scenario; written by rust/golden's doctor mode "
                        + "(rust/golden/Doctor.cs), replayed by tests/doctor.rs. Don't edit by hand.",
            ["os"] = Os, ["newline"] = Environment.NewLine, ["cases"] = cases,
        };
        var path = Path.Combine(dir, "doctor.json");
        File.WriteAllText(path, corpus.ToJsonString(Out) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"wrote {path} ({cases.Count} scenarios)");
        return 0;
    }

    /// An endpoint of this run's: a pipe name, or a socket path short enough for one.
    static string Endpoint(string what) => OperatingSystem.IsWindows()
        ? $"AiPet-golden-{what}-{Guid.NewGuid():N}"
        : $"/tmp/aipet-{what}-{Guid.NewGuid().ToString("N")[..8]}.sock";

    /// The organisation's settings file the scenarios write (Doctor.ManagedClaudeSettings has the Windows one twice).
    static string ManagedPath() => OperatingSystem.IsWindows() ? @"C:\ProgramData\ClaudeCode\managed-settings.json"
        : OperatingSystem.IsMacOS() ? "/Library/Application Support/ClaudeCode/managed-settings.json"
        : "/etc/claude-code/managed-settings.json";

    /// The hook's dll, built as CI builds it (no apphost: a fresh exe would be scanned on every start).
    static string BuildHook(string repo, string dotnet)
    {
        var project = Path.Combine(repo, "src", "AiPet.Hook", "AiPet.Hook.csproj");
        var (exit, output) = Start(dotnet, ["build", project, "-c", "Release", "--nologo", "-v", "q", "-p:UseAppHost=false"], null, null, 600);
        if (exit != 0) throw new InvalidOperationException("building the hook failed:\n" + output);
        var dll = Path.Combine(repo, "src", "AiPet.Hook", "bin", "Release", "net10.0", "aipet-hook.dll");
        if (!File.Exists(dll)) throw new FileNotFoundException("the hook wasn't built", dll);
        return dll;
    }

    // ------------------------------------------------------------------ the scenarios
    sealed class Case
    {
        public string Name, Agent;
        public bool Probe;
        public readonly SortedDictionary<string, string> Files = new(StringComparer.Ordinal);
        public readonly List<string> Dirs = new(), Executable = new(), HookCopies = new();
        /// The events a running pet got before the doctor runs; null: no pet.
        public List<JsonObject> Pet;
        /// Where it applies: "unix" or "windows"; ci: it starts PowerShell (Windows), which only a CI runner may;
        /// managed: the organisation's settings, which only a CI runner may write; no_codex: nothing may find the
        /// user's Codex where the doctor looks for it (Windows' install folder); slow: runs once.
        public string Only, Managed;
        public bool Ci, NoCodex, Slow;

        public string Skipped()
        {
            string family = OperatingSystem.IsWindows() ? "windows" : "unix";
            if (Only != null && Only != family) return "it is for " + Only;
            if (Ci && OperatingSystem.IsWindows() && Environment.GetEnvironmentVariable("CI") is not { Length: > 0 })
                return "it starts PowerShell, which only a CI runner may (CI isn't set)";
            if (Managed != null && Environment.GetEnvironmentVariable(ManagedVar) != "1")
                return $"it writes the organisation's settings, which only a CI runner may ({ManagedVar} isn't 1)";
            if (NoCodex && File.Exists(InstalledCodex())) return "the doctor would find the Codex installed on this machine";
            return null;
        }

        public JsonObject Setup() => new()
        {
            ["dirs"] = new JsonArray(Dirs.Select(d => (JsonNode)d).ToArray()),
            ["files"] = new JsonObject(Files.Select(f => KeyValuePair.Create(f.Key, (JsonNode)f.Value))),
            ["executable"] = new JsonArray(Executable.Select(e => (JsonNode)e).ToArray()),
            ["hook_copies"] = new JsonArray(HookCopies.Select(h => (JsonNode)h).ToArray()),
        };
    }

    /// Where the doctor looks for Codex when it isn't on PATH (Windows).
    static string InstalledCodex() => OperatingSystem.IsWindows()
        ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "OpenAI", "Codex", "bin", "codex.exe")
        : "/nonexistent";

    static Case Claude(string name, string settings = null, bool probe = false)
    {
        var c = new Case { Name = name, Agent = "claude", Probe = probe };
        c.Dirs.Add("claude");
        if (settings != null) c.Files["claude/settings.json"] = settings.ReplaceLineEndings("\n");
        return c;
    }

    static Case Codex(string name, string toml = null, bool probe = false)
    {
        var c = new Case { Name = name, Agent = "codex", Probe = probe };
        c.Dirs.Add("codex");
        if (toml != null) c.Files["codex/config.toml"] = toml.ReplaceLineEndings("\n");
        return c;
    }

    /// A settings.json with AiPet's hook registered for these events (all of them by default), as --install writes it.
    static string Registered(IEnumerable<string> events = null, string command = null, bool disableAll = false)
    {
        var hooks = new JsonObject();
        foreach (var e in ClaudeConfig.Events.Where(e => events == null || events.Contains(e.Event)))
        {
            var hook = new JsonObject { ["type"] = "command", ["command"] = command ?? HookFwd, ["args"] = new JsonArray("--agent", "claude") };
            hooks[e.Event] = new JsonArray(ClaudeConfig.Group(e, hook));
        }
        var root = new JsonObject();
        if (disableAll) root["disableAllHooks"] = true;
        root["hooks"] = hooks;
        return root.ToJsonString(Out);
    }

    const string PluginSettings = """{ "enabledPlugins": { "aipet@aipet": true } }""";

    /// Claude Code's record of an installed aipet plugin.
    static Case WithClaudePlugin(this Case c)
    {
        c.Files["claude/plugins/installed_plugins.json"] = """{"version":2,"plugins":{"aipet@aipet":[{"scope":"user","version":"0.2.0"}]}}""";
        return c;
    }

    /// A pet running, which has had a chat of each agent already.
    static Case WithPet(this Case c)
    {
        c.Pet = new List<JsonObject> { Seed("claude", "golden-claude-chat"), Seed("codex", "golden-codex-chat") };
        return c;
    }

    static JsonObject Seed(string agent, string session)
    {
        double at = Board.Unix;
        return new JsonObject
        {
            [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = agent, [Ipc.At] = at, [Ipc.Sent] = at + 0.01,
            [Ipc.Pid] = 4242, [Ipc.Env] = new JsonObject(),
            [Ipc.Payload] = new JsonObject { ["hook_event_name"] = "SessionStart", ["session_id"] = session },
        };
    }

    // ------------------------------------------------------------------ a fake codex
    /// A codex on PATH (bin/codex, or bin/codex.cmd on Windows). `--version` says codex-cli 0.0.0-test, and its
    /// app-server answers initialize and hooks/list (with these hooks) at once, then reads its input until it ends:
    /// Windows' cmd.exe can't read a pipe a line at a time. mode: "fails" (app-server exits 1 at once), "version"
    /// (--version exits 3), "silent" (neither answers: --version sleeps, app-server only reads), "not-executable"
    /// (Unix: no x bit).
    static Case WithCodex(this Case c, IEnumerable<JsonObject> hooks = null, string mode = null)
    {
        bool windows = OperatingSystem.IsWindows();
        string version = mode switch
        {
            "version" => windows ? "exit /b 3" : "exit 3",
            "silent" => windows ? "ping -n 30 127.0.0.1 > nul\r\nexit /b 0" : "sleep 30; exit 0",
            _ => windows ? "echo codex-cli 0.0.0-test\r\nexit /b 0" : "echo \"codex-cli 0.0.0-test\"; exit 0",
        };
        string server = mode switch
        {
            "fails" => windows ? "exit /b 1" : "exit 1",
            "silent" => windows ? "more > nul" : "cat > /dev/null",
            _ => windows ? "type \"%~dp0app-server.jsonl\"\r\nmore > nul" : "cat \"${0%/*}/app-server.jsonl\"\ncat > /dev/null",
        };
        if (windows)
            c.Files["bin/codex.cmd"] = "@echo off\r\nif \"%~1\"==\"--version\" (\r\n" + version + "\r\n)\r\nif not \"%~1\"==\"app-server\" exit /b 2\r\n"
                                       + server + "\r\n";
        else
        {
            c.Files["bin/codex"] = "#!/bin/sh\ncase \"$1\" in\n  --version) " + version + " ;;\n  app-server) ;;\n  *) exit 2 ;;\nesac\n" + server + "\n";
            if (mode != "not-executable") c.Executable.Add("bin/codex");
        }
        var list = new JsonObject
        {
            ["id"] = 2,
            ["result"] = new JsonObject
            {
                ["data"] = new JsonArray(new JsonObject
                {
                    ["cwd"] = Native("{root}/home"), ["hooks"] = new JsonArray((hooks ?? []).Select(h => h.DeepClone()).ToArray()),
                }),
            },
        };
        c.Files["bin/app-server.jsonl"] = ("{\"id\":1,\"result\":{}}\n" + list.ToJsonString() + "\n").Replace("{root}", "{root-json}");
        return c;
    }

    static string Native(string path) => path.Replace('/', Sep);

    /// A hook as hooks/list gives it (Codex 0.157).
    static JsonObject Listed(string ev, string command, string trust = "trusted", bool enabled = true, bool plugin = false,
                             bool source = true, string handlerType = "command")
    {
        var o = new JsonObject
        {
            ["key"] = plugin ? $"aipet@aipet:hooks/codex.json:{CodexConfig.Snake(ev)}:0:0" : $"config.toml:{CodexConfig.Snake(ev)}:0:0",
            ["eventName"] = char.ToLowerInvariant(ev[0]) + ev[1..], ["handlerType"] = handlerType, ["command"] = command,
            ["async"] = true, ["timeoutSec"] = 30,
            ["sourcePath"] = !source ? null : plugin ? Native($"{{root}}/{PluginRoot}/hooks/codex.json") : Native("{root}/codex/config.toml"),
            ["source"] = plugin ? "plugin" : "user", ["pluginId"] = plugin ? "aipet@aipet" : null,
            ["enabled"] = enabled, ["isManaged"] = false, ["trustStatus"] = trust,
        };
        return o;
    }

    static IEnumerable<JsonObject> Direct(Func<string, string> trust = null) =>
        CodexConfig.Events.Select(e => Listed(e.Event, CodexCommand, trust?.Invoke(e.Event) ?? "trusted"));

    const string PluginCommand = "sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex";

    static IEnumerable<JsonObject> Plugin(IEnumerable<string> events = null, bool source = true) =>
        (events ?? CodexConfig.PluginEvents).Select(e => Listed(e, PluginCommand, plugin: true, source: source));

    /// The plugin as Codex caches it: its launcher, and (withHook) the hook for this system.
    static Case WithCodexPlugin(this Case c, string launcher, bool withHook = true)
    {
        c.Dirs.Add(PluginRoot + "/hooks");
        c.Files[PluginRoot + "/native/aipet-hook.sh"] = launcher;
        if (!withHook) return c;
        if (OperatingSystem.IsWindows()) c.Files[PluginRoot + "/native/win-x64/aipet-hook.exe"] = "";
        else foreach (var rid in new[] { "linux-x64", "linux-arm64" }) c.HookCopies.Add($"{PluginRoot}/native/{rid}/aipet-hook");
        return c;
    }

    const string PluginToml = "[plugins.\"aipet@aipet\"]\nenabled = true\n";

    /// Handlers in config.toml, as --install writes them: the command as a literal string on Windows (its paths have
    /// backslashes, and no quote), and as a basic one elsewhere (its path is in single quotes, and has no backslash).
    static string TomlHooks(string command, params string[] events)
    {
        var value = OperatingSystem.IsWindows() ? $"'{command}'" : $"\"{command.Replace("\\", "\\\\").Replace("\"", "\\\"")}\"";
        return string.Concat(events.Select(e => $"[[hooks.{e}]]\n[[hooks.{e}.hooks]]\ntype = \"command\"\ncommand = {value}\ntimeout = 30\n"));
    }

    static IEnumerable<Case> Cases(string launcher)
    {
        bool windows = OperatingSystem.IsWindows();
        // ---- Claude Code
        yield return Claude("claude-no-settings");
        yield return new Case { Name = "claude-no-config-folder", Agent = "claude" };
        yield return Claude("claude-unparsable", "{ \"hooks\": ");
        yield return Claude("claude-not-an-object", "[]");
        yield return Claude("claude-not-registered", "{ \"model\": \"opus\" }");
        yield return Claude("claude-registered", Registered());
        yield return Claude("claude-registered-pet", Registered()).WithPet();
        yield return Claude("claude-some-events-and-hooks-off", Registered(["Stop", "SessionStart"], disableAll: true));
        yield return Claude("claude-hook-missing", Registered(command: "{root-fwd}/nowhere/aipet-hook"));
        yield return Claude("claude-legacy-hook-py", """
            { "hooks": { "Stop": [ { "hooks": [ { "type": "command", "args": [ "{root-fwd}/home/.claude/pet/hook.py" ] } ] } ] } }
            """);
        yield return Claude("claude-plugin", PluginSettings).WithClaudePlugin().WithPet();
        yield return Claude("claude-plugin-not-installed", PluginSettings);
        yield return Claude("claude-plugin-and-direct",
            Registered().Replace("{\n  \"hooks\"", "{\n  \"enabledPlugins\": { \"aipet@aipet\": true },\n  \"hooks\"")).WithClaudePlugin();
        foreach (var (name, text) in new[]
                 {
                     ("claude-managed-hooks-off", """{ "disableAllHooks": true }"""),
                     ("claude-managed-only-managed-hooks", """{ "allowManagedHooksOnly": true, "disableAllHooks": true }"""),
                     ("claude-managed-allows", """{ "disableAllHooks": false }"""),
                     ("claude-managed-unreadable", "{ \"disableAllHooks\": "),
                 })
        {
            var c = Claude(name, Registered());
            c.Managed = text;
            yield return c;
        }

        // ---- Codex: without hooks/list
        var legacy = Codex("codex-missing-legacy-hooks",
            "[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = 'C:\\Users\\JOHNSM~1\\AppData\\Local\\AiPet\\hooks\\AIPET-~1.EXE --agent codex'\ntimeout = 30\n");
        legacy.NoCodex = true;
        yield return legacy;
        var missing = Codex("codex-missing-nothing-registered", "model = \"o3\"\n");
        missing.NoCodex = true;
        yield return missing;
        var plugin = Codex("codex-missing-plugin", PluginToml);
        plugin.Dirs.Add(PluginRoot);
        plugin.NoCodex = true;
        yield return plugin;
        var twice = Codex("codex-missing-plugin-and-direct", PluginToml + "\n" + TomlHooks(CodexCommand, "Stop"));
        twice.Dirs.Add(PluginRoot);
        twice.NoCodex = true;
        yield return twice;
        yield return Codex("codex-app-server-fails",
            "[features]\ncodex_hooks = true\n\n" + TomlHooks(CodexCommand, CodexConfig.Events.Select(e => e.Event).ToArray())
            + TomlHooks(windows ? "\"C:\\tools\\x.exe\" %APPDATA%" : "other-tool", "Stop")).WithCodex(mode: "fails");
        var both = Codex("codex-hooks-in-both-files", "[features]\nhooks = false\n\n" + TomlHooks(CodexCommand, "Stop"))
            .WithCodex(Direct());
        both.Files["codex/hooks.json"] = "{ \"hooks\": { \"Stop\": [ { \"hooks\": [ { \"type\": \"command\", \"command\": \"other-tool\" } ] } ] } }\n";
        yield return both;
        var silent = Codex("codex-silent").WithCodex(mode: "silent");
        silent.Slow = true;
        yield return silent;
        yield return Codex("codex-version-fails").WithCodex(Direct(), mode: "version");
        var notExecutable = Codex("codex-not-executable", TomlHooks(CodexCommand, "Stop")).WithCodex(mode: "not-executable");
        notExecutable.Only = "unix";
        yield return notExecutable;

        // ---- Codex: with hooks/list
        var others = new[]
        {
            Listed("Stop", windows ? "\"C:\\tools\\other.exe\" --hook" : "other-tool --hook"),
            Listed("PreToolUse", windows ? @"%USERPROFILE%\tools\lint.exe" : "lint", trust: "untrusted"),
            Listed("SessionStart", null, handlerType: "prompt"),
        };
        yield return Codex("codex-trusted").WithCodex(Direct().Concat(others));
        yield return Codex("codex-trusted-pet").WithCodex(Direct()).WithPet();
        yield return Codex("codex-untrusted").WithCodex(Direct(e => e switch { "Stop" => "untrusted", "PreToolUse" => "modified", _ => "trusted" })
            .Where(h => (string)h["eventName"] != "sessionStart")
            .Select(h => { if ((string)h["eventName"] == "subagentStop") h["enabled"] = false; return h; })
            .Append(Listed("SessionStart", windows ? "\"C:\\x\\aipet-hook.exe\" --agent codex" : "'/x y/aipet-hook' --agent codex")));
        yield return Codex("codex-not-registered").WithCodex(others);
        yield return Codex("codex-plugin").WithCodex(Plugin()).WithCodexPlugin(launcher);
        yield return Codex("codex-plugin-without-a-hook", probe: true).WithCodex(Plugin()).WithCodexPlugin(launcher, withHook: false);
        yield return Codex("codex-plugin-and-direct").WithCodex(Plugin().Concat(Direct())).WithCodexPlugin(launcher);
        yield return Codex("codex-plugin-no-source", probe: true).WithCodex(Plugin(source: false)).WithCodexPlugin(launcher);
        yield return Codex("codex-plugin-some-events").WithCodex(Plugin(["Stop", "PreToolUse"])).WithCodexPlugin(launcher);

        // ---- --probe: through the shell Codex uses (PowerShell, then cmd.exe on Windows)
        var probe = Codex("codex-probe", probe: true).WithCodex(Direct());
        probe.Ci = true;
        yield return probe;
        var probePet = Codex("codex-probe-pet", probe: true).WithCodex(Direct()).WithPet();
        probePet.Ci = true;
        yield return probePet;
        var probeFiles = Codex("codex-probe-files", TomlHooks(CodexCommand, "Stop"), probe: true).WithCodex(mode: "fails").WithPet();
        probeFiles.Ci = true;
        yield return probeFiles;
        var probePlugin = Codex("codex-probe-plugin", probe: true).WithCodex(Plugin()).WithCodexPlugin(launcher).WithPet();
        probePlugin.Only = "unix";
        yield return probePlugin;
    }

    // ------------------------------------------------------------------ running a scenario
    /// A scenario run until two runs agree (the slow one runs once), and every run must: a run that comes out otherwise
    /// than the first stops the generator, since the C# gave two answers. The one exception is a run where the machine
    /// was slow: the doctor says a hook took long (`dotnet` starting the C# hook can take over a second under load, the
    /// limit it holds a hook to without a pet). That run is set aside and said; a fifth one stops it too.
    static JsonObject Settled(string caseRoot, Case c, Func<string, JsonObject> run)
    {
        JsonObject first = null;
        int clean = 0, setAside = 0;
        for (int n = 1; clean < (c.Slow ? 1 : 2); n++)
        {
            var result = run($"{caseRoot}-{n}");
            if (((string)result["stdout"]).Contains("took {ms} ms"))
            {
                Console.Error.WriteLine($"{c.Name}: run {n} set aside, a hook was slow");
                if (++setAside == 5) throw new InvalidOperationException($"{c.Name}: a hook was slow in {setAside} runs");
                continue;
            }
            first ??= result;
            if (result.ToJsonString() != first.ToJsonString())
                throw new InvalidOperationException($"{c.Name}: run {n} came out otherwise than the first, so the C# gives two answers:\n"
                                                    + $"{first.ToJsonString()}\n{result.ToJsonString()}");
            clean++;
        }
        return first;
    }

    static JsonObject RunCase(string root, Case c, string dotnet, string hook, string petEndpoint, string noPet)
    {
        var expand = (string text, bool json) => text
            .Replace("{root-json}", JsonValue.Create(root).ToJsonString()[1..^1])
            .Replace("{root-fwd}", root.Replace('\\', '/'))
            .Replace("{root}", json ? JsonValue.Create(root).ToJsonString()[1..^1] : root);
        foreach (var d in new[] { "home", "data", "tmp", "bin" }.Concat(c.Dirs)) Directory.CreateDirectory(Path.Combine(root, d));
        foreach (var (path, text) in c.Files)
        {
            var file = Path.Combine(root, path);
            Directory.CreateDirectory(Path.GetDirectoryName(file));
            File.WriteAllText(file, expand(text, path.EndsWith(".json") || path.EndsWith(".jsonl")));
        }
        File.WriteAllText(Path.Combine(root, "home", "bash.exe"), "");
        // the hook under test: this side's
        var runsHook = OperatingSystem.IsWindows() ? $"@\"{dotnet}\" \"{hook}\" %*\r\n" : $"#!/bin/sh\nexec '{dotnet}' '{hook}' \"$@\"\n";
        var copies = c.HookCopies.Prepend(OperatingSystem.IsWindows() ? "bin/aipet-hook.cmd" : "bin/aipet-hook").ToList();
        foreach (var copy in copies)
        {
            var file = Path.Combine(root, copy);
            Directory.CreateDirectory(Path.GetDirectoryName(file));
            File.WriteAllText(file, runsHook);
        }
        if (!OperatingSystem.IsWindows())
            foreach (var x in c.Executable.Concat(copies))
                File.SetUnixFileMode(Path.Combine(root, x), UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);

        HookServer server = null;
        string managed = ManagedPath(), managedDir = Path.GetDirectoryName(managed);
        bool madeDir = false;
        try
        {
            if (c.Managed != null)
            {
                madeDir = !Directory.Exists(managedDir);
                Directory.CreateDirectory(managedDir);
                File.WriteAllText(managed, c.Managed);
            }
            JsonArray recent = null;
            if (c.Pet != null)
            {
                server = StartPet();
                foreach (var seed in c.Pet)
                    if (Ask(seed)?[Ipc.Ok]?.GetValue<bool>() != true) throw new InvalidOperationException("the pet didn't take " + seed.ToJsonString());
                var pong = Ask(new JsonObject { [Ipc.V] = Ipc.Version, [Ipc.Kind] = "ping" });
                recent = new JsonArray(((JsonArray)pong[Ipc.Recent]).Select(l => (JsonNode)Tokens((string)l, root, null)).ToArray());
            }
            var endpoint = c.Pet != null ? petEndpoint : noPet;
            var args = new List<string> { hook, "--doctor", c.Agent };
            if (c.Probe) args.Add("--probe");
            var (exit, stdout, stderr) = Doctor(dotnet, args, root, Env(root, endpoint, c.Files.Keys.Any(k => k.StartsWith("bin/codex"))));
            var shown = OperatingSystem.IsWindows() ? endpoint : Path.GetFullPath(endpoint);
            var setup = c.Setup();
            if (recent != null) setup["pet"] = new JsonObject { ["recent"] = recent };
            var result = new JsonObject { ["name"] = c.Name, ["agent"] = c.Agent, ["probe"] = c.Probe };
            if (c.Only != null) result["only"] = c.Only;
            if (c.Ci) result["ci"] = true;
            if (c.NoCodex) result["no_codex"] = true;
            if (c.Managed != null) result["managed"] = c.Managed;
            result["setup"] = setup;
            result["exit"] = exit;
            result["stdout"] = Tokens(stdout, root, shown);
            result["stderr"] = Tokens(stderr, root, shown);
            return result;
        }
        finally
        {
            server?.Stop();
            if (c.Managed != null)
            {
                File.Delete(managed);
                if (madeDir) Directory.Delete(managedDir);
            }
        }
    }

    /// The environment the hook gets: every folder in the sandbox, and PATH with only bin/ and what the fake codex
    /// needs (the system's commands). The Rust test gives the same.
    static List<(string Name, string Value)> Env(string root, string endpoint, bool codex)
    {
        string In(params string[] parts) => Path.Combine(new[] { root }.Concat(parts).ToArray());
        var path = OperatingSystem.IsWindows()
            ? In("bin") + ";" + Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows), "System32")
            : In("bin") + (codex ? ":/usr/bin:/bin" : "");
        return new()
        {
            ("HOME", In("home")), ("USERPROFILE", In("home")), ("XDG_CONFIG_HOME", In("home", ".config")),
            ("XDG_DATA_HOME", In("home", ".local", "share")), ("XDG_STATE_HOME", In("home", ".local", "state")),
            ("CLAUDE_CONFIG_DIR", In("claude")), ("CODEX_HOME", In("codex")), ("AIPET_DATA_DIR", In("data")),
            ("TMPDIR", In("tmp")), ("TMP", In("tmp")), ("TEMP", In("tmp")), ("AIPET_PIPE", endpoint), ("SHELL", "/bin/sh"),
            ("CLAUDE_CODE_GIT_BASH_PATH", In("home", "bash.exe")), ("PATH", path),
        };
    }

    /// `dotnet aipet-hook.dll --doctor ...` in the sandbox: its exit code, stdout and stderr.
    static (int Exit, string Stdout, string Stderr) Doctor(string dotnet, List<string> args, string root, List<(string, string)> env)
    {
        var psi = new ProcessStartInfo(dotnet)
        {
            UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            WorkingDirectory = root, StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8,
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        foreach (var name in new[] { "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID" }) psi.Environment.Remove(name);
        foreach (var (name, value) in env) psi.Environment[name] = value;
        using var p = Process.Start(psi);
        p.StandardInput.Close();
        var o = p.StandardOutput.ReadToEndAsync();
        var e = p.StandardError.ReadToEndAsync();
        if (!p.WaitForExit(120_000))
        {
            try { p.Kill(entireProcessTree: true); } catch { }
            throw new TimeoutException("the doctor didn't finish within 120 s: " + string.Join(" ", args));
        }
        return (p.ExitCode, o.Result, e.Result);
    }

    static (int Exit, string Output) Start(string file, string[] args, string cwd, List<(string, string)> env, int timeoutSec)
    {
        var psi = new ProcessStartInfo(file) { UseShellExecute = false, RedirectStandardOutput = true, RedirectStandardError = true };
        if (cwd != null) psi.WorkingDirectory = cwd;
        foreach (var a in args) psi.ArgumentList.Add(a);
        if (env != null) foreach (var (name, value) in env) psi.Environment[name] = value;
        using var p = Process.Start(psi);
        var o = p.StandardOutput.ReadToEndAsync();
        var e = p.StandardError.ReadToEndAsync();
        if (!p.WaitForExit(timeoutSec * 1000))
        {
            try { p.Kill(entireProcessTree: true); } catch { }
            throw new TimeoutException($"{file} didn't finish within {timeoutSec} s");
        }
        return (p.ExitCode, o.Result + e.Result);
    }

    /// What varies between runs, as tokens: the sandbox, the pet's endpoint, pid and clock, and durations.
    static string Tokens(string text, string root, string endpoint)
    {
        text = text.Replace(root, "{root}").Replace(root.Replace('\\', '/'), "{root-fwd}");
        if (endpoint != null) text = text.Replace(endpoint, "{endpoint}");
        text = Regex.Replace(text, @"\(pid \d+\)", "(pid {pid})");
        text = Regex.Replace(text, @"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}", "{time}");
        return Regex.Replace(text, @"\b\d+ ms\b", "{ms} ms");
    }

    // ------------------------------------------------------------------ the pet
    /// A HookServer on this run's endpoint, known to answer: the one before it may still be going away (Windows keeps a
    /// pipe name while any instance is open), so it pings and tries again.
    static HookServer StartPet()
    {
        var until = Stopwatch.StartNew();
        while (true)
        {
            var server = new HookServer(new AgentSessions());
            server.Start();
            try
            {
                if (Ask(new JsonObject { [Ipc.V] = Ipc.Version, [Ipc.Kind] = "ping" })?[Ipc.Ok]?.GetValue<bool>() == true) return server;
            }
            catch (Exception) { }
            server.Stop();
            if (until.ElapsedMilliseconds > 15000) throw new InvalidOperationException("no HookServer could listen on " + Ipc.Endpoint);
            Thread.Sleep(200);
        }
    }

    static JsonObject Ask(JsonObject request)
    {
        using var pipe = Ipc.Connect();
        if (pipe == null) return null;
        var reply = Ipc.Ask(pipe, request.ToJsonString());
        return reply == null ? null : JsonNode.Parse(reply) as JsonObject;
    }
}

using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet.Golden;

/// Mode registration: golden data for the Rust hook's `--install`/`--uninstall` and the System.Text.Json it
/// reproduces, from the hook's own registration code (Install.cs, linked in). Replayed by
/// rust/crates/aipet-hook/tests/registration.rs.
///
///   dotnet run --project rust/golden -c Release -- registration [DIR]
///
/// writes DIR (by default rust/crates/aipet-hook/tests/golden/registration/):
///   claude.json   each Claude fixture's steps: a config folder set up as the fixture says, then --install or
///                 --uninstall claude, one after another, each with its exit code, what it printed, and every file
///                 left in the case's folder. Also Install.Run's own answers (usage, and a wrong program).
///   json.json     System.Text.Json as the registration uses it: which characters its writers escape and how, JSON
///                 written indented, JsonNode.ToString, DeepEquals, and what JsonNode.Parse and reading a member throw.
/// Task 4 adds the Codex fixtures.
///
/// The hook's path is `{exe}` in the fixtures: each side puts the path it registers in its place, and back. The
/// settings file's path is `{settings}` in what a step printed, and the case's folder `{root}`. A new backup is named
/// by the clock, so after each step it is renamed settings.json.aipet-20000101-<n>.bak (n counts the case's backups):
/// older than the clock, and newer than the fixtures' own backups (1999). The Rust test does the same.
///
/// Fixtures marked "only" need what the other OS can't give (symlinks and modes on Unix, the read-only attribute on
/// Windows). Those marked os_specific give another answer on another OS (Git Bash, which only Windows needs), and the
/// Rust test compares them only on the OS this was written on ("os"). Otherwise only the newline differs
/// (Environment.NewLine, in the files the C# writes and in what it prints).
static class RegistrationMode
{
    const string ExeToken = "{exe}", SettingsToken = "{settings}", RootToken = "{root}";

    /// The hook the fixtures register (Install.Run passes Environment.ProcessPath); ClaudeConfig writes it with "/".
    static readonly string Exe = OperatingSystem.IsWindows()
        ? @"C:\Users\Ann\AppData\Local\AiPetApp\current\aipet-hook.exe"
        : "/home/ann/.local/share/AiPet/hooks/aipet-hook";

    static string ExeWritten => Exe.Replace('\\', '/');

    static readonly JsonSerializerOptions Out = new()
    { WriteIndented = true, NewLine = "\n", Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static int Run(string repo, string data, string[] args)
    {
        if (args.Length > 1) return Program.Usage();
        string dir = args.Length == 1
            ? Path.GetFullPath(args[0])
            : Path.Combine(repo, "rust", "crates", "aipet-hook", "tests", "golden", "registration");
        CheckCatch(repo);
        Directory.CreateDirectory(dir);
        Write(Path.Combine(dir, "claude.json"), ClaudeCorpus(Path.Combine(data, "claude")));
        Write(Path.Combine(dir, "json.json"), JsonCorpus());
        return 0;
    }

    static void Write(string path, JsonNode corpus)
    {
        File.WriteAllText(path, corpus.ToJsonString(Out) + "\n", new UTF8Encoding(false));
        Console.WriteLine($"wrote {path} ({new FileInfo(path).Length / 1024} KB)");
    }

    // ------------------------------------------------------------------ Install.Run's catch
    /// Install.Run turns what ClaudeConfig throws into this line. The fixtures call ClaudeConfig themselves (Install.Run
    /// would register this process, dotnet), so they catch as it does; checked against Install.cs.
    const string CatchLine = "Console.Error.WriteLine($\"Couldn't update {agent} settings: {ex.Message}\");";

    static void CheckCatch(string repo)
    {
        var source = File.ReadAllText(Path.Combine(repo, "src", "AiPet.Hook", "Install.cs"));
        if (!source.Contains(CatchLine)) throw new InvalidOperationException("Install.Run no longer catches with: " + CatchLine);
    }

    /// Runs a step as Install.Run does, with what it prints.
    static (int Exit, string Stdout, string Stderr) Step(Func<int> step, string agent)
    {
        TextWriter output = Console.Out, errors = Console.Error;
        var stdout = new StringWriter();
        var stderr = new StringWriter();
        Console.SetOut(stdout);
        Console.SetError(stderr);
        int exit;
        try
        {
            try { exit = step(); }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Couldn't update {agent} settings: {ex.Message}");
                exit = 1;
            }
        }
        finally
        {
            Console.SetOut(output);
            Console.SetError(errors);
        }
        return (exit, stdout.ToString(), stderr.ToString());
    }

    // ------------------------------------------------------------------ Claude
    sealed class Fixture
    {
        public string Name;
        public string[] Actions = ["install", "install", "uninstall", "uninstall"];
        /// Git Bash found (it only counts on Windows): CLAUDE_CODE_GIT_BASH_PATH names a file. Either way PATH,
        /// ProgramFiles and LOCALAPPDATA name an empty folder, so it isn't found anywhere else.
        public bool GitBash = true;
        /// "unix" or "windows": the fixture needs what only that OS gives.
        public string Only;
        public bool OsSpecific;
        /// The case's files, under its folder (claude/ is CLAUDE_CONFIG_DIR): text, with {exe} for the hook.
        public readonly SortedDictionary<string, string> Files = new(StringComparer.Ordinal);
        /// Files given as bytes (hex).
        public readonly SortedDictionary<string, string> Bytes = new(StringComparer.Ordinal);
        public readonly List<string> Dirs = new();
        /// Symlinks (Unix): path -> target, as written.
        public readonly SortedDictionary<string, string> Links = new(StringComparer.Ordinal);
        /// Unix modes, octal.
        public readonly SortedDictionary<string, string> Modes = new(StringComparer.Ordinal);
        /// Files with the read-only attribute (Windows).
        public readonly List<string> ReadOnly = new();
        /// Whether claude/ is made when no file is in it.
        public bool ConfigDir = true;

        public JsonObject Setup()
        {
            var o = new JsonObject();
            if (Files.Count > 0) o["files"] = Obj(Files);
            if (Bytes.Count > 0) o["bytes"] = Obj(Bytes);
            if (Dirs.Count > 0) o["dirs"] = new JsonArray(Dirs.Select(d => (JsonNode)d).ToArray());
            if (Links.Count > 0) o["links"] = Obj(Links);
            if (Modes.Count > 0) o["modes"] = Obj(Modes);
            if (ReadOnly.Count > 0) o["read_only"] = new JsonArray(ReadOnly.Select(d => (JsonNode)d).ToArray());
            if (!ConfigDir) o["config_dir"] = false;
            return o;
        }

        static JsonObject Obj(SortedDictionary<string, string> d)
        {
            var o = new JsonObject();
            foreach (var (k, v) in d) o[k] = v;
            return o;
        }
    }

    const string S = "claude/settings.json";

    static Fixture F(string name, string settings = null, params string[] actions)
    {
        var f = new Fixture { Name = name };
        if (settings != null) f.Files[S] = settings;
        if (actions.Length > 0) f.Actions = actions;
        return f;
    }

    static string Hex(byte[] bytes) => Convert.ToHexStringLower(bytes);

    /// An installed aipet plugin, as Claude Code records it.
    const string InstalledPlugins = """{"version":2,"plugins":{"aipet@aipet":[{"scope":"user","version":"0.2.0"}]}}""";
    const string InstalledList = "claude/plugins/installed_plugins.json";
    const string Enabled = "{\n  \"enabledPlugins\": { \"aipet@aipet\": true }\n}\n";

    static IEnumerable<Fixture> ClaudeFixtures(string current)
    {
        // ---- nothing yet
        yield return F("missing");
        yield return new Fixture { Name = "config-folder-missing", ConfigDir = false, Actions = ["uninstall", "install", "uninstall"] };
        yield return F("empty", "", "install", "uninstall");
        yield return F("blank", " \r\n\t", "uninstall", "install", "uninstall");
        yield return F("empty-object", "{}", "uninstall", "install", "uninstall");

        // ---- other settings: kept, but written as System.Text.Json writes them
        yield return F("other-settings", """
            {
              "$schema": "https://json.schemastore.org/claude-code-settings.json",
              "env": {
                "ANTHROPIC_API_KEY": "secret",
                "WIN": "C:\\tools;D:\\x y",
                "TAB": "a\tb", "NL": "a\nb\r\n", "QUOTE": "say \"hi\"", "HTML": "<a href='x'>&amp;</a> + `x`",
                "TEXT": "é ø 中 😀 € \u00a0 \u200b \u2028 \ufeff \u0085 \u007f \u0001 \u001f ÿ",
                "ESCAPED": "\u00e9\ud83d\ude00\u002f\/\u0041\u00E9",
                "PRIVATE": "\ue000 \uffff \ufffe \ufffd \u0378 \u061c \u00ad \u3000 \u2000"
              },
              "permissions": { "allow": ["Bash(git status:*)", "Read(~/.zshrc)"], "deny": [], "defaultMode": "acceptEdits" },
              "model": "opus",
              "numbers": [0, -0, 1.0, 1e3, 1E-7, -12.50, 123456789012345678901234567890, 0.1, 1e400],
              "flags": [true, false, null],
              "nested": { "a": { "b": { "c": [[], {}] } }, "empty": {} },
              "statusLine": { "type": "command", "command": "~/.claude/statusline.sh", "padding": 0 },
              "": "an empty name",
              "name with \"quotes\", \\ and é": 1
            }
            """, "install", "uninstall");
        yield return F("crlf", "{\r\n  \"model\": \"opus\",\r\n  \"hooks\": {\r\n    \"Stop\": [ { \"hooks\": [ { \"type\": \"command\", \"command\": \"C:/Tools/aipet-hook.exe\" } ] } ]\r\n  }\r\n}\r\n",
            "install", "uninstall");

        // ---- other tools' hooks, and older hooks of AiPet's
        yield return F("other-tools-hooks", """
            {
              "hooks": {
                "PreToolUse": [
                  { "matcher": "Bash", "hooks": [ { "type": "command", "command": "~/bin/guard.sh", "timeout": 10 } ] }
                ],
                "Stop": [
                  { "hooks": [ { "type": "command", "command": "notify-send done" } ] }
                ],
                "CustomEvent": [ { "hooks": [ { "type": "command", "command": "x" } ] } ]
              }
            }
            """, "install", "install", "uninstall");
        yield return F("legacy", """
            {
              "hooks": {
                "Stop": [
                  { "hooks": [
                    { "type": "command", "command": "python3 ~/.claude/pet/hook.py stop" },
                    { "type": "command", "command": "python3", "args": ["C:\\Users\\ann\\.claude\\pet\\hook.py", "--x"] },
                    { "type": "command", "command": "C:/Users/ann/.claude/ClaudePet/ClaudePet.exe --stop" },
                    { "type": "command", "command": "C:/Users/ann/claudepet.EXE" },
                    { "type": "command", "command": "python3 ~/.claude/pet/hook.pyc" },
                    { "type": "command", "command": "python3 ~/other/pet/hook.py" },
                    { "type": "command", "command": "python3 \"/home/ann/.CLAUDE/Pet/HOOK.PY\"" },
                    { "type": "command", "command": "python3 '/home/ann/.claude/pet/hook.py'" },
                    { "type": "command", "command": "python3 /home/ann/.claude/pet/hook.py\u00a0x" },
                    { "type": "command", "command": "python3 /home/ann/.claude/pet/hook.py\u3000x" },
                    { "type": "command", "command": "python3 /home/ann/.claude/pet/hook.py\nx" },
                    { "type": "command", "command": "python3 /home/ann/.claude/pet/hook.py-x" },
                    { "type": "command", "command": "python3 x.claude/pet/hook.py" },
                    { "type": "command", "command": "python3 /home/ann/.claude/pet/hoo\u212a.py" },
                    { "type": "command", "command": "C:\\old\\AIPET-HOOK.EXE --agent claude" }
                  ] }
                ],
                "Notification": [
                  { "hooks": [ { "type": "command", "command": "python3 /home/ann/.claude/pet/hook.py notification" } ] }
                ]
              }
            }
            """, "install", "uninstall");
        yield return F("older-aipet", """
            {
              "hooks": {
                "SessionStart": [ { "hooks": [ { "type": "command", "command": "C:/Users/Ann/AppData/Local/AiPet/aipet-hook.exe", "args": ["--agent", "claude"], "timeout": 5 } ] } ],
                "PreToolUse": [ { "matcher": "Bash", "hooks": [
                  { "type": "command", "command": "C:/Users/Ann/AppData/Local/AiPet/aipet-hook.exe", "args": ["--agent", "claude"], "timeout": 5, "async": true },
                  { "type": "command", "command": "~/bin/log-tool.sh" } ] } ],
                "Stop": [
                  { "hooks": [ { "type": "command", "command": "/opt/aipet/aipet-hook --agent claude" } ] },
                  { "hooks": [ { "type": "command", "command": "afplay done.aiff" } ] }
                ]
              },
              "model": "sonnet"
            }
            """, "install", "uninstall");
        yield return F("current", current, "install", "uninstall");
        yield return F("current-reordered", Reordered(), "install", "uninstall");
        yield return F("ours-not-last", """
            {"hooks":{"Stop":[{"hooks":[{"type":"command","command":"{exe}","args":["--agent","claude"],"timeout":5}]},{"hooks":[{"type":"command","command":"afplay done.aiff"}]}]}}
            """, "install", "uninstall");
        yield return F("matcher", """{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"{exe}","args":["--agent","claude"],"timeout":5,"async":true}]}]}}""",
            "install");

        // ---- "hooks" and its events in other shapes
        yield return F("hooks-empty", """{"hooks": {}}""", "uninstall", "install", "uninstall");
        yield return F("hooks-empty-install", """{"hooks": {}}""", "install", "uninstall");
        yield return F("hooks-array", """{"hooks": [], "model": "x"}""", "uninstall", "install", "uninstall");
        yield return F("hooks-null", """{"a": 1, "hooks": null, "b": 2}""", "install", "uninstall");
        yield return F("events-not-arrays", """{"hooks":{"Stop":{},"Notification":null,"PreToolUse":"x","Other":5,"SessionEnd":[]}}""", "install", "uninstall");
        yield return F("user-empty-arrays", """{"hooks":{"Stop":[],"X":[]}}""", "uninstall", "install");
        yield return F("odd-groups", """{"hooks":{"Stop":["x",null,5,[],{"matcher":"y"},{"hooks":"z"},{"hooks":[]},{"hooks":[null,{"type":"command","command":"{exe}"}]}]}}""",
            "install", "uninstall");
        yield return F("hook-not-object", """{"hooks":{"Stop":[{"hooks":["x"]}]}}""", "install", "uninstall");
        yield return F("hook-array", """{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"},[]]}]}}""", "install", "uninstall");
        yield return F("command-not-string", """
            {"hooks":{"Stop":[{"hooks":[
              {"type":"command","command":5},
              {"type":"command","command":{"x":"aipet-hook"}},
              {"type":"command","command":["/x/.claude/pet/hook.py"]},
              {"type":"command","command":["/x/.claude/pet/hook.py'"]},
              {"type":"command","command":true},
              {"command":null,"args":[null,5,"~/.claude/pet/hook.py"]},
              {"command":"x","args":[{"a":"/.claude/pet/hook.py"}]},
              {"command":"x","args":"~/.claude/pet/hook.py"},
              {"command":"x","args":[["/x/.claude/pet/hook.py'"]]}
            ]}]}}
            """, "install", "uninstall");

        // ---- not an object, or not JSON
        foreach (var (name, text) in new[] { ("array", "[]"), ("null", "null"), ("string", "\"x\""), ("number", "5") })
            yield return F("not-an-object-" + name, text, "uninstall", "install");
        foreach (var (name, text) in new[]
                 {
                     ("truncated", "{\n  \"model\": \"opus\",\n"),
                     ("trailing-comma", "{\n  \"model\": \"opus\",\n  \"env\": { \"A\": \"é\", },\n}\n"),
                     ("comment", "// my settings\n{ \"model\": \"opus\" }\n"),
                     ("single-quotes", "{'model': 'opus'}"),
                     ("two-values", "{} {}"),
                 })
            yield return F("unparsable-" + name, text, "install", "uninstall");

        // ---- a name given twice: System.Text.Json refuses it once that object is read
        yield return F("duplicate-root", """{"model":"a","model":"b"}""", "install", "uninstall");
        yield return F("duplicate-unread", """{"env":{"A":"1","A":"2"}}""", "install", "uninstall");
        // nothing of AiPet's to take out, so DeepEquals reads it all
        yield return F("duplicate-compared", """{"env":{"A":"1","A":"2"}}""", "uninstall");
        yield return F("duplicate-event", """{"hooks":{"Stop":[],"Stop":[]}}""", "install", "uninstall");
        yield return F("duplicate-in-group", """{"hooks":{"Stop":[{"hooks":[],"hooks":[]}]}}""", "install", "uninstall");
        yield return F("duplicate-in-hook", """{"hooks":{"Stop":[{"hooks":[{"command":"a","command":"b"}]}]}}""", "install", "uninstall");
        yield return F("duplicate-enabled-plugins", """{"enabledPlugins":{"x@y":true,"x@y":false}}""", "install", "uninstall");

        // ---- the aipet plugin
        var plugin = F("plugin-installed", Enabled, "install", "uninstall");
        plugin.Files[InstalledList] = InstalledPlugins;
        yield return plugin;
        var withHooks = F("plugin-installed-with-hooks", ReplaceFirst(current, "{", "{\"enabledPlugins\":{\"aipet@aipet\":true},"),
            "install", "install", "uninstall");
        withHooks.Files[InstalledList] = InstalledPlugins;
        yield return withHooks;
        var cache = F("plugin-cache", Enabled, "install");
        cache.Dirs.Add("claude/plugins/cache/aipet/aipet/0.2.0");
        yield return cache;
        var emptyCache = F("plugin-cache-empty", Enabled, "install");
        emptyCache.Dirs.Add("claude/plugins/cache/aipet/aipet");
        yield return emptyCache;
        yield return F("plugin-not-installed", Enabled, "install", "uninstall");
        var disabled = F("plugin-disabled", "{ \"enabledPlugins\": { \"aipet@aipet\": false } }", "install");
        disabled.Files[InstalledList] = InstalledPlugins;
        yield return disabled;
        foreach (var (name, list) in new[]
                 {
                     ("empty-list", """{"plugins":{"aipet@aipet":[]}}"""),
                     ("object", """{"plugins":{"aipet@aipet":{"x":1}}}"""),
                     ("null", """{"plugins":{"aipet@aipet":null}}"""),
                     ("number", """{"plugins":{"aipet@aipet":0}}"""),
                     ("broken", "{broken"),
                     ("array", "[1]"),
                     ("plugins-array", """{"plugins":[]}"""),
                     ("duplicate", """{"plugins":{"aipet@aipet":[1],"aipet@aipet":[2]}}"""),
                 })
        {
            var f = F("plugin-list-" + name, Enabled, "install");
            f.Files[InstalledList] = list;
            yield return f;
        }
        var other = F("plugin-other-marketplace", "{\"enabledPlugins\":{\"aipet@dev\":true}}", "install");
        other.Dirs.Add("claude/plugins/cache/dev/aipet/1.0.0");
        yield return other;
        var notBool = F("plugin-value-not-bool", "{\"enabledPlugins\":{\"aipet@aipet\":\"true\"}}", "install");
        notBool.Files[InstalledList] = InstalledPlugins;
        yield return notBool;
        var several = F("plugin-several", "{\"enabledPlugins\":{\"other@x\":true,\"aipet@a\":false,\"aipet@b\":true,\"aipet@c\":true}}", "install");
        several.Dirs.Add("claude/plugins/cache/b/aipet/1");
        several.Dirs.Add("claude/plugins/cache/c/aipet/1");
        yield return several;
        var notObject = F("plugin-enabled-not-object", "{\"enabledPlugins\":[\"aipet@aipet\"]}", "install");
        notObject.Files[InstalledList] = InstalledPlugins;
        yield return notObject;
        var noBash = F("plugin-no-git-bash", Enabled, "install", "install", "uninstall");
        noBash.Files[InstalledList] = InstalledPlugins;
        noBash.GitBash = false;
        noBash.OsSpecific = true;
        yield return noBash;

        // ---- backups, encodings, leftovers
        var backups = F("backups", "{}", "install", "uninstall", "install");
        for (int i = 1; i <= 4; i++) backups.Files[$"claude/settings.json.aipet-19990101-00000{i}.bak"] = $"backup {i}";
        backups.Files["claude/settings.json.bak"] = "not ours";
        backups.Files["claude/settings.json.aipet-x.txt"] = "not a backup";
        backups.Files["claude/other.json.aipet-19990101-000009.bak"] = "another file's";
        yield return backups;
        var bom = F("utf8-bom", null, "install");
        bom.Bytes[S] = Hex([0xEF, 0xBB, 0xBF, .. Encoding.UTF8.GetBytes("{\"model\":\"x\"}")]);
        yield return bom;
        var utf16 = F("utf16", null, "install");
        utf16.Bytes[S] = Hex([0xFF, 0xFE, .. Encoding.Unicode.GetBytes("{\"model\":\"é\"}")]);
        yield return utf16;
        var invalid = F("invalid-utf8", null, "install");
        invalid.Bytes[S] = Hex([.. Encoding.UTF8.GetBytes("{\"model\":\""), 0xC3, 0x28, 0xFF, .. Encoding.UTF8.GetBytes("\"}")]);
        yield return invalid;
        var leftover = F("leftover-temp-file", "{\"model\":\"x\"}", "install");
        leftover.Files["claude/settings.json.aipet-tmp"] = "half a file";
        yield return leftover;

        // ---- Unix: modes and symlinks
        var mode = F("mode-kept", "{\"env\":{\"TOKEN\":\"secret\"}}", "install", "uninstall");
        mode.Modes[S] = "640";
        mode.Only = "unix";
        yield return mode;
        var groupWrite = F("mode-group-writable", "{}", "install");
        groupWrite.Modes[S] = "660";
        groupWrite.Only = "unix";
        yield return groupWrite;
        var tmpMode = F("mode-leftover-temp-file", "{}", "install");
        tmpMode.Modes[S] = "600";
        tmpMode.Files["claude/settings.json.aipet-tmp"] = "half a file";
        tmpMode.Modes["claude/settings.json.aipet-tmp"] = "646";
        tmpMode.Only = "unix";
        yield return tmpMode;
        var link = new Fixture { Name = "symlink", Actions = ["install", "install", "uninstall"], Only = "unix" };
        link.Files["dotfiles/claude-settings.json"] = "{\"model\":\"x\"}";
        link.Modes["dotfiles/claude-settings.json"] = "644";
        link.Links[S] = "../dotfiles/claude-settings.json";
        yield return link;
        var dangling = new Fixture { Name = "symlink-dangling", Actions = ["uninstall", "install", "uninstall"], Only = "unix" };
        dangling.Dirs.Add("dotfiles");
        dangling.Links[S] = "../dotfiles/none.json";
        yield return dangling;
        var chain = new Fixture { Name = "symlink-chain", Actions = ["install"], Only = "unix" };
        chain.Files["dotfiles/real.json"] = "{}";
        chain.Links[S] = "link2.json";
        chain.Links["claude/link2.json"] = "../dotfiles/./real.json";
        yield return chain;
        var loop = new Fixture { Name = "symlink-loop", Actions = ["install"], Only = "unix" };
        loop.Links[S] = "settings.json";
        yield return loop;

        // ---- Windows: a file that can't be replaced
        var readOnly = F("read-only", "{\"model\":\"x\"}", "install", "uninstall");
        readOnly.ReadOnly.Add(S);
        readOnly.Only = "windows";
        yield return readOnly;
    }

    static string ReplaceFirst(string s, string what, string with)
    {
        int at = s.IndexOf(what, StringComparison.Ordinal);
        return s[..at] + with + s[(at + what.Length)..];
    }

    /// AiPet's current hooks, but the events in the other order, each object's members too, and the timeout as 5.0:
    /// DeepEquals takes that for the same.
    static string Reordered()
    {
        var events = ClaudeConfig.Events.Reverse().Select(e =>
        {
            var hook = (e.Async ? "\"async\":true," : "") + "\"timeout\":5.0,\"args\":[\"--agent\",\"claude\"],\"command\":\"{exe}\",\"type\":\"command\"";
            var group = (e.Matcher ? "\"matcher\":\"*\"," : "") + "\"hooks\":[{" + hook + "}]";
            return $"\"{e.Event}\":[{{{group}}}]";
        });
        return "{\"hooks\":{" + string.Join(",", events) + "}}";
    }

    static JsonNode ClaudeCorpus(string root)
    {
        // what --install writes into an empty folder: the "current" fixture
        string current;
        using (new Env(("CLAUDE_CONFIG_DIR", Path.Combine(root, "current"))))
        {
            var (exit, _, err) = Step(() => ClaudeConfig.Install(Exe), "claude");
            if (exit != 0) throw new InvalidOperationException(err);
            current = File.ReadAllText(ClaudeConfig.Settings).Replace(ExeWritten, ExeToken, StringComparison.Ordinal);
        }
        var cases = new JsonArray();
        int n = 0;
        foreach (var f in ClaudeFixtures(current))
        {
            if (f.Only == "unix" && OperatingSystem.IsWindows() || f.Only == "windows" && !OperatingSystem.IsWindows()) continue;
            cases.Add(Settled(Path.Combine(root, $"{n++:D2}"), f));
        }
        var names = cases.Select(c => (string)c["name"]).ToList();
        if (names.Distinct().Count() != names.Count) throw new InvalidOperationException("two fixtures have the same name");
        Console.WriteLine($"claude: {cases.Count} fixtures, {cases.Sum(c => c["steps"].AsArray().Count)} steps");

        return new JsonObject
        {
            ["about"] = "Written by rust/golden (dotnet run --project rust/golden -c Release -- registration) with the hook's "
                        + "Install.cs; replayed by rust/crates/aipet-hook/tests/registration.rs. Don't edit by hand.",
            ["os"] = OperatingSystem.IsWindows() ? "windows" : OperatingSystem.IsMacOS() ? "macos" : "linux",
            ["newline"] = Environment.NewLine,
            ["cases"] = cases,
            ["run"] = RunCases(Path.Combine(root, "run")),
        };
    }

    /// Install.Run's own answers, before and around ClaudeConfig: usage, and --install refused because what runs it
    /// (this process: dotnet) isn't aipet-hook, which it prints as {exe}.
    static JsonArray RunCases(string root)
    {
        var config = Directory.CreateDirectory(Path.Combine(root, "claude")).FullName;
        var cases = new JsonArray();
        using var env = new Env(("CLAUDE_CONFIG_DIR", config));
        foreach (var (action, agent) in new[]
                 {
                     ("--install", "cursor"), ("--uninstall", "gemini"), ("--install", ""), ("--install", "claude"),
                     ("--install", "codex"), ("--uninstall", "claude"),
                 })
        {
            var (exit, stdout, stderr) = Step(() => Install.Run(action, agent), agent);
            if (Directory.EnumerateFileSystemEntries(config).Any()) throw new InvalidOperationException($"{action} {agent} wrote something");
            string Norm(string s) => s.Replace(Environment.ProcessPath!, ExeToken, StringComparison.Ordinal)
                .Replace(ClaudeConfig.Settings, SettingsToken, StringComparison.Ordinal);
            cases.Add(new JsonObject
            {
                ["args"] = new JsonArray(action, agent), ["exit"] = exit, ["stdout"] = Norm(stdout), ["stderr"] = Norm(stderr),
            });
        }
        return cases;
    }

    /// A case run until two runs in a row agree: a virus scanner can hold a file just written for a moment (Windows)
    /// and fail a step that doesn't fail otherwise.
    static JsonObject Settled(string caseRoot, Fixture f)
    {
        string last = null;
        for (int attempt = 1; attempt <= 5; attempt++)
        {
            var result = RunCase($"{caseRoot}-{attempt}", f);
            var json = result.ToJsonString();
            if (json == last) return result;
            last = json;
        }
        throw new InvalidOperationException($"{f.Name}: no two runs in a row agree");
    }

    static JsonObject RunCase(string caseRoot, Fixture f)
    {
        Directory.CreateDirectory(caseRoot);
        var config = Path.Combine(caseRoot, "claude");
        // not recorded: a bash.exe to find, and an empty folder for PATH and the install folders
        var harness = Directory.CreateDirectory(Path.Combine(caseRoot, "harness")).FullName;
        var nobin = Directory.CreateDirectory(Path.Combine(harness, "nobin")).FullName;
        var bash = Path.Combine(harness, "bash.exe");
        File.WriteAllText(bash, "");
        if (f.ConfigDir) Directory.CreateDirectory(config);
        foreach (var d in f.Dirs) Directory.CreateDirectory(Path.Combine(caseRoot, d));
        foreach (var (p, text) in f.Files)
        {
            var path = Path.Combine(caseRoot, p);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, text.Replace(ExeToken, ExeWritten, StringComparison.Ordinal));
        }
        foreach (var (p, hex) in f.Bytes) File.WriteAllBytes(Path.Combine(caseRoot, p), Convert.FromHexString(hex));
        foreach (var (p, target) in f.Links) File.CreateSymbolicLink(Path.Combine(caseRoot, p), target);
        if (!OperatingSystem.IsWindows())
            foreach (var (p, mode) in f.Modes) File.SetUnixFileMode(Path.Combine(caseRoot, p), (UnixFileMode)Convert.ToInt32(mode, 8));
        foreach (var p in f.ReadOnly) File.SetAttributes(Path.Combine(caseRoot, p), File.GetAttributes(Path.Combine(caseRoot, p)) | FileAttributes.ReadOnly);

        var steps = new JsonArray();
        int backups = 0;
        using (new Env(("CLAUDE_CONFIG_DIR", config), ("CLAUDE_CODE_GIT_BASH_PATH", f.GitBash ? bash : null), ("PATH", nobin),
                   ("ProgramFiles", nobin), ("LOCALAPPDATA", nobin)))
        {
            foreach (var action in f.Actions)
            {
                var before = Backups(config).ToHashSet();
                var (exit, stdout, stderr) = Step(() => action == "install" ? ClaudeConfig.Install(Exe) : ClaudeConfig.Uninstall(), "claude");
                foreach (var name in Backups(config).Where(b => !before.Contains(b)).Order(StringComparer.Ordinal))
                    File.Move(Path.Combine(config, name), Path.Combine(config, $"settings.json.aipet-20000101-{++backups:D6}.bak"));
                string Norm(string s) => s.Replace(ClaudeConfig.Settings, SettingsToken, StringComparison.Ordinal)
                    .Replace(caseRoot, RootToken, StringComparison.Ordinal);
                steps.Add(new JsonObject
                {
                    ["action"] = action, ["exit"] = exit, ["stdout"] = Norm(stdout), ["stderr"] = Norm(stderr),
                    ["files"] = Tree(caseRoot),
                });
            }
        }
        // a read-only file (and its backups, which keep the attribute) would stop the temp folder's removal
        if (OperatingSystem.IsWindows())
            foreach (var file in Directory.EnumerateFiles(caseRoot, "*", SearchOption.AllDirectories)) File.SetAttributes(file, FileAttributes.Normal);

        var c = new JsonObject { ["name"] = f.Name };
        if (f.Only != null) c["only"] = f.Only;
        if (f.OsSpecific) c["os_specific"] = true;
        if (!f.GitBash) c["git_bash"] = false;
        c["setup"] = f.Setup();
        c["steps"] = steps;
        return c;
    }

    /// The backups a step can make: settings.json.aipet-<time>.bak.
    static IEnumerable<string> Backups(string config) =>
        Directory.Exists(config)
            ? Directory.EnumerateFiles(config).Select(Path.GetFileName).Where(n => Regex.IsMatch(n, @"^settings\.json\.aipet-\d{8}-\d{6}\.bak$"))
            : [];

    /// Everything under the case's folder but the harness: a folder, a symlink (not followed), or a file's text
    /// ({exe} for the hook) or bytes, and its mode on Unix.
    static JsonObject Tree(string caseRoot)
    {
        var entries = new SortedDictionary<string, JsonObject>(StringComparer.Ordinal);
        void Walk(string dir)
        {
            foreach (var path in Directory.EnumerateFileSystemEntries(dir))
            {
                var rel = Path.GetRelativePath(caseRoot, path).Replace('\\', '/');
                if (rel == "harness") continue;
                var info = new FileInfo(path);
                if (info.LinkTarget != null)
                {
                    entries[rel] = new JsonObject { ["link"] = info.LinkTarget };
                    continue;
                }
                if (Directory.Exists(path))
                {
                    entries[rel] = new JsonObject { ["dir"] = true };
                    Walk(path);
                    continue;
                }
                var bytes = File.ReadAllBytes(path);
                var e = new JsonObject();
                string text = null;
                try { text = new UTF8Encoding(false, throwOnInvalidBytes: true).GetString(bytes); } catch (ArgumentException) { }
                if (text != null && !text.StartsWith('\uFEFF')) e["text"] = text.Replace(ExeWritten, ExeToken, StringComparison.Ordinal);
                else e["hex"] = Hex(bytes);
                if (!OperatingSystem.IsWindows()) e["mode"] = Convert.ToString((int)File.GetUnixFileMode(path), 8);
                entries[rel] = e;
            }
        }
        Walk(caseRoot);
        var o = new JsonObject();
        foreach (var (k, v) in entries) o[k] = v;
        return o;
    }

    /// Sets environment variables (null takes one out) and puts the old values back.
    sealed class Env : IDisposable
    {
        readonly (string Name, string Value)[] old;

        public Env(params (string Name, string Value)[] vars)
        {
            old = vars.Select(v => (v.Name, Environment.GetEnvironmentVariable(v.Name))).ToArray();
            foreach (var (name, value) in vars) Environment.SetEnvironmentVariable(name, value);
        }

        public void Dispose()
        {
            foreach (var (name, value) in old) Environment.SetEnvironmentVariable(name, value);
        }
    }

    // ------------------------------------------------------------------ System.Text.Json
    /// ClaudeConfig's writer, and the default one: JsonNode.ToString's, which IsOurs reads a command that isn't a
    /// string with.
    static readonly JsonSerializerOptions Relaxed = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly JsonSerializerOptions Default = new();

    static JsonNode JsonCorpus()
    {
        return new JsonObject
        {
            ["about"] = "Written by rust/golden (dotnet run --project rust/golden -c Release -- registration) with System.Text.Json; "
                        + "replayed by rust/crates/aipet-hook (src/json_out.rs). Don't edit by hand.",
            ["escaped"] = new JsonObject { ["relaxed"] = EscapedRanges(Relaxed), ["default"] = EscapedRanges(Default) },
            ["escapes"] = Escapes(),
            ["writes"] = Writes(),
            ["to_string"] = ToStrings(),
            ["deep_equals"] = DeepEquals(),
            ["parse"] = Parses(),
        };
    }

    /// The scalar values a writer escapes, as [first, last] ranges (a range may span the surrogates, which aren't
    /// scalar values).
    static JsonArray EscapedRanges(JsonSerializerOptions options)
    {
        var ranges = new JsonArray();
        int start = -1;
        for (int c = 0; c <= 0x110000; c++)
        {
            if (c is >= 0xD800 and <= 0xDFFF) continue;
            bool escaped = c <= 0x10FFFF && Escaped(char.ConvertFromUtf32(c), options);
            if (escaped && start < 0) start = c;
            if (!escaped && start >= 0)
            {
                ranges.Add(new JsonArray(start, c is 0xE000 ? 0xD7FF : c - 1));
                start = -1;
            }
        }
        return ranges;
    }

    static bool Escaped(string s, JsonSerializerOptions options) => JsonValue.Create(s).ToJsonString(options) != "\"" + s + "\"";

    /// How an escaped character is written.
    static JsonArray Escapes()
    {
        var chars = Enumerable.Range(0, 0x20).Concat([0x22, 0x26, 0x27, 0x2B, 0x2F, 0x3C, 0x3E, 0x5C, 0x60, 0x7F, 0x85, 0xA0, 0xAD, 0xE9, 0x378,
            0x2028, 0x3000, 0xE000, 0xFEFF, 0xFFFD, 0xFFFF, 0x1F600, 0x10000, 0x10FFFF]);
        var list = new JsonArray();
        foreach (var c in chars)
        {
            var s = char.ConvertFromUtf32(c);
            list.Add(new JsonObject
            {
                ["char"] = c, ["relaxed"] = JsonValue.Create(s).ToJsonString(Relaxed), ["default"] = JsonValue.Create(s).ToJsonString(Default),
            });
        }
        return list;
    }

    static readonly string[] Texts =
    [
        "{}", "[]", "0", "\"x\"", "true",
        """{"a":1,"b":[1,2,{"c":[]}],"d":{},"e":[[],[{}]],"f":"é😀\u00a0<'>\"\\/","g":-0.0e-0,"h":1E+2}""",
        """[{"":""},{"a":{"b":{"c":{"d":[1]}}}}]""",
        """{"key \"quoted\" é\u0001":"v"}""",
        """{"a":1,"a":2}""",
        " {\"spaced\" : [ 1 , 2 ] } ",
    ];

    /// JsonNode.Parse(text).ToJsonString, indented and relaxed as ClaudeConfig writes it, with each newline.
    static JsonArray Writes()
    {
        var list = new JsonArray();
        foreach (var text in Texts)
            list.Add(new JsonObject
            {
                ["text"] = text,
                ["lf"] = JsonNode.Parse(text)!.ToJsonString(new JsonSerializerOptions(Relaxed) { WriteIndented = true, NewLine = "\n" }),
                ["crlf"] = JsonNode.Parse(text)!.ToJsonString(new JsonSerializerOptions(Relaxed) { WriteIndented = true, NewLine = "\r\n" }),
            });
        return list;
    }

    /// JsonNode.ToString: a string as it is, anything else indented, escaped by the default encoder, with
    /// Environment.NewLine (written here as \n).
    static JsonArray ToStrings()
    {
        var list = new JsonArray();
        foreach (var text in Texts.Concat(["\"a 'q' é\"", "[\"/x/.claude/pet/hook.py'\"]", "5.50", "false", "[\"a\\nb\"]"]))
            list.Add(new JsonObject { ["text"] = text, ["string"] = JsonNode.Parse(text)!.ToString().Replace(Environment.NewLine, "\n") });
        return list;
    }

    /// JsonNode.DeepEquals of two texts, or what it throws.
    static JsonArray DeepEquals()
    {
        var pairs = new (string, string)[]
        {
            ("{}", "{}"), ("{\"a\":1,\"b\":2}", "{\"b\":2,\"a\":1}"), ("[1,2]", "[2,1]"), ("{\"a\":1}", "{\"a\":1,\"b\":2}"),
            ("1", "1.0"), ("1", "1e0"), ("100", "1e2"), ("10", "1e1"), ("0.5", "5e-1"), ("0", "-0"), ("0", "0.0e5"), ("-0", "-0.0"),
            ("1.5", "15e-1"), ("123456789012345678901234567890", "1.2345678901234567890123456789e29"), ("1", "2"), ("-1", "1"),
            ("0.10", "0.1"), ("1e400", "1e400"), ("1e400", "10e399"), ("12", "1.2"), ("0.0001", "1e-4"), ("-0.0", "0e7"),
            ("\"é\"", "\"\\u00e9\""), ("\"a\"", "\"b\""), ("true", "true"), ("true", "false"), ("null", "null"), ("null", "{}"),
            ("1", "\"1\""), ("[]", "{}"), ("[[]]", "[[]]"), ("{\"a\":[1,{\"b\":null}]}", "{\"a\":[1.0,{\"b\":null}]}"),
            ("{\"a\":1,\"a\":2}", "{\"a\":1,\"a\":2}"), ("{\"x\":{\"a\":1,\"a\":2}}", "{\"x\":{\"a\":1,\"a\":2}}"),
            ("{\"x\":{\"a\":1,\"a\":2}}", "{\"x\":{\"a\":1}}"), ("{\"x\":{\"a\":1}}", "{\"x\":{\"a\":1,\"a\":2}}"),
            ("[{\"a\":1,\"a\":2}]", "[{\"a\":1,\"a\":2}]"), ("{\"a\":null}", "{\"b\":null}"),
        };
        var list = new JsonArray();
        foreach (var (a, b) in pairs)
        {
            var o = new JsonObject { ["a"] = a, ["b"] = b };
            try { o["equal"] = JsonNode.DeepEquals(JsonNode.Parse(a), JsonNode.Parse(b)); }
            catch (Exception ex) { o["error"] = $"{ex.GetType().Name}: {ex.Message}"; }
            list.Add(o);
        }
        return list;
    }

    /// JsonNode.Parse of a text: fine, or its exception; and then reading its member "x" as ClaudeConfig reads one,
    /// which is where a name given twice, or a node that isn't an object, throws.
    static JsonArray Parses()
    {
        var texts = new[]
        {
            "", " ", "{", "[", "}", "]", "{\"a\":1,}", "[1,]", "[1,,2]", "{,}", "{\"a\":}", "{\"a\" 1}", "{\"a\":1 \"b\":2}", "{'a':1}", "{a:1}",
            "{\"a\":1}}", "{\"a\":1} x", "{} {}", "[1] 2", "// c\n{}", "{}// c", "/* c */{}", "{\"a\":1/*c*/}",
            "{\"a\":01}", "{\"a\":-}", "{\"a\":1.}", "{\"a\":.5}", "{\"a\":1e}", "{\"a\":1e+}", "{\"a\":+1}", "{\"a\":NaN}", "{\"a\":-Infinity}",
            "{\"a\":tru}", "{\"a\":nul}", "{\"a\":falsey}", "{\"a\":truex}", "{\"a\":t}", "tru", "nul", "fals", "-", "1.", "01", "1 2",
            "{\"a\":\"x\\x\"}", "{\"a\":\"\\u12\"}", "{\"a\":\"\\u12g4\"}", "{\"a\":\"a\u0001b\"}", "{\"a\":\"a\tb\"}", "{\"a\":\"open", "\"x",
            "{\"a\":\"\\", "{\"a", "{\"a\"", "{\"a\":", "{\"a\":1", "{\"a\":1,", "[1", "[1,", "[\"a\"", "[\"a", "{\"a\":[}", "[}", "{]",
            "{\n  \"model\": \"opus\",\n}", "{\n  \"a\": 1,\n  \"b\": x\n}", "{\r\n  \"a\": 1,\r\n}", "{\"é\":1,}", "{\"a\":\"é\"x}",
            "{\"😀\":1 x}", "\uFEFF{}", "{}\uFEFF", "\u00a0{}", "{\"a\":1}\u0000", "\u0000", "{\"a\":[1,2]]", "[1,2}", "{\"a\":{}]",
            new string('[', 64) + new string(']', 64), new string('[', 65) + new string(']', 65),
            "{\"a\":" + new string('[', 63) + new string(']', 63) + "}", "{\"a\":" + new string('[', 64) + new string(']', 64) + "}",
            string.Concat(Enumerable.Repeat("{\"a\":", 64)) + "1" + new string('}', 64),
            string.Concat(Enumerable.Repeat("{\"a\":", 65)) + "1" + new string('}', 65),
            "{\"a\":tru, \"b\": \"" + new string('x', 300) + "\"}", "{\"a\":nulé}", "{\"a\":faé}", "{\"a\":1x}", "1.5x", "1.5.5", "1e5x", "0x10",
            "00", "-01", "1.e5", "-a", "[-]", "{\"a\":1.5e}", "123abc", "{\"a\":0.}", "{\"a\":\"\\u00e9\\uZZZZ\"}", "{\"a\":1,\n\n\t}",
            "{\"a\":1}\n\n x", "{\"a\":\"x\ny\"}", "\"\u0000\"", "{\"a\":1,\r}", "[1,\n  2,\n  ]", "{\"a\":1e5}", "-0", "[0.5e+10]", "{\"a\" : 1 }",
            "{\"a\":[1,\"b\",{\"c\":true}],\"d\":fals}", "[\"\\/\"]", "\t\r\n {}", "{\"a\":\"x\"\"b\"}", "{\"a\"1}",
            "{\"x\":1,\"x\":2}", "{\"x\":{\"a\":1,\"a\":2}}", "[{\"a\":1,\"a\":2}]", "5", "\"s\"", "null", "[1]", "{\"x\":null}", "{\"x\":[1]}",
            "{\"a\":\"\\ud800\"}", "{\"a\":\"\\udc00\\ud800\"}", "{\"a\":1e400}", "{\"a\":-0}", "  {  }  ", "{\"a\":\"\u007f\u0080\"}",
        };
        var list = new JsonArray();
        foreach (var text in texts)
        {
            var o = new JsonObject { ["text"] = text };
            JsonNode node = null;
            try
            {
                node = JsonNode.Parse(text);
                o["parsed"] = true;
            }
            catch (Exception ex)
            {
                o["parsed"] = false;
                o["error"] = $"{ex.GetType().Name}: {ex.Message}";
            }
            if (node != null)
                try { o["x"] = node["x"]?.ToJsonString() ?? "(none)"; }
                catch (Exception ex) { o["x_error"] = $"{ex.GetType().Name}: {ex.Message}"; }
            list.Add(o);
        }
        return list;
    }
}

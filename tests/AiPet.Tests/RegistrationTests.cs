using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json.Nodes;
using Xunit;

namespace AiPet.Tests;

/// Registering AiPet's hooks with Claude Code and Codex (Install.cs and CodexConfig.cs, compiled into the tests) in
/// temp config folders, and `aipet-hook --install|--doctor` run as a program in a temp home. The in-process tests
/// point CLAUDE_CONFIG_DIR and CODEX_HOME, which the whole process sees, at their own folders, so they run alone.
[Collection(RegistrationCollection.Name)]
public sealed class RegistrationTests : IDisposable
{
    const string Exe = "/opt/aipet/hooks/aipet-hook";  // needs no quoting or short name on any OS
    const UnixFileMode Private = UnixFileMode.UserRead | UnixFileMode.UserWrite;

    readonly string claudeDir = TestEnv.NewDir("claude"), codexHome = TestEnv.NewDir("codex");
    readonly string oldClaude = Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR"), oldCodex = Environment.GetEnvironmentVariable("CODEX_HOME");

    public RegistrationTests()
    {
        Environment.SetEnvironmentVariable("CLAUDE_CONFIG_DIR", claudeDir);
        Environment.SetEnvironmentVariable("CODEX_HOME", codexHome);
    }

    public void Dispose()
    {
        Environment.SetEnvironmentVariable("CLAUDE_CONFIG_DIR", oldClaude);
        Environment.SetEnvironmentVariable("CODEX_HOME", oldCodex);
    }

    string Settings => Path.Combine(claudeDir, "settings.json");
    string ConfigToml => Path.Combine(codexHome, "config.toml");
    string HooksJson => Path.Combine(codexHome, "hooks.json");

    /// Claude Code's record of an installed aipet plugin (enabling it in settings.json alone runs nothing).
    void InstallClaudePlugin()
    {
        Directory.CreateDirectory(Path.Combine(claudeDir, "plugins"));
        File.WriteAllText(Path.Combine(claudeDir, "plugins", "installed_plugins.json"),
            "{\"version\":2,\"plugins\":{\"aipet@aipet\":[{\"scope\":\"user\",\"version\":\"0.2.0\"}]}}");
    }

    /// The aipet plugin as `codex plugin add aipet@aipet` leaves it: in config.toml and in Codex's plugin cache.
    void AddCodexPlugin(string entry = "[plugins.\"aipet@aipet\"]\nenabled = true\n")
    {
        Directory.CreateDirectory(Path.Combine(codexHome, "plugins", "cache", "aipet", "aipet", "0.2.0"));
        File.AppendAllText(ConfigToml, "\n" + entry);
    }

    // ------------------------------------------------------------------ file modes and atomic writes
    /// A rewritten settings.json or config.toml keeps its mode: they often hold tokens, and Codex keeps config.toml
    /// 0600. Also a mode the umask would narrow (group-writable).
    [UnixFact("Unix modes only")]
    public void Rewrites_KeepTheFilesMode()
    {
        foreach (var mode in new[] { Private, Private | UnixFileMode.GroupRead | UnixFileMode.GroupWrite })
        {
            File.WriteAllText(Settings, "{\"env\":{\"ANTHROPIC_API_KEY\":\"secret\"}}");
            File.SetUnixFileMode(Settings, mode);
            Assert.Equal(0, ClaudeConfig.Install(Exe));
            Assert.Contains("aipet-hook", File.ReadAllText(Settings));
            Assert.Equal(mode, File.GetUnixFileMode(Settings));
            Assert.Equal(0, ClaudeConfig.Uninstall());
            Assert.DoesNotContain("aipet-hook", File.ReadAllText(Settings));
            Assert.Equal(mode, File.GetUnixFileMode(Settings));

            File.WriteAllText(ConfigToml, "[mcp_servers.x]\ncommand = \"x\"\nenv = { TOKEN = \"secret\" }\n");
            File.SetUnixFileMode(ConfigToml, mode);
            Assert.Equal(0, CodexConfig.Install(Exe));
            Assert.Contains("aipet-hook", File.ReadAllText(ConfigToml));
            Assert.Equal(mode, File.GetUnixFileMode(ConfigToml));
            Assert.Equal(0, CodexConfig.Uninstall());
            Assert.DoesNotContain("aipet-hook", File.ReadAllText(ConfigToml));
            Assert.Equal(mode, File.GetUnixFileMode(ConfigToml));
        }
        Assert.Empty(Directory.GetFiles(claudeDir, "*.aipet-tmp").Concat(Directory.GetFiles(codexHome, "*.aipet-tmp")));
    }

    [UnixFact]
    public void NewFiles_AreTheUsersOnly()
    {
        Assert.Equal(0, ClaudeConfig.Install(Exe));
        Assert.Equal(Private, File.GetUnixFileMode(Settings));
        Assert.Equal(0, CodexConfig.Install(Exe));
        Assert.Equal(Private, File.GetUnixFileMode(ConfigToml));
    }

    /// A temp file left by an interrupted run is written anew, so it can't pass its own mode on.
    [UnixFact]
    public void ALeftoverTempFile_DoesntPassItsModeOn()
    {
        File.WriteAllText(ConfigToml, "model = \"o3\"\n");
        File.SetUnixFileMode(ConfigToml, Private);
        var tmp = ConfigToml + ".aipet-tmp";
        File.WriteAllText(tmp, "half a file");
        File.SetUnixFileMode(tmp, Private | UnixFileMode.GroupRead | UnixFileMode.OtherRead | UnixFileMode.OtherWrite);
        Assert.Equal(0, CodexConfig.Install(Exe));
        Assert.Equal(Private, File.GetUnixFileMode(ConfigToml));
        Assert.StartsWith("model = \"o3\"", File.ReadAllText(ConfigToml));
        Assert.False(File.Exists(tmp));
    }

    /// hooks.json (used when the user's other hooks are there) is replaced by a new file, not truncated and
    /// rewritten: a reader that opened it before still reads it whole. It keeps its mode too.
    [UnixFact("an open file can't be replaced there")]
    public void HooksJson_IsReplacedAtOnce()
    {
        var other = "{\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          { \"type\": \"command\", \"command\": \"other-tool\" }\n        ]\n      }\n    ]\n  }\n}\n";
        File.WriteAllText(HooksJson, other);
        File.SetUnixFileMode(HooksJson, Private);
        using (var reader = new FileStream(HooksJson, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
        {
            Assert.Equal(0, CodexConfig.Install(Exe));
            Assert.Equal(other, new StreamReader(reader).ReadToEnd());
        }
        var now = File.ReadAllText(HooksJson);
        Assert.Contains("other-tool", now);
        Assert.Contains("aipet-hook", now);
        Assert.False(File.Exists(ConfigToml) && File.ReadAllText(ConfigToml).Contains("aipet-hook"));
        Assert.Equal(Private, File.GetUnixFileMode(HooksJson));

        Assert.Equal(0, CodexConfig.Uninstall());
        Assert.DoesNotContain("aipet-hook", File.ReadAllText(HooksJson));
        Assert.Contains("other-tool", File.ReadAllText(HooksJson));
        Assert.Equal(Private, File.GetUnixFileMode(HooksJson));
        Assert.Empty(Directory.GetFiles(codexHome, "*.aipet-tmp"));
    }

    // ------------------------------------------------------------------ the plugin
    /// With the aipet plugin enabled and installed, hooks in settings.json would report every event a second time:
    /// --install registers none, and takes out the ones registered before the plugin. It still succeeds, since the
    /// pet gets every event and installers run it unchecked (install-from-source.sh would stop halfway).
    [Fact]
    public void InstallClaude_WithThePluginEnabled_RegistersNone()
    {
        if (!ClaudeConfig.PluginRuns()) return;  // Windows without Git Bash: see InstallClaude_WithAPluginThatCantRun_Registers
        InstallClaudePlugin();
        var text = "{\n  \"enabledPlugins\": { \"aipet@aipet\": true }\n}\n";
        File.WriteAllText(Settings, text);
        Assert.Equal(0, ClaudeConfig.Install(Exe));
        Assert.Equal(text, File.ReadAllText(Settings));
        Assert.Empty(Directory.GetFiles(claudeDir, "*.bak"));  // untouched, so no backup either

        File.WriteAllText(Settings, "{ \"enabledPlugins\": { \"aipet@aipet\": false } }");
        Assert.Equal(0, ClaudeConfig.Install(Exe));
        Assert.Contains("aipet-hook", File.ReadAllText(Settings));
        Assert.Equal(0, ClaudeConfig.Install(Exe));  // unchanged, and still fine

        // hooks from before the plugin was enabled go: the plugin replaces them
        var root = JsonNode.Parse(File.ReadAllText(Settings)).AsObject();
        root["enabledPlugins"]["aipet@aipet"] = true;
        File.WriteAllText(Settings, root.ToJsonString());
        Assert.Equal(0, ClaudeConfig.Install(Exe));
        Assert.DoesNotContain("aipet-hook", File.ReadAllText(Settings));
        Assert.Equal("aipet@aipet", ClaudeConfig.Plugin(JsonNode.Parse(File.ReadAllText(Settings)).AsObject()));
    }

    /// A settings.json synced from another machine can enable the plugin where it was never installed: nothing
    /// runs its hooks, so --install registers its own.
    [Fact]
    public void InstallClaude_WithThePluginEnabledButNotInstalled_Registers()
    {
        File.WriteAllText(Settings, "{ \"enabledPlugins\": { \"aipet@aipet\": true } }");
        Assert.Null(ClaudeConfig.Plugin(JsonNode.Parse(File.ReadAllText(Settings)).AsObject()));
        Assert.Equal(0, ClaudeConfig.Install(Exe));
        Assert.Contains("aipet-hook", File.ReadAllText(Settings));
    }

    [Fact]
    public void InstallCodex_WithThePluginEnabled_RegistersNone()
    {
        File.WriteAllText(ConfigToml, "model = \"o3\"\n");
        AddCodexPlugin();
        var text = File.ReadAllText(ConfigToml);
        Assert.Equal(0, CodexConfig.Install(Exe));
        Assert.Equal(text, File.ReadAllText(ConfigToml));
        Assert.False(File.Exists(HooksJson));
        Assert.Empty(Directory.GetFiles(codexHome, "*.bak"));

        // hooks from before the plugin was added go: the plugin replaces them
        File.WriteAllText(ConfigToml, "model = \"o3\"\n");
        Assert.Equal(0, CodexConfig.Install(Exe));
        AddCodexPlugin();
        Assert.Equal(0, CodexConfig.Install(Exe));
        var after = File.ReadAllText(ConfigToml);
        Assert.DoesNotContain("aipet-hook", after);
        Assert.Contains("[plugins.\"aipet@aipet\"]", after);
    }

    /// On Windows the plugin's hooks run in Git Bash. Without it they don't run, so --install registers its own.
    [WindowsFact]
    public void InstallClaude_WithAPluginThatCantRun_Registers()
    {
        var empty = TestEnv.NewDir("nobash");
        using var env = new EnvScope(("CLAUDE_CODE_GIT_BASH_PATH", null), ("PATH", empty), ("ProgramFiles", empty), ("LOCALAPPDATA", empty));
        Assert.False(ClaudeConfig.PluginRuns());
        InstallClaudePlugin();
        File.WriteAllText(Settings, "{ \"enabledPlugins\": { \"aipet@aipet\": true } }");
        Assert.Equal(0, ClaudeConfig.Install(Exe));
        Assert.Contains("aipet-hook", File.ReadAllText(Settings));
    }

    /// Git Bash is found where install.ps1 and Claude Code look: CLAUDE_CODE_GIT_BASH_PATH, next to git.exe on
    /// PATH (<Git>\cmd\git.exe), then the usual install folders.
    [Fact]
    public void GitBash_IsFoundWhereClaudeLooks()
    {
        var root = TestEnv.NewDir("git");
        string Touch(params string[] parts)
        {
            var f = Path.Combine(new[] { root }.Concat(parts).ToArray());
            Directory.CreateDirectory(Path.GetDirectoryName(f));
            File.WriteAllText(f, "");
            return f;
        }
        var nowhere = Path.Combine(root, "nowhere");
        using var env = new EnvScope(("CLAUDE_CODE_GIT_BASH_PATH", null), ("PATH", nowhere), ("ProgramFiles", nowhere), ("LOCALAPPDATA", nowhere));
        Assert.Null(ClaudeConfig.GitBash());

        var local = Touch("local", "Programs", "Git", "bin", "bash.exe");
        Environment.SetEnvironmentVariable("LOCALAPPDATA", Path.Combine(root, "local"));
        Assert.Equal(local, ClaudeConfig.GitBash());
        var pf = Touch("pf", "Git", "bin", "bash.exe");
        Environment.SetEnvironmentVariable("ProgramFiles", Path.Combine(root, "pf"));
        Assert.Equal(pf, ClaudeConfig.GitBash());
        Touch("scoop", "Git", "cmd", "git.exe");
        var scoop = Touch("scoop", "Git", "bin", "bash.exe");
        Environment.SetEnvironmentVariable("PATH", nowhere + Path.PathSeparator + Path.Combine(root, "scoop", "Git", "cmd"));
        Assert.Equal(scoop, ClaudeConfig.GitBash());
        var set = Touch("custom", "bash.exe");
        Environment.SetEnvironmentVariable("CLAUDE_CODE_GIT_BASH_PATH", set);
        Assert.Equal(set, ClaudeConfig.GitBash());
    }

    /// Codex runs the plugin's hooks while config.toml has its entry, not disabled, and its cache folder is there.
    [Fact]
    public void CodexPlugin_IsWhatCodexRuns()
    {
        string With(string toml, bool cache = true)
        {
            var cacheDir = Path.Combine(codexHome, "plugins", "cache", "aipet", "aipet");
            if (Directory.Exists(cacheDir)) Directory.Delete(cacheDir, true);
            if (cache) Directory.CreateDirectory(Path.Combine(cacheDir, "0.2.0"));
            File.WriteAllText(ConfigToml, toml);
            return CodexConfig.Plugin();
        }
        Assert.Equal("aipet@aipet", With("[plugins.\"aipet@aipet\"]\nenabled = true\n"));
        Assert.Equal("aipet@aipet", With("[plugins.\"aipet@aipet\"]\n"));  // Codex takes it as enabled
        Assert.Equal("aipet@aipet", With("[plugins]\n\"aipet@aipet\".enabled = true  # on\n"));
        Assert.Equal("aipet@aipet", With("plugins.\"aipet@aipet\".enabled = true\n"));
        Assert.Null(With("[plugins.\"aipet@aipet\"]\nenabled = false\n"));
        Assert.Null(With("[plugins]\n\"aipet@aipet\" = { enabled = false }\n"));
        Assert.Null(With("[plugins.\"aipet@aipet\"]\nenabled = true\n", cache: false));
        Assert.Null(With("[plugins.\"other@aipet\"]\nenabled = true\n"));
        Assert.Null(With("[profiles.x]\nplugins = 1\n"));
        Assert.Null(With(""));
    }

    /// PluginEvents is what plugins/aipet/hooks/codex.json registers.
    [Fact]
    public void PluginEvents_AreThePluginsHooks()
    {
        var file = JsonNode.Parse(File.ReadAllText(Path.Combine(RepoRoot, "plugins", "aipet", "hooks", "codex.json")));
        Assert.Equal(file["hooks"].AsObject().Select(kv => kv.Key).Order(), CodexConfig.PluginEvents.Order());
    }

    /// Older installs' commands used the 8.3 short name; they're AiPet's, and so are the plugin's.
    [Fact]
    public void IsOurs_KnowsEveryFormOfAiPetsCommand()
    {
        Assert.True(CodexConfig.IsOurs(@"C:\Users\JOHNSM~1\AppData\Local\AiPet\hooks\AIPET-~1.EXE --agent codex"));
        Assert.True(CodexConfig.IsOurs(@"C:\Users\x\AppData\Local\AiPet\hooks\aipet-hook.exe --agent codex"));
        Assert.True(CodexConfig.IsOurs("'/home/x/.local/share/AiPet/hooks/aipet-hook' --agent codex"));
        Assert.True(CodexConfig.IsOurs("sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex"));
        Assert.True(CodexConfig.IsOurs(@"& (Join-Path $env:PLUGIN_ROOT 'native\win-x64\aipet-hook.exe') --agent codex"));
        Assert.False(CodexConfig.IsOurs(@"C:\tools\AIPET-~1.EXE --agent claude"));
        Assert.False(CodexConfig.IsOurs("other-tool --agent codex"));
        Assert.False(CodexConfig.IsOurs(null));
    }

    // ------------------------------------------------------------------ aipet-hook as a program
    /// `dotnet aipet-hook.dll --install` would register dotnet itself: it refuses and writes nothing. --uninstall
    /// works from there.
    [Fact]
    public void Install_UnderDotnet_Refuses()
    {
        var home = TestEnv.NewDir("home");
        foreach (var agent in new[] { "claude", "codex" })
        {
            var r = RunHook(home, "--install", agent);
            Assert.Equal(1, r.Exit);
            Assert.Contains("not aipet-hook", r.Err);
            Assert.Contains("Nothing was changed", r.Err);
            Assert.Empty(TestEnv.Contents(home));
        }
        var u = RunHook(home, "--uninstall", "claude");
        Assert.Equal((0, ""), (u.Exit, u.Err));
        Assert.Equal(2, RunHook(home, "--install", "cursor").Exit);
    }

    /// Without hooks/list (no codex), an older install's AIPET-~1.EXE hook in config.toml is AiPet's.
    [UnixFact("the doctor would find the user's codex.exe there")]
    public void Doctor_KnowsLegacyCodexHooks()
    {
        var home = TestEnv.NewDir("home");
        var codex = Directory.CreateDirectory(Path.Combine(home, "codex")).FullName;
        File.WriteAllText(Path.Combine(codex, "config.toml"),
            "[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = 'C:\\Users\\JOHNSM~1\\AppData\\Local\\AiPet\\hooks\\AIPET-~1.EXE --agent codex'\ntimeout = 30\n");
        var r = RunHook(home, "--doctor", "codex");
        Assert.Contains("[ok]   Stop", r.Out);
        Assert.DoesNotContain("[FAIL]", r.Out);
        Assert.Equal(0, r.Exit);
    }

    /// Without hooks/list, a plugin enabled in config.toml provides the hooks: no "not registered".
    [UnixFact]
    public void Doctor_WithoutHooksList_KnowsThePlugin()
    {
        var home = TestEnv.NewDir("home");
        var codex = Path.Combine(home, "codex");
        Directory.CreateDirectory(Path.Combine(codex, "plugins", "cache", "aipet", "aipet", "0.2.0"));
        File.WriteAllText(Path.Combine(codex, "config.toml"), "[plugins.\"aipet@aipet\"]\nenabled = true\n");
        var r = RunHook(home, "--doctor", "codex");
        Assert.Contains("aipet@aipet plugin is enabled", r.Out);
        Assert.DoesNotContain("--install codex", r.Out);
        Assert.Equal(0, r.Exit);

        // and hooks in config.toml as well: every event twice
        File.AppendAllText(Path.Combine(codex, "config.toml"), "\n[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ntype = \"command\"\ncommand = \"'/x/aipet-hook' --agent codex\"\n");
        r = RunHook(home, "--doctor", "codex");
        Assert.Contains("reported twice", r.Out);
        Assert.Contains("--uninstall codex", r.Out);
    }

    /// The plugin's 9 events (the Windows set, on every OS) are all it needs, and its command is probed with
    /// PLUGIN_ROOT set, as Codex runs it.
    [UnixFact("a fake codex on PATH is a shell script")]
    public void Doctor_ChecksThePluginsHooksAsThePlugins()
    {
        var home = TestEnv.NewDir("home");
        var root = PluginRoot(home, withHook: true);
        FakeCodex(home, PluginHooks(root));
        var r = RunHook(home, "--doctor", "codex", "--probe");
        Assert.Contains("hooks come from the aipet@aipet plugin, for all 9 of its events", r.Out);
        Assert.DoesNotContain("--install codex", r.Out);
        Assert.DoesNotContain("[FAIL]", r.Out);
        Assert.Contains("[ok]   sh: ran, printed nothing, exit 0", r.Out);
        Assert.Equal(0, r.Exit);
    }

    [UnixFact]
    public void Doctor_PluginAndDirectHooks_AreReportedTwice()
    {
        var home = TestEnv.NewDir("home");
        var root = PluginRoot(home, withHook: true);
        var direct = CodexConfig.Events.Select(e => Listed(e.Event, "'/opt/aipet/hooks/aipet-hook' --agent codex", "user", null));
        FakeCodex(home, PluginHooks(root).Concat(direct));
        var r = RunHook(home, "--doctor", "codex");
        Assert.Contains("the aipet@aipet plugin is also enabled, so every event is reported twice: run aipet-hook --uninstall codex", r.Out);
        Assert.DoesNotContain("--install codex", r.Out);
    }

    /// The plugin's launcher quietly does nothing without a hook binary for this system: that's a problem, and
    /// isn't probed as if it worked.
    [UnixFact]
    public void Doctor_PluginWithoutAHookForThisSystem_Fails()
    {
        var home = TestEnv.NewDir("home");
        FakeCodex(home, PluginHooks(PluginRoot(home, withHook: false)));
        var r = RunHook(home, "--doctor", "codex", "--probe");
        Assert.Contains("the plugin has no hook for this system", r.Out);
        Assert.Contains("skipped: the plugin has no hook for this system", r.Out);
        Assert.Equal(1, r.Exit);
    }

    // ------------------------------------------------------------------ helpers
    static readonly string RepoRoot = FindRepo();

    static string FindRepo()
    {
        for (var d = new DirectoryInfo(AppContext.BaseDirectory); d != null; d = d.Parent)
            if (File.Exists(Path.Combine(d.FullName, "AiPet.slnx"))) return d.FullName;
        throw new DirectoryNotFoundException("the repository isn't above " + AppContext.BaseDirectory);
    }

    /// The plugin as Codex caches it, under the home's CODEX_HOME: the real launcher, and (withHook) a hook binary
    /// for this system that runs the built hook.
    static string PluginRoot(string home, bool withHook)
    {
        var root = Path.Combine(home, "codex", "plugins", "cache", "aipet", "aipet", "0.2.0");
        Directory.CreateDirectory(Path.Combine(root, "hooks"));
        File.Copy(Path.Combine(RepoRoot, "plugins", "aipet", "hooks", "codex.json"), Path.Combine(root, "hooks", "codex.json"));
        Directory.CreateDirectory(Path.Combine(root, "native"));
        File.Copy(Path.Combine(RepoRoot, "plugins", "aipet", "native", "aipet-hook.sh"), Path.Combine(root, "native", "aipet-hook.sh"));
        if (withHook)
            foreach (var rid in new[] { "linux-x64", "linux-arm64" })
                Script(Path.Combine(root, "native", rid, "aipet-hook"), $"#!/bin/sh\nexec '{TestEnv.Dotnet}' '{TestEnv.HookDll}' \"$@\"\n");
        return root;
    }

    /// The plugin's hooks as hooks/list gives them (Codex 0.157), trusted.
    static IEnumerable<JsonObject> PluginHooks(string root) =>
        CodexConfig.PluginEvents.Select(e => Listed(e, "sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex", "plugin", root));

    static JsonObject Listed(string ev, string command, string source, string pluginRoot) => new()
    {
        ["key"] = pluginRoot != null ? $"aipet@aipet:hooks/codex.json:{CodexConfig.Snake(ev)}:0:0" : $"config.toml:{CodexConfig.Snake(ev)}:0:0",
        ["eventName"] = char.ToLowerInvariant(ev[0]) + ev[1..], ["handlerType"] = "command", ["command"] = command,
        ["async"] = true, ["timeoutSec"] = 30,
        ["sourcePath"] = pluginRoot != null ? Path.Combine(pluginRoot, "hooks", "codex.json") : "/x/config.toml",
        ["source"] = source, ["pluginId"] = pluginRoot != null ? "aipet@aipet" : null,
        ["enabled"] = true, ["isManaged"] = false, ["trustStatus"] = "trusted",
    };

    /// A codex on the home's PATH whose app-server answers initialize, and hooks/list with these hooks.
    static void FakeCodex(string home, IEnumerable<JsonObject> hooks)
    {
        var bin = Path.Combine(home, "bin");
        var list = new JsonObject
        {
            ["id"] = 2,
            ["result"] = new JsonObject { ["data"] = new JsonArray(new JsonObject { ["cwd"] = home, ["hooks"] = new JsonArray(hooks.ToArray<JsonNode>()) }) },
        };
        Directory.CreateDirectory(bin);
        File.WriteAllText(Path.Combine(bin, "hooks-list.json"), list.ToJsonString() + "\n");
        Script(Path.Combine(bin, "codex"), """
            #!/bin/sh
            case "$1" in
              --version) echo "codex-cli 0.0.0-test"; exit 0 ;;
              app-server) ;;
              *) exit 2 ;;
            esac
            while IFS= read -r line; do
              case "$line" in
                *'"id":1,'*) echo '{"id":1,"result":{}}' ;;
                *'"id":2,'*) while IFS= read -r l; do printf '%s\n' "$l"; done < "${0%/*}/hooks-list.json" ;;
              esac
            done
            """ + "\n");
    }

    static void Script(string path, string text)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path));
        File.WriteAllText(path, text);
        if (!OperatingSystem.IsWindows()) File.SetUnixFileMode(path, UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
    }

    /// `dotnet aipet-hook.dll <args>` with everything it could read or write in home: the agents' config folders,
    /// the data and temp folders, and an endpoint no pet listens on. PATH is home/bin (a fake codex, if any) and the
    /// system's programs, never the user's codex.
    static (int Exit, string Out, string Err) RunHook(string home, params string[] args)
    {
        Assert.True(File.Exists(TestEnv.HookDll), "the hook isn't built at " + TestEnv.HookDll);
        var psi = new ProcessStartInfo(TestEnv.Dotnet)
        {
            UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8,
        };
        psi.ArgumentList.Add(TestEnv.HookDll);
        foreach (var a in args) psi.ArgumentList.Add(a);
        var bin = Path.Combine(home, "bin");
        var tmp = Directory.CreateDirectory(Path.Combine(TestEnv.Root, "tmp-" + Guid.NewGuid().ToString("N")[..8])).FullName;
        foreach (var (name, value) in new[]
                 {
                     ("HOME", home), ("USERPROFILE", home), ("XDG_CONFIG_HOME", Path.Combine(home, ".config")),
                     ("XDG_DATA_HOME", Path.Combine(home, ".local", "share")), ("XDG_STATE_HOME", Path.Combine(home, ".local", "state")),
                     ("CLAUDE_CONFIG_DIR", Path.Combine(home, "claude")), ("CODEX_HOME", Path.Combine(home, "codex")),
                     ("AIPET_DATA_DIR", Path.Combine(home, "data")), ("TMPDIR", tmp), ("TMP", tmp), ("TEMP", tmp),
                     ("AIPET_PIPE", TestEnv.NoPet()), ("SHELL", "/bin/sh"),
                     ("PATH", OperatingSystem.IsWindows() ? bin : File.Exists(Path.Combine(bin, "codex")) ? $"{bin}:/usr/bin:/bin" : bin),
                 })
            psi.Environment[name] = value;
        using var p = Process.Start(psi);
        p.StandardInput.Close();
        var o = p.StandardOutput.ReadToEndAsync();
        var e = p.StandardError.ReadToEndAsync();
        if (!p.WaitForExit(60000))
        {
            try { p.Kill(true); } catch { }
            Assert.Fail($"aipet-hook {string.Join(" ", args)} didn't exit within 60 s");
        }
        return (p.ExitCode, o.Result, e.Result);
    }

    /// Sets environment variables for one test and puts the old values back.
    sealed class EnvScope : IDisposable
    {
        readonly (string Name, string Value)[] old;

        public EnvScope(params (string Name, string Value)[] vars)
        {
            old = vars.Select(v => (v.Name, Environment.GetEnvironmentVariable(v.Name))).ToArray();
            foreach (var (name, value) in vars) Environment.SetEnvironmentVariable(name, value);
        }

        public void Dispose()
        {
            foreach (var (name, value) in old) Environment.SetEnvironmentVariable(name, value);
        }
    }
}

/// The registration tests change process-wide environment variables, so nothing else runs beside them.
[CollectionDefinition(Name, DisableParallelization = true)]
public sealed class RegistrationCollection
{
    public const string Name = "registration";
}

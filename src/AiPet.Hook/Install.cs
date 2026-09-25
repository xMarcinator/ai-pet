using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet;

/// `aipet-hook --install claude|codex` / `--uninstall claude|codex`
/// Registers this exe as the agent's hook, so installers need no jq/python and work on every OS.
/// Re-running is safe: AiPet's own (and older AiPet/ClaudePet) entries are replaced, everything else is kept,
/// and nothing is written when the registration is already exactly right.
static class Install
{
    public static int Run(string action, string agent)
    {
        if (agent is not ("claude" or "codex"))
        {
            Console.Error.WriteLine("usage: aipet-hook --install|--uninstall claude|codex");
            return 2;
        }
        // what runs this is what gets registered: under `dotnet aipet-hook.dll` that's dotnet itself, which fails on
        // every event and which --uninstall can't tell from anyone else's hook
        var exe = Environment.ProcessPath;
        if (action == "--install" && !IsHook(exe))
        {
            Console.Error.WriteLine($"--install registers the program that runs it, and that's {exe ?? "unknown"}, not aipet-hook.");
            Console.Error.WriteLine("Run the aipet-hook executable itself (as the installers do), not `dotnet aipet-hook.dll`. Nothing was changed.");
            return 1;
        }
        try
        {
            if (agent == "claude") return action == "--install" ? ClaudeConfig.Install(exe) : ClaudeConfig.Uninstall();
            return action == "--install" ? CodexConfig.Install(exe) : CodexConfig.Uninstall();
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Couldn't update {agent} settings: {ex.Message}");
            return 1;
        }
    }

    static bool IsHook(string exe) => Path.GetFileName(exe) is { } name
        && (name.Equals("aipet-hook", StringComparison.OrdinalIgnoreCase) || name.Equals("aipet-hook.exe", StringComparison.OrdinalIgnoreCase));

    /// Replaces a config file with text: written next to it first and then moved over it, so a reader or a crash
    /// never sees half a file. real is the file itself, not a link to it. It keeps the old file's Unix mode (Codex
    /// keeps config.toml 0600, and these files often hold tokens); a new file is 0600.
    internal static void Save(string real, string text)
    {
        var tmp = real + ".aipet-tmp";
        File.Delete(tmp);  // a leftover would keep its own mode: the one given here only applies to a new file
        var options = new FileStreamOptions { Mode = FileMode.CreateNew, Access = FileAccess.Write };
        var mode = UnixFileMode.UserRead | UnixFileMode.UserWrite;
        if (!OperatingSystem.IsWindows())
        {
            if (File.Exists(real)) mode = File.GetUnixFileMode(real);
            options.UnixCreateMode = mode;
        }
        try
        {
            using (var w = new StreamWriter(new FileStream(tmp, options), new UTF8Encoding(false))) w.Write(text);
            if (!OperatingSystem.IsWindows()) File.SetUnixFileMode(tmp, mode);  // the umask may have taken some away
            File.Move(tmp, real, overwrite: true);
        }
        catch
        {
            try { File.Delete(tmp); } catch { }
            throw;
        }
    }
}

/// Claude Code: hooks in ~/.claude/settings.json (or $CLAUDE_CONFIG_DIR). Claude runs the exe directly with its
/// arguments (no shell), so the path needs no quoting on any OS.
static class ClaudeConfig
{
    static string Dir => Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR") is { Length: > 0 } d ? d
        : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".claude");

    public static string Settings => Path.Combine(Dir, "settings.json");

    /// All run in the background except Stop, which runs in line so the final "done" state is written after the
    /// turn's other events (the hook never returns anything to Claude). Matcher: "*" on the tool events; the others
    /// have no matcher, which Claude takes as every one (all compactions, sub-agent types, MCP servers).
    public static readonly (string Event, bool Matcher, bool Async)[] Events =
    {
        ("SessionStart", false, true), ("UserPromptSubmit", false, true), ("PreToolUse", true, true),
        ("PostToolUse", true, true), ("PostToolUseFailure", true, true), ("PermissionRequest", true, true),
        ("PermissionDenied", false, true), ("Notification", false, true), ("Elicitation", false, true),
        ("ElicitationResult", false, true), ("PreCompact", false, true), ("PostCompact", false, true),
        ("SubagentStart", false, true), ("SubagentStop", false, true),
        ("Stop", false, false), ("StopFailure", false, true), ("SessionEnd", false, true),
    };

    public static bool IsOurs(JsonNode hook)
    {
        var cmd = hook?["command"]?.ToString() ?? "";
        return cmd.Contains("aipet-hook", StringComparison.OrdinalIgnoreCase) || cmd.Contains("ClaudePet.exe", StringComparison.OrdinalIgnoreCase)
            || LegacyHook(cmd) || (hook?["args"] is JsonArray args && args.Any(a => LegacyHook(a?.ToString())));
    }

    /// The old Python hook, only in its own place (~/.claude/pet/hook.py): other tools' pet/hook.py are left alone.
    static bool LegacyHook(string s) => s != null && Regex.IsMatch(s, @"[\\/]\.claude[\\/]pet[\\/]hook\.py(?=$|[""'\s])", RegexOptions.IgnoreCase);

    /// The AiPet plugin enabled in settings.json and installed (its id, e.g. aipet@aipet), or null. It brings its own
    /// hooks (plugins/aipet/hooks/hooks.json), which Claude runs as well as any in settings.json. A settings.json
    /// synced from another machine can enable a plugin this one never installed, and then nothing runs its hooks.
    public static string Plugin(JsonObject root) =>
        root?["enabledPlugins"] is JsonObject plugins
            ? plugins.FirstOrDefault(p => p.Key.StartsWith("aipet@") && p.Value is JsonValue v && v.TryGetValue(out bool on) && on && Installed(p.Key)).Key
            : null;

    /// Claude Code lists what's installed in plugins/installed_plugins.json ({"version":2,"plugins":{"<id>":[..]}})
    /// and keeps each plugin in plugins/cache/<marketplace>/<plugin>/<version>/.
    static bool Installed(string id)
    {
        try
        {
            var plugins = JsonNode.Parse(File.ReadAllText(Path.Combine(Dir, "plugins", "installed_plugins.json")))?["plugins"] as JsonObject;
            if (plugins?[id] is JsonNode entry && (entry is not JsonArray list || list.Count > 0)) return true;
        }
        catch { }
        var cache = Path.Combine(Dir, "plugins", "cache", id[(id.IndexOf('@') + 1)..], "aipet");
        return Directory.Exists(cache) && Directory.EnumerateDirectories(cache).Any();
    }

    public static int Install(string exe)
    {
        var text = File.Exists(Settings) ? File.ReadAllText(Settings) : "";
        var plugin = Plugin(string.IsNullOrWhiteSpace(text) ? null : JsonNode.Parse(text) as JsonObject);
        // the pet gets every event either way, so this isn't a failure: installers run --install without checking
        if (plugin != null && PluginRuns())
        {
            // hooks registered here earlier would report every event a second time: the plugin replaces them
            bool removed = Edit(null);
            Console.WriteLine(removed
                ? $"The {plugin} plugin is enabled in {Settings} and reports every event, so the AiPet hooks registered there earlier were removed (the plugin replaces them)."
                : $"The {plugin} plugin is enabled in {Settings} and already reports every event, so no hooks were registered (they would report each one twice).");
            Console.WriteLine($"To use hooks in settings.json instead, disable the plugin (claude plugin disable {plugin}) and install again.");
            return 0;
        }
        bool changed = Edit(hooks => Add(hooks, exe.Replace('\\', '/')));
        Console.WriteLine(changed ? $"AiPet hooks registered for Claude Code ({Settings})" : $"AiPet hooks for Claude Code are already registered ({Settings}, unchanged)");
        if (plugin != null)
            Console.WriteLine($"The {plugin} plugin is enabled too, but its hooks need Git Bash, which wasn't found. Once Git for Windows is installed, run aipet-hook --uninstall claude, or every event is reported twice.");
        Console.WriteLine("Check with: aipet-hook --doctor claude");
        return 0;
    }

    /// Whether the plugin's hooks can run here: on Windows Claude runs them in Git Bash ("shell": "bash").
    public static bool PluginRuns() => !OperatingSystem.IsWindows() || GitBash() != null;

    /// Git Bash where Claude Code looks for it (as install.ps1's Find-GitBash does), or null.
    internal static string GitBash()
    {
        var candidates = new List<string> { Environment.GetEnvironmentVariable("CLAUDE_CODE_GIT_BASH_PATH") };
        foreach (var dir in (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator))
            try
            {
                var git = Path.Combine(dir.Trim('"'), "git.exe");  // <Git>\cmd\git.exe
                if (File.Exists(git)) { candidates.Add(Path.Combine(Path.GetDirectoryName(Path.GetDirectoryName(git)), "bin", "bash.exe")); break; }
            }
            catch { }
        if (Environment.GetEnvironmentVariable("ProgramFiles") is { Length: > 0 } pf) candidates.Add(Path.Combine(pf, "Git", "bin", "bash.exe"));
        if (Environment.GetEnvironmentVariable("LOCALAPPDATA") is { Length: > 0 } la) candidates.Add(Path.Combine(la, "Programs", "Git", "bin", "bash.exe"));
        return candidates.FirstOrDefault(c => !string.IsNullOrEmpty(c) && File.Exists(c));
    }

    public static int Uninstall()
    {
        bool changed = Edit(null);
        Console.WriteLine(changed ? "AiPet hooks removed from Claude Code" : "No AiPet hooks were registered with Claude Code");
        return 0;
    }

    /// Drop AiPet's entries, optionally add new ones, and save only if that changed anything (with a backup).
    static bool Edit(Action<JsonObject> add)
    {
        var path = Settings;
        if (!File.Exists(path) && add == null) return false;
        var before = File.Exists(path) ? File.ReadAllText(path) : "";
        var root = (string.IsNullOrWhiteSpace(before) ? null : JsonNode.Parse(before) as JsonObject) ?? new JsonObject();
        bool hadHooks = root["hooks"] is JsonObject;
        if (root["hooks"] is not JsonObject hooks) root["hooks"] = hooks = new JsonObject();
        foreach (var (_, value) in hooks.ToList())
        {
            if (value is not JsonArray groups) continue;
            foreach (var g in groups.OfType<JsonObject>().ToList())
                if (g["hooks"] is JsonArray list)
                {
                    foreach (var h in list.Where(IsOurs).ToList()) list.Remove(h);
                    if (list.Count == 0) groups.Remove(g);
                }
        }
        add?.Invoke(hooks);
        foreach (var ev in hooks.Where(kv => kv.Value is JsonArray a && a.Count == 0).Select(kv => kv.Key).ToList()) hooks.Remove(ev);
        if (hooks.Count == 0 && (!hadHooks || add == null)) root.Remove("hooks");  // leave no empty "hooks" behind

        if (JsonNode.DeepEquals(JsonNode.Parse(string.IsNullOrWhiteSpace(before) ? "{}" : before), root)) return false;
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        if (File.Exists(path)) Backup(path);
        var real = RealPath(path);  // a symlinked settings.json (dotfiles): replace the file it points at, not the link
        AiPet.Install.Save(real, root.ToJsonString(new JsonSerializerOptions
        { WriteIndented = true, Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping }));
        return true;
    }

    static string RealPath(string path)
    {
        try { return File.ResolveLinkTarget(path, returnFinalTarget: true)?.FullName ?? path; }
        catch { return path; }
    }

    static void Add(JsonObject hooks, string exe)
    {
        foreach (var e in Events)
        {
            if (hooks[e.Event] is not JsonArray a) hooks[e.Event] = a = new JsonArray();
            a.Add((JsonNode)Group(e, new JsonObject { ["type"] = "command", ["command"] = exe, ["args"] = new JsonArray("--agent", "claude") }));
        }
    }

    /// An event's group around its one hook, which comes with its type and command; the timeout and async follow
    /// them. The plugin's hooks (PluginHooks) are the same with another command.
    internal static JsonObject Group((string Event, bool Matcher, bool Async) e, JsonObject hook)
    {
        hook["timeout"] = 5;
        if (e.Async) hook["async"] = true;
        var group = new JsonObject { ["hooks"] = new JsonArray(hook) };
        if (e.Matcher) group["matcher"] = "*";
        return group;
    }

    /// settings.json.aipet-<time>.bak before each change; the newest three are kept.
    static void Backup(string path)
    {
        try
        {
            File.Copy(path, $"{path}.aipet-{DateTime.Now:yyyyMMdd-HHmmss}.bak", overwrite: true);
            foreach (var old in Directory.GetFiles(Path.GetDirectoryName(path)!, Path.GetFileName(path) + ".aipet-*.bak").OrderByDescending(f => f).Skip(3))
                File.Delete(old);
        }
        catch { }
    }
}

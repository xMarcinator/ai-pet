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
        var exe = Environment.ProcessPath!;
        try
        {
            switch (agent)
            {
                case "claude": return action == "--install" ? ClaudeConfig.Install(exe) : ClaudeConfig.Uninstall();
                case "codex": return action == "--install" ? CodexConfig.Install(exe) : CodexConfig.Uninstall();
                default:
                    Console.Error.WriteLine("usage: aipet-hook --install|--uninstall claude|codex");
                    return 2;
            }
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Couldn't update {agent} settings: {ex.Message}");
            return 1;
        }
    }
}

/// Claude Code: hooks in ~/.claude/settings.json (or $CLAUDE_CONFIG_DIR). Claude runs the exe directly with its
/// arguments (no shell), so the path needs no quoting on any OS.
static class ClaudeConfig
{
    public static string Settings => Path.Combine(
        Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR") is { Length: > 0 } d ? d
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".claude"),
        "settings.json");

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

    public static int Install(string exe)
    {
        bool changed = Edit(hooks => Add(hooks, exe.Replace('\\', '/')));
        Console.WriteLine(changed ? $"AiPet hooks registered for Claude Code ({Settings})" : $"AiPet hooks for Claude Code are already registered ({Settings}, unchanged)");
        Console.WriteLine("Check with: aipet-hook --doctor claude");
        return 0;
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
        var tmp = real + ".aipet-tmp";
        File.WriteAllText(tmp, root.ToJsonString(new JsonSerializerOptions
        { WriteIndented = true, Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping }), new UTF8Encoding(false));
        File.Move(tmp, real, overwrite: true);
        return true;
    }

    static string RealPath(string path)
    {
        try { return File.ResolveLinkTarget(path, returnFinalTarget: true)?.FullName ?? path; }
        catch { return path; }
    }

    static void Add(JsonObject hooks, string exe)
    {
        foreach (var (ev, matcher, isAsync) in Events)
        {
            var hook = new JsonObject
            {
                ["type"] = "command", ["command"] = exe, ["args"] = new JsonArray("--agent", "claude"), ["timeout"] = 5,
            };
            if (isAsync) hook["async"] = true;
            var group = new JsonObject { ["hooks"] = new JsonArray(hook) };
            if (matcher) group["matcher"] = "*";
            if (hooks[ev] is not JsonArray a) hooks[ev] = a = new JsonArray();
            a.Add((JsonNode)group);
        }
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

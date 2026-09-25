using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;

namespace AiPet;

/// What uninstalling the Windows app does to the agents' configs. install.ps1 registers the hook directly
/// (aipet-hook --install claude) when Claude's plugin can't run for lack of Git Bash, and that registration names the
/// installed exe (%LOCALAPPDATA%\AiPetApp\current\aipet-hook.exe). Once the uninstaller has deleted it, Claude would
/// fail to run it on every event. So the uninstaller (Velopack's uninstall hook, Updates.cs) runs
/// `aipet-hook --uninstall <agent>` first, for each agent whose config names this install's hook. Registrations of
/// another copy (the portable zip, a build from source) are left alone, and so are the plugins, which the agents
/// manage themselves.
public static class HookCleanup
{
    /// Claude's settings.json, found as ClaudeConfig.Settings (the hook's Install.cs) finds it.
    public static string ClaudeSettings => Path.Combine(
        Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR") is { Length: > 0 } d ? d
            : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".claude"),
        "settings.json");

    /// Codex's user layer, where CodexConfig registers hooks: config.toml, or hooks.json.
    public static string[] CodexFiles => new[] { Path.Combine(Paths.CodexHome, "config.toml"), Path.Combine(Paths.CodexHome, "hooks.json") };

    /// The agents ("claude", "codex") whose config names one of these hook paths.
    public static List<string> Agents(IReadOnlyList<string> hooks, bool ignoreCase)
    {
        var agents = new List<string>();
        if (Names(Read(ClaudeSettings), hooks, ignoreCase)) agents.Add("claude");
        if (CodexFiles.Any(f => Names(Read(f), hooks, ignoreCase))) agents.Add("codex");
        return agents;
    }

    /// Whether a config file's text names one of the hook paths, with / or \ (settings.json has "/", Codex's command
    /// "\"), escaped as in JSON and TOML strings ("\\") or not (TOML literal strings), and as the whole file name: a
    /// path that only starts with it (aipet-hook.exe.old) isn't the hook.
    public static bool Names(string text, IReadOnlyList<string> hooks, bool ignoreCase)
    {
        if (string.IsNullOrEmpty(text)) return false;
        var t = Slashes(text.Replace(@"\\", @"\"));
        var how = ignoreCase ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        foreach (var hook in hooks.Where(h => !string.IsNullOrEmpty(h)))
        {
            // PowerShell's quoting (a path with a ' in it) doubles the quote in Codex's command
            foreach (var h in new[] { Slashes(hook), Slashes(hook.Replace("'", "''")) }.Distinct())
                for (int i = t.IndexOf(h, how); i >= 0; i = t.IndexOf(h, i + 1, how))
                {
                    int end = i + h.Length;
                    if (end == t.Length || !(char.IsLetterOrDigit(t[end]) || t[end] is '.' or '-' or '_')) return true;
                }
        }
        return false;
    }

    static string Slashes(string s) => s.Replace('\\', '/');

    static string Read(string path)
    {
        try { return File.Exists(path) ? File.ReadAllText(path) : null; }
        catch { return null; }
    }

    /// Runs `<hook> --uninstall <agent>` for each agent whose config names this hook: by its path, or with its folder
    /// in 8.3 short form, as Codex's command has it when the path has spaces (CodexConfig.BuildCommand). Each run gets
    /// a few seconds and no window. Nothing here throws, and the uninstall goes on whatever happens.
    public static void Run(string hook)
    {
        try
        {
            if (!File.Exists(hook)) return;
            var paths = new List<string> { hook };
            if (OperatingSystem.IsWindows())
            {
                if (ShortPath(Path.GetDirectoryName(hook)) is { } dir) paths.Add(Path.Combine(dir, Path.GetFileName(hook)));
                if (ShortPath(hook) is { } file) paths.Add(file);  // older installs shortened the file name too
            }
            foreach (var agent in Agents(paths, ignoreCase: OperatingSystem.IsWindows()))
                try
                {
                    var psi = new ProcessStartInfo(hook)
                    {
                        UseShellExecute = false, CreateNoWindow = true,
                        RedirectStandardOutput = true, RedirectStandardError = true,
                    };
                    psi.ArgumentList.Add("--uninstall");
                    psi.ArgumentList.Add(agent);
                    using var p = Process.Start(psi);
                    // read as it comes, so a full pipe can't hold the hook up
                    var output = p.StandardOutput.ReadToEndAsync();
                    var errors = p.StandardError.ReadToEndAsync();
                    if (!p.WaitForExit(10_000))
                    {
                        try { p.Kill(); } catch { }
                        Log.Write($"uninstall: aipet-hook --uninstall {agent} took too long; stopped it");
                        continue;
                    }
                    Log.Write($"uninstall: aipet-hook --uninstall {agent} (exit {p.ExitCode}): {(output.Result + errors.Result).Trim().ReplaceLineEndings(" ")}");
                }
                catch (Exception ex) { Log.Write($"uninstall: couldn't run aipet-hook --uninstall {agent}: {ex.Message}"); }
        }
        catch (Exception ex) { Log.Write("uninstall: couldn't check the agents' hooks: " + ex.Message); }
    }

    /// C:\Users\First Last\... -> C:\Users\FIRSTL~1\...; null if there's no short name.
    static string ShortPath(string path)
    {
        try
        {
            var buf = new char[1024];
            int n = GetShortPathNameW(path, buf, buf.Length);
            return n > 0 && n < buf.Length ? new string(buf, 0, n) : null;
        }
        catch { return null; }
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern int GetShortPathNameW(string path, char[] buf, int size);
}

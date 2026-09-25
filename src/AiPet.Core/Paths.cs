using System.IO;

namespace AiPet;

/// The app's data folder and the other places the app and the agent hooks both know. Shared by AiPet.Core/AiPet.UI
/// and AiPet.Hook (linked source file).
///   Windows: %LOCALAPPDATA%\AiPet
///   Linux:   ~/.local/share/AiPet
///   macOS:   ~/Library/Application Support/AiPet   (not supported yet)
public static class Paths
{
    static readonly string LocalAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
    static readonly string Home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);

    static readonly string DefaultDataDir = Path.Combine(
        string.IsNullOrEmpty(LocalAppData) ? Path.Combine(Home, ".local", "share") : LocalAppData, "AiPet");

    /// AIPET_DATA_DIR points the app and the hooks at another folder (for testing without touching the real one).
    public static readonly string DataDir =
        Environment.GetEnvironmentVariable("AIPET_DATA_DIR") is { Length: > 0 } dir ? Path.GetFullPath(dir) : DefaultDataDir;

    public static string Config => Path.Combine(DataDir, "config.json");
    public static string Log => Path.Combine(DataDir, "aipet.log");
    /// The app's line per hook event it got (trimmed to its last half past 64 KB), so you can see the agents run them.
    public static string HookEventsLog => Path.Combine(DataDir, "hook-events.log");

    /// ~/.codex, or $CODEX_HOME.
    public static string CodexHome =>
        Environment.GetEnvironmentVariable("CODEX_HOME") is { Length: > 0 } d ? d : Path.Combine(Home, ".codex");
}

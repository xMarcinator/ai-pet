namespace AiPet;

/// Where the app's own update stands, as Settings shows it. Only the copy the Windows installer put there updates
/// itself (Updates.cs in the app, with Velopack); any other copy is Off and never checks.
public sealed record UpdateStatus(UpdateStatus.Kinds Kind, string Version = null, int Percent = 0, string Error = null)
{
    public enum Kinds { Off, Idle, Checking, UpToDate, Downloading, Ready, Failed }

    /// The releases the installed app updates from (stable ones only, read without a token), and the Velopack channel
    /// release.yml packs the Windows app for (vpk pack --channel win): its feed is releases.win.json in each release.
    public const string Repo = "https://github.com/xMarcinator/ai-pet";
    public const string Channel = "win";

    /// The first check comes a while after the pet starts, the next ones every few hours. Unauthenticated GitHub API
    /// calls are limited per IP address (60 an hour, shared with everything else on the network), and each check
    /// makes one.
    public static readonly TimeSpan FirstCheck = TimeSpan.FromMinutes(3), Every = TimeSpan.FromHours(6);

    /// Whether the app in appDir is the installed one: Velopack's installer puts Update.exe in the folder above the
    /// app's (%LOCALAPPDATA%\AiPetApp\current), which is the first thing Velopack's own Windows locator looks for. The
    /// portable zip and a build run with `dotnet AiPet.dll` have none there.
    public static bool Installed(string appDir) => File.Exists(Path.Combine(appDir, "..", "Update.exe"));

    public bool Busy => Kind is Kinds.Checking or Kinds.Downloading;

    public string Text => Kind switch
    {
        Kinds.Off => "This copy doesn't update itself. To update, install AiPet again.",
        Kinds.Idle => "Checks for updates a few minutes after the pet starts, then every few hours.",
        Kinds.Checking => "Checking for updates…",
        Kinds.UpToDate => "Up to date.",
        Kinds.Downloading => $"Downloading AiPet {Version}… {Percent}%",
        Kinds.Ready => $"AiPet {Version} is ready. It's installed when you quit the pet, or restart now.",
        _ => Version == null ? $"Couldn't check for updates: {Error}" : $"Couldn't update to AiPet {Version}: {Error}",
    };
}

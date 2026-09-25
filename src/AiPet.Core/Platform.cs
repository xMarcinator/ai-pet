namespace AiPet;

/// Everything the pet needs from the operating system. One implementation per platform lives in
/// AiPet.UI/Platform (Windows, Linux, macOS stub); the rest of the app only talks to this interface.
public interface IPlatform
{
    string Name { get; }

    /// Stores the Jira API token etc. (Credential Manager / libsecret keyring / macOS Keychain).
    ISecretStore Secrets { get; }

    /// The music player the pet listens along with (Spotify via window title / MPRIS / AppleScript).
    IMediaPlayer Media { get; }

    /// Bring the agent's desktop app to the front ("claude", "codex"). False unless it's now really in front.
    /// It never runs the agent's CLI; when the app isn't open, at most its URL is opened (claude:// on Windows),
    /// and this still returns false.
    bool FocusAgent(string agent);

    /// Press Escape in the app FocusAgent brought forward (stops the running turn). Returns false, without
    /// pressing anything, if that app is no longer in front.
    Task<bool> SendEscape();

    void OpenUrl(string url);
    void OpenFolder(string path);

    /// Called once the pet window exists: hide from Alt+Tab, etc.
    void SetupPetWindow(IntPtr handle);
    /// Re-assert always-on-top (some window managers drop it).
    void KeepOnTop(IntPtr handle);
    /// Only these rectangles (physical pixels, window-relative) take mouse input; the rest clicks through.
    void SetInputRegion(IntPtr handle, IReadOnlyList<(int X, int Y, int W, int H)> rects);
}

public interface ISecretStore
{
    string Read(string key);
    void Write(string key, string user, string secret);
    /// Forget a saved secret (nothing happens if there is none).
    void Delete(string key);
}

public interface IMediaPlayer
{
    /// e.g. "Spotify"; null when this platform has no player integration.
    string Name { get; }
    string Song { get; }
    string Artist { get; }
    bool Playing { get; }
    /// Unix time the current track started showing (brings a dismissed bubble back on the next song).
    double TrackSince { get; }
    event Action Changed;
    /// Called about once a second on the UI thread.
    void Poll();
    void PlayPause();
    void Next();
    void Previous();
    void Focus();
}

/// Shared helper for players that report "artist - song" text.
public abstract class MediaPlayerBase : IMediaPlayer
{
    public abstract string Name { get; }
    public string Song { get; protected set; }
    public string Artist { get; protected set; }
    public bool Playing { get; protected set; }
    public double TrackSince { get; protected set; }
    public event Action Changed;

    public abstract void Poll();
    public abstract void PlayPause();
    public abstract void Next();
    public abstract void Previous();
    public virtual void Focus() { }

    /// Update the track; raises Changed when anything differs.
    protected void Set(string song, string artist, bool playing)
    {
        if (song != Song || artist != Artist)
        {
            if (song != null) TrackSince = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() / 1000.0;
        }
        else if (playing == Playing) return;
        Song = song;
        Artist = artist;
        Playing = playing;
        Changed?.Invoke();
    }
}

/// Logging to %LOCALAPPDATA%\AiPet\aipet.log (or ~/.local/share/AiPet/aipet.log).
public static class Log
{
    public static void Write(string line)
    {
        try
        {
            if (File.Exists(Paths.Log) && new FileInfo(Paths.Log).Length > 256 * 1024) File.Delete(Paths.Log);
            File.AppendAllText(Paths.Log, $"{DateTime.Now:HH:mm:ss.fff} {line}\n");
        }
        catch { }
    }
}

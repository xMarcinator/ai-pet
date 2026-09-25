using System.Diagnostics;
using System.Runtime.Versioning;

namespace AiPet.Platforms;

/// macOS is designed for but not supported yet: this compiles and keeps the app running, but nothing here
/// has been tested. Planned: osascript for Spotify and focusing apps, `security` for the Keychain,
/// and NSWindow.ignoresMouseEvents toggling for click-through.
[SupportedOSPlatform("macos")]
sealed class MacPlatform : IPlatform
{
    public string Name => "macOS (unsupported)";
    public ISecretStore Secrets { get; } = new MemorySecrets();
    public IMediaPlayer Media => null;

    public bool FocusAgent(string agent) => false;
    public Task<bool> SendEscape() => Task.FromResult(false);
    public void OpenUrl(string url) { try { Process.Start("open", url)?.Dispose(); } catch { } }
    public void OpenFolder(string path) => OpenUrl(path);
    public void SetupPetWindow(IntPtr handle) { }
    public void KeepOnTop(IntPtr handle) { }
    public void SetInputRegion(IntPtr handle, IReadOnlyList<(int X, int Y, int W, int H)> rects) { }

    sealed class MemorySecrets : ISecretStore
    {
        readonly Dictionary<string, string> store = new();
        public string Read(string key) => store.GetValueOrDefault(key);
        public void Write(string key, string user, string secret) => store[key] = secret;
        public void Delete(string key) => store.Remove(key);
    }
}

static class PlatformFactory
{
    public static IPlatform Create()
    {
        if (OperatingSystem.IsWindows()) return new WindowsPlatform();
        if (OperatingSystem.IsLinux()) return new LinuxPlatform();
        if (OperatingSystem.IsMacOS()) return new MacPlatform();
        throw new PlatformNotSupportedException();
    }
}

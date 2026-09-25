using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace AiPet.Platforms;

/// Linux (X11, or XWayland under Wayland). Uses standard desktop tools when present:
/// playerctl/dbus-send (MPRIS media), secret-tool (keyring), wmctrl/xdotool (focusing other apps),
/// xdg-open, and an X11 input shape for click-through.
[SupportedOSPlatform("linux")]
sealed class LinuxPlatform : IPlatform
{
    public string Name => "Linux";
    public ISecretStore Secrets { get; } = new SecretTool();
    public IMediaPlayer Media { get; } = new Mpris();

    static bool Has(string tool) => Sh.Which(tool) != null;

    public Task<bool> SendEscape() => Task.FromResult(Has("xdotool") && Sh.Run("xdotool", "key", "Escape") == 0);

    public bool FocusAgent(string agent)
    {
        // Window classes of the agents' desktop apps where they exist (e.g. unofficial Claude builds). Only an open
        // window is activated; nothing is started.
        var classes = agent == "codex" ? new[] { "chatgpt", "codex" } : new[] { "claude" };
        foreach (var cls in classes)
        {
            if (Has("wmctrl") && Sh.Run("wmctrl", "-x", "-a", cls) == 0) return true;
            if (Has("xdotool") && Sh.Run("xdotool", "search", "--class", cls, "windowactivate") == 0) return true;
        }
        return false;
    }

    public void OpenUrl(string url) => Sh.Start("xdg-open", null, url);
    public void OpenFolder(string path) => Sh.Start("xdg-open", null, path);

    // ------------------------------------------------------------------ the pet window
    public void SetupPetWindow(IntPtr window)
    {
        // Ask compositors that honour it (picom/compton) not to draw a drop shadow around the window.
        try
        {
            if (display == IntPtr.Zero) display = XOpenDisplay(IntPtr.Zero);
            if (display == IntPtr.Zero) return;
            var atom = XInternAtom(display, "_COMPTON_SHADOW", false);
            var zero = new[] { 0L };
            XChangeProperty(display, window, atom, (IntPtr)6 /* XA_CARDINAL */, 32, 0 /* PropModeReplace */, zero, 1);
            XFlush(display);
        }
        catch (DllNotFoundException) { }
        catch (EntryPointNotFoundException) { }
    }
    public void KeepOnTop(IntPtr handle) { }

    IntPtr display;

    public void SetInputRegion(IntPtr window, IReadOnlyList<(int X, int Y, int W, int H)> rects)
    {
        // X11 shapes: the bounding shape makes the window just its visible parts (so compositors don't draw a
        // shadow box around the whole rectangle); the input shape is where it takes the mouse.
        if (Environment.GetEnvironmentVariable("AIPET_NOSHAPE") == "1") return;
        try
        {
            if (display == IntPtr.Zero) display = XOpenDisplay(IntPtr.Zero);
            if (display == IntPtr.Zero) return;
            var xr = rects.Select(r => new XRectangle
            {
                x = (short)Math.Clamp(r.X, short.MinValue, short.MaxValue), y = (short)Math.Clamp(r.Y, short.MinValue, short.MaxValue),
                width = (ushort)Math.Clamp(r.W, 0, ushort.MaxValue), height = (ushort)Math.Clamp(r.H, 0, ushort.MaxValue),
            }).ToArray();
            XShapeCombineRectangles(display, window, ShapeBounding, 0, 0, xr, xr.Length, ShapeSet, Unsorted);
            XShapeCombineRectangles(display, window, ShapeInput, 0, 0, xr, xr.Length, ShapeSet, Unsorted);
            XFlush(display);
        }
        catch (DllNotFoundException) { }
        catch (EntryPointNotFoundException) { }
    }

    const int ShapeBounding = 0, ShapeInput = 2, ShapeSet = 0, Unsorted = 0;
    [StructLayout(LayoutKind.Sequential)] struct XRectangle { public short x, y; public ushort width, height; }
    [DllImport("libX11.so.6")] static extern IntPtr XOpenDisplay(IntPtr name);
    [DllImport("libX11.so.6")] static extern int XFlush(IntPtr display);
    [DllImport("libX11.so.6")] static extern IntPtr XInternAtom(IntPtr display, string name, bool onlyIfExists);
    [DllImport("libX11.so.6")]
    static extern int XChangeProperty(IntPtr display, IntPtr window, IntPtr property, IntPtr type, int format, int mode, long[] data, int count);
    [DllImport("libXext.so.6")]
    static extern void XShapeCombineRectangles(IntPtr display, IntPtr window, int kind, int x, int y,
                                               XRectangle[] rects, int n, int op, int ordering);
}

/// Small process helpers.
static class Sh
{
    static readonly Dictionary<string, string> which = new();

    public static string Which(string tool)
    {
        lock (which)
        {
            if (which.TryGetValue(tool, out var hit)) return hit;
            var found = (Environment.GetEnvironmentVariable("PATH") ?? "").Split(':')
                .Select(d => Path.Combine(d, tool)).FirstOrDefault(File.Exists);
            which[tool] = found;
            return found;
        }
    }

    public static int Run(string exe, params string[] args) => Capture(exe, args, out _, null);

    public static int Capture(string exe, string[] args, out string stdout, string stdin = null, int timeoutMs = 3000)
    {
        stdout = "";
        try
        {
            var psi = new ProcessStartInfo(exe)
            {
                UseShellExecute = false, RedirectStandardOutput = true, RedirectStandardError = true,
                RedirectStandardInput = stdin != null, StandardOutputEncoding = Encoding.UTF8,
            };
            foreach (var a in args) psi.ArgumentList.Add(a);
            using var p = Process.Start(psi);
            if (stdin != null) { p.StandardInput.Write(stdin); p.StandardInput.Close(); }
            var o = p.StandardOutput.ReadToEndAsync();
            if (!p.WaitForExit(timeoutMs)) { try { p.Kill(); } catch { } return -1; }
            stdout = o.Result;
            return p.ExitCode;
        }
        catch { return -1; }
    }

    /// Fire and forget, detached from us.
    public static bool Start(string exe, string dir, params string[] args)
    {
        try
        {
            var psi = new ProcessStartInfo(exe) { UseShellExecute = false, WorkingDirectory = dir ?? "" };
            foreach (var a in args) psi.ArgumentList.Add(a);
            Process.Start(psi)?.Dispose();
            return true;
        }
        catch { return false; }
    }
}

/// Secrets in the desktop keyring through libsecret's secret-tool; falls back to a file only you can read.
[SupportedOSPlatform("linux")]
sealed class SecretTool : ISecretStore
{
    static string FallbackFile => Path.Combine(Paths.DataDir, "secrets.json");

    public string Read(string key)
    {
        if (Sh.Which("secret-tool") != null &&
            Sh.Capture("secret-tool", new[] { "lookup", "service", "aipet", "key", key }, out var o) == 0 && o.Length > 0)
            return o.TrimEnd('\n');
        try { return JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(FallbackFile)).GetValueOrDefault(key); }
        catch { return null; }
    }

    public void Write(string key, string user, string secret)
    {
        if (Sh.Which("secret-tool") != null &&
            Sh.Capture("secret-tool", new[] { "store", "--label=AiPet " + key, "service", "aipet", "key", key, "user", user ?? "" },
                       out _, secret) == 0)
            return;
        // no keyring available: a 0600 file in the data folder
        Dictionary<string, string> all;
        try { all = JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(FallbackFile)) ?? new(); } catch { all = new(); }
        all[key] = secret;
        Directory.CreateDirectory(Paths.DataDir);
        File.WriteAllText(FallbackFile, JsonSerializer.Serialize(all));
        try { File.SetUnixFileMode(FallbackFile, UnixFileMode.UserRead | UnixFileMode.UserWrite); } catch { }
    }

    public void Delete(string key)
    {
        if (Sh.Which("secret-tool") != null) Sh.Run("secret-tool", "clear", "service", "aipet", "key", key);
        try
        {
            var all = JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(FallbackFile));
            if (all != null && all.Remove(key))
            {
                if (all.Count == 0) File.Delete(FallbackFile);
                else File.WriteAllText(FallbackFile, JsonSerializer.Serialize(all));
            }
        }
        catch { }
    }
}

/// MPRIS media players (Spotify first) through playerctl, or dbus-send when playerctl isn't installed.
[SupportedOSPlatform("linux")]
sealed class Mpris : MediaPlayerBase
{
    string playerName = "Spotify";
    public override string Name => playerName;
    bool busy;

    public override void Poll()
    {
        if (busy) return;
        busy = true;
        Task.Run(() =>
        {
            try
            {
                var (song, artist, playing, player) = Sh.Which("playerctl") != null ? ReadPlayerctl() : ReadDbus();
                if (player != null) playerName = player;
                Set(song, artist, playing);
            }
            catch { }
            finally { busy = false; }
        });
    }

    static (string, string, bool, string) ReadPlayerctl()
    {
        if (Sh.Capture("playerctl", new[] { "--player=spotify,%any", "metadata", "--format", "{{status}}\t{{artist}}\t{{title}}\t{{playerName}}" }, out var o) != 0
            || string.IsNullOrWhiteSpace(o))
            return (null, null, false, null);
        var f = o.TrimEnd('\n').Split('\t');
        if (f.Length < 4 || string.IsNullOrWhiteSpace(f[2])) return (null, null, false, null);
        var player = f[3].Length > 0 ? char.ToUpperInvariant(f[3][0]) + f[3][1..] : null;
        return (f[2], f[1], f[0] == "Playing", player);
    }

    // ---- dbus-send fallback
    static string Bus()
    {
        if (Sh.Capture("dbus-send", new[] { "--session", "--dest=org.freedesktop.DBus", "--type=method_call", "--print-reply",
                "/org/freedesktop/DBus", "org.freedesktop.DBus.ListNames" }, out var o) != 0) return null;
        var names = Regex.Matches(o, "\"(org\\.mpris\\.MediaPlayer2\\.[^\"]+)\"").Select(m => m.Groups[1].Value).ToList();
        return names.FirstOrDefault(n => n.Contains("spotify", StringComparison.OrdinalIgnoreCase)) ?? names.FirstOrDefault();
    }

    static string Prop(string bus, string name)
    {
        Sh.Capture("dbus-send", new[] { "--session", "--print-reply", "--dest=" + bus, "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties.Get", "string:org.mpris.MediaPlayer2.Player", "string:" + name }, out var o);
        return o;
    }

    static (string, string, bool, string) ReadDbus()
    {
        var bus = Bus();
        if (bus == null) return (null, null, false, null);
        var meta = Prop(bus, "Metadata");
        string After(string key)
        {
            var i = meta.IndexOf("\"" + key + "\"", StringComparison.Ordinal);
            if (i < 0) return null;
            var m = Regex.Match(meta[i..], "string \"((?:[^\"\\\\]|\\\\.)*)\"");
            return m.Success ? m.Groups[1].Value : null;
        }
        var title = After("xesam:title");
        if (string.IsNullOrWhiteSpace(title)) return (null, null, false, null);
        var status = Prop(bus, "PlaybackStatus");
        var player = bus.Split('.').Last();
        return (title, After("xesam:artist"), status.Contains("\"Playing\""), char.ToUpperInvariant(player[0]) + player[1..]);
    }

    void Command(string playerctl, string mpris)
    {
        Task.Run(() =>
        {
            if (Sh.Which("playerctl") != null) Sh.Run("playerctl", "--player=spotify,%any", playerctl);
            else if (Bus() is { } bus)
                Sh.Run("dbus-send", "--session", "--type=method_call", "--dest=" + bus, "/org/mpris/MediaPlayer2", "org.mpris.MediaPlayer2.Player." + mpris);
            Poll();
        });
    }

    public override void PlayPause() => Command("play-pause", "PlayPause");
    public override void Next() => Command("next", "Next");
    public override void Previous() => Command("previous", "Previous");
    public override void Focus()
    {
        if (Sh.Which("wmctrl") != null) Sh.Run("wmctrl", "-x", "-a", "spotify");
    }
}

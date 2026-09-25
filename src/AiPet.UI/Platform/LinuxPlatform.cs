using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace AiPet.Platforms;

/// Linux (X11, or XWayland under Wayland). Uses standard desktop tools when present:
/// playerctl/dbus-send (MPRIS media), secret-tool (keyring), wmctrl/xdotool (focusing other apps),
/// xdg-open, and an X11 input shape for click-through. Native Wayland windows can't be found or focused this way,
/// so for them FocusAgent is false and no key is ever pressed.
[SupportedOSPlatform("linux")]
sealed class LinuxPlatform : IPlatform
{
    public string Name => "Linux";
    public ISecretStore Secrets { get; } = new SecretTool();
    public IMediaPlayer Media { get; } = new Mpris();

    static bool Has(string tool) => Sh.Which(tool) != null;

    /// The X window FocusAgent brought forward; Escape is only pressed while it's still the active one.
    string focused;

    /// The window manager may refuse to activate a window (focus-stealing prevention) and the tools still exit 0,
    /// and xdotool's key goes to whichever window has focus. So a window only counts as in front once
    /// `xdotool getactivewindow` names it.
    static bool Active(string id) =>
        Sh.Capture("xdotool", new[] { "getactivewindow" }, out var o) == 0 &&
        long.TryParse(o.Trim(), out var active) && long.TryParse(id, out var want) && active == want;

    public Task<bool> SendEscape()
    {
        var target = focused;
        return Task.Run(async () =>
        {
            await Task.Delay(150);
            if (target == null || !Active(target)) { Log.Write("focus moved away; not sending keys"); return false; }
            return Sh.Run("xdotool", "key", "Escape") == 0;
        });
    }

    public bool FocusAgent(string agent, string link = null)
    {
        focused = null;
        // Window classes of the agents' desktop apps where they exist (e.g. unofficial Claude builds). Only an open
        // window is activated; at most the chat's link is opened, nothing is started.
        var classes = agent == "codex" ? new[] { "chatgpt", "codex" } : new[] { "claude" };
        foreach (var cls in classes)
        {
            // visible windows only: Electron apps have hidden helper windows of the same class
            if (Has("xdotool") && Sh.Capture("xdotool", new[] { "search", "--onlyvisible", "--class", cls }, out var ids) == 0 &&
                ids.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).FirstOrDefault() is { } id)
            {
                Sh.Run("xdotool", "windowactivate", id);
                for (int i = 0; !Active(id); i++)
                {
                    if (i == 10) { Log.Write($"couldn't bring {agent} to the front"); return false; }
                    Thread.Sleep(15);
                }
                focused = id;
                return true;
            }
            // wmctrl can bring a window forward (a minimised one too) but can't tell whether it came
            if (Has("wmctrl") && Sh.Run("wmctrl", "-x", "-a", cls) == 0) return false;
        }
        if (link != null) OpenUrl(link);
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
            // A fitted window (see MoveResize) changes size around the pet's bottom-centre. Until the frame for the new
            // size is drawn, the X server shows what was drawn before, and by default keeps it at the top-left: for a
            // moment the pet shows twice. South bit gravity keeps it at the bottom-centre, under the pet.
            if (FitsPetWindow)
            {
                var south = new XSetWindowAttributes { bit_gravity = SouthGravity };
                XChangeWindowAttributes(display, window, CWBitGravity, ref south);
                foreach (var child in Children(window)) XChangeWindowAttributes(display, child, CWBitGravity, ref south);
            }
            XFlush(display);
        }
        catch (DllNotFoundException) { }
        catch (EntryPointNotFoundException) { }
    }
    public void KeepOnTop(IntPtr handle) { }

    /// Some compositors ignore the X11 input shape (Hyprland: XWayland windows take the mouse on every pixel), so the
    /// window itself is kept to what the pet shows. AIPET_NOFIT=1 keeps it at its full size.
    public bool FitsPetWindow => Environment.GetEnvironmentVariable("AIPET_NOFIT") != "1";

    /// One request, so the window never shows the new size at the old place. The size hints go first: the pet's window
    /// can't be resized (min = max, as Avalonia sets them), which is also why window managers float it.
    public void MoveResize(IntPtr window, int x, int y, int width, int height)
    {
        try
        {
            if (display == IntPtr.Zero) display = XOpenDisplay(IntPtr.Zero);
            if (display == IntPtr.Zero) return;
            var hints = new XSizeHints
            {
                flags = PMinSize | PMaxSize, min_width = width, min_height = height, max_width = width, max_height = height,
            };
            XSetWMNormalHints(display, window, ref hints);
            XMoveResizeWindow(display, window, x, y, (uint)width, (uint)height);
            // the child window Avalonia renders into too, in the same go (it would resize it only once it hears of the
            // new size): with its south bit gravity, what it shows stays under the pet until the next frame
            foreach (var child in Children(window)) XMoveResizeWindow(display, child, 0, 0, (uint)width, (uint)height);
            XFlush(display);
        }
        catch (DllNotFoundException) { }
        catch (EntryPointNotFoundException) { }
    }

    IntPtr display;

    List<IntPtr> Children(IntPtr window)
    {
        var list = new List<IntPtr>();
        if (XQueryTree(display, window, out _, out _, out var children, out int n) != 0 && children != IntPtr.Zero)
        {
            for (int i = 0; i < n; i++) list.Add(Marshal.ReadIntPtr(children, i * IntPtr.Size));
            XFree(children);
        }
        return list;
    }

    public void SetInputRegion(IntPtr window, IReadOnlyList<(int X, int Y, int W, int H)> rects)
    {
        // X11 shapes: the bounding shape makes the window just its visible parts (so compositors don't draw a
        // shadow box around the whole rectangle); the input shape is where it takes the mouse. A fitted window
        // (FitsPetWindow) has no bounding shape: XWayland updates only what's inside it, so what was drawn outside
        // stays on screen (the pet where it was before the window grew, a bubble that went) and what's drawn there
        // never shows (the menu).
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
            if (FitsPetWindow) XShapeCombineMask(display, window, ShapeBounding, 0, 0, IntPtr.Zero, ShapeSet);
            else XShapeCombineRectangles(display, window, ShapeBounding, 0, 0, xr, xr.Length, ShapeSet, Unsorted);
            XShapeCombineRectangles(display, window, ShapeInput, 0, 0, xr, xr.Length, ShapeSet, Unsorted);
            XFlush(display);
        }
        catch (DllNotFoundException) { }
        catch (EntryPointNotFoundException) { }
    }

    const int ShapeBounding = 0, ShapeInput = 2, ShapeSet = 0, Unsorted = 0;
    [StructLayout(LayoutKind.Sequential)] struct XRectangle { public short x, y; public ushort width, height; }
    /// Xlib's XSizeHints (flags is a long).
    [StructLayout(LayoutKind.Sequential)]
    struct XSizeHints
    {
        public IntPtr flags;
        public int x, y, width, height, min_width, min_height, max_width, max_height, width_inc, height_inc;
        public int min_aspect_x, min_aspect_y, max_aspect_x, max_aspect_y, base_width, base_height, win_gravity;
    }
    static readonly IntPtr PMinSize = 1 << 4, PMaxSize = 1 << 5;
    /// Xlib's XSetWindowAttributes (unsigned long and pointer-sized fields as IntPtr, Bool as int).
    [StructLayout(LayoutKind.Sequential)]
    struct XSetWindowAttributes
    {
        public IntPtr background_pixmap, background_pixel, border_pixmap, border_pixel;
        public int bit_gravity, win_gravity, backing_store;
        public IntPtr backing_planes, backing_pixel;
        public int save_under;
        public IntPtr event_mask, do_not_propagate_mask;
        public int override_redirect;
        public IntPtr colormap, cursor;
    }
    const int SouthGravity = 8;
    const long CWBitGravity = 1 << 4;
    [DllImport("libX11.so.6")]
    static extern int XChangeWindowAttributes(IntPtr display, IntPtr window, long valueMask, ref XSetWindowAttributes attributes);
    [DllImport("libX11.so.6")]
    static extern int XQueryTree(IntPtr display, IntPtr window, out IntPtr root, out IntPtr parent, out IntPtr children, out int count);
    [DllImport("libX11.so.6")] static extern int XFree(IntPtr data);
    [DllImport("libX11.so.6")] static extern void XSetWMNormalHints(IntPtr display, IntPtr window, ref XSizeHints hints);
    [DllImport("libX11.so.6")] static extern int XMoveResizeWindow(IntPtr display, IntPtr window, int x, int y, uint width, uint height);
    [DllImport("libX11.so.6")] static extern IntPtr XOpenDisplay(IntPtr name);
    [DllImport("libX11.so.6")] static extern int XFlush(IntPtr display);
    [DllImport("libX11.so.6")] static extern IntPtr XInternAtom(IntPtr display, string name, bool onlyIfExists);
    [DllImport("libX11.so.6")]
    static extern int XChangeProperty(IntPtr display, IntPtr window, IntPtr property, IntPtr type, int format, int mode, long[] data, int count);
    /// With no mask (None), the window has no shape of that kind any more.
    [DllImport("libXext.so.6")]
    static extern void XShapeCombineMask(IntPtr display, IntPtr window, int kind, int x, int y, IntPtr mask, int op);
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

/// Secrets in the desktop keyring through libsecret's secret-tool; falls back to a file only you can read, from the
/// moment it exists.
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
        Save(all);
    }

    /// Writes a new file that is 0600 from the start and renames it over the old one, so no one else can have it open
    /// from a moment it was readable (earlier builds chmod'ed it only after writing).
    static void Save(Dictionary<string, string> all)
    {
        Directory.CreateDirectory(Paths.DataDir);
        var tmp = FallbackFile + "." + Guid.NewGuid().ToString("N")[..8] + ".tmp";
        try
        {
            using (var f = new FileStream(tmp, new FileStreamOptions
                   { Mode = FileMode.CreateNew, Access = FileAccess.Write, UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite }))
                JsonSerializer.Serialize(f, all);
            File.Move(tmp, FallbackFile, overwrite: true);
        }
        catch
        {
            try { File.Delete(tmp); } catch { }
            throw;
        }
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
                else Save(all);
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

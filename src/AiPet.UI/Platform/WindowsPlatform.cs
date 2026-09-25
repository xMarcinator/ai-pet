using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Text;

namespace AiPet.Platforms;

[SupportedOSPlatform("windows")]
sealed class WindowsPlatform : IPlatform
{
    public string Name => "Windows";
    public ISecretStore Secrets { get; } = new CredentialManager();
    public IMediaPlayer Media { get; } = new SpotifyWindowTitle();

    // ------------------------------------------------------------------ agents
    /// Desktop app per agent, and the URL that opens it when it isn't running. Codex runs inside the ChatGPT desktop
    /// app (ChatGPT.exe from the OpenAI.Codex package). Nothing opens that one: `codex app` runs PowerShell and
    /// starts a new chat in the pet's own folder.
    static (string[] Exes, string LaunchUrl) AgentApp(string agent) => agent switch
    {
        "codex" => (new[] { "chatgpt.exe", "codex.exe" }, null),
        _ => (new[] { "claude.exe" }, "claude://"),
    };

    /// The window FocusAgent brought forward; keys are only sent while that app is still in front.
    IntPtr focused;

    public bool FocusAgent(string agent)
    {
        focused = IntPtr.Zero;
        var app = AgentApp(agent);
        var h = FindWindowOf(app.Exes);
        if (h == IntPtr.Zero)
        {
            if (app.LaunchUrl != null) OpenUrl(app.LaunchUrl);
            Log.Write(app.LaunchUrl != null ? $"no {agent} window found; opening {app.LaunchUrl}" : $"no {agent} window found");
            return false;
        }
        Bring(h);
        for (int i = 0; i < 10 && !InFront(h); i++) Thread.Sleep(15);
        if (!InFront(h)) { Log.Write($"couldn't bring {agent} to the front"); return false; }
        focused = h;
        return true;
    }

    /// The foreground window belongs to the same process as h.
    static bool InFront(IntPtr h)
    {
        var fg = GetForegroundWindow();
        if (fg == IntPtr.Zero) return false;
        GetWindowThreadProcessId(fg, out uint a);
        GetWindowThreadProcessId(h, out uint b);
        return a == b;
    }

    internal static void Bring(IntPtr h)
    {
        if (IsIconic(h)) ShowWindow(h, 9);
        // No Alt-key trick: a lone Alt tap moves Electron apps (Claude, ChatGPT) into their menu bar and swallows
        // the key press that follows. We're normally the foreground app (the user just clicked the pet).
        if (!SetForegroundWindow(h))
        {
            uint fg = GetWindowThreadProcessId(GetForegroundWindow(), out _), me = GetCurrentThreadId();
            AttachThreadInput(me, fg, true);
            BringWindowToTop(h);
            SetForegroundWindow(h);
            AttachThreadInput(me, fg, false);
        }
    }

    internal static IntPtr FindWindowOf(string[] exes)
    {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, _) =>
        {
            // main windows only: skip owned pop-ups (menus, tooltips) and tool windows
            if (!IsWindowVisible(h) || GetWindow(h, 4 /* GW_OWNER */) != IntPtr.Zero || (GetWindowLong(h, -20) & 0x80) != 0) return true;
            var sb = new StringBuilder(256);
            GetWindowText(h, sb, 256);
            if (sb.Length > 0 && exes.Contains(ExeName(h))) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    static string ExeName(IntPtr hwnd)
    {
        GetWindowThreadProcessId(hwnd, out uint pid);
        var h = OpenProcess(0x1000, false, pid);
        if (h == IntPtr.Zero) return "";
        var sb = new StringBuilder(512); int n = 512;
        bool ok = QueryFullProcessImageName(h, 0, sb, ref n);
        CloseHandle(h);
        return ok ? Path.GetFileName(sb.ToString()).ToLowerInvariant() : "";
    }

    public Task<bool> SendEscape() => Keys(150, new byte[] { VK_ESCAPE });

    /// Press each key combo in turn, but only while the app FocusAgent brought forward is still in front, so
    /// nothing is ever typed into another window. False if it stopped (or nothing was focused).
    Task<bool> Keys(int delayMs, params byte[][] combos)
    {
        var target = focused;
        return Task.Run(async () =>
        {
            await Task.Delay(delayMs);
            foreach (var keys in combos)
            {
                if (target == IntPtr.Zero || !InFront(target)) { Log.Write("focus moved away; not sending keys"); return false; }
                foreach (var k in keys) keybd_event(k, 0, 0, UIntPtr.Zero);
                foreach (var k in keys.Reverse()) keybd_event(k, 0, 2, UIntPtr.Zero);
                await Task.Delay(120);
            }
            return true;
        });
    }

    public void OpenUrl(string url)
    {
        try { Process.Start(new ProcessStartInfo(url) { UseShellExecute = true }); } catch { }
    }

    public void OpenFolder(string path) => OpenUrl(path);

    // ------------------------------------------------------------------ the pet window
    public void SetupPetWindow(IntPtr h)
    {
        SetWindowLong(h, -20, GetWindowLong(h, -20) | 0x80);  // WS_EX_TOOLWINDOW: not in Alt+Tab
    }

    public void KeepOnTop(IntPtr h) => SetWindowPos(h, new IntPtr(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);

    public void SetInputRegion(IntPtr h, IReadOnlyList<(int X, int Y, int W, int H)> rects)
    {
        // The window region is where the pet can be clicked (and drawn); everything else falls through to
        // the windows behind. Rects are padded by the caller to include shadows.
        IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
        foreach (var (x, y, w, hgt) in rects)
        {
            var r = CreateRectRgn(x, y, x + w, y + hgt);
            CombineRgn(rgn, rgn, r, 2);  // RGN_OR
            DeleteObject(r);
        }
        if (SetWindowRgn(h, rgn, true) == 0) DeleteObject(rgn);  // on success the system owns the region
    }

    // ------------------------------------------------------------------ Win32
    const byte VK_ESCAPE = 0x1B;
    delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] static extern int GetWindowLong(IntPtr h, int i);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongW")] static extern int SetWindowLong(IntPtr h, int i, int v);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint f);
    [DllImport("user32.dll")] static extern int SetWindowRgn(IntPtr h, IntPtr rgn, bool redraw);
    [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int l, int t, int r, int b);
    [DllImport("gdi32.dll")] static extern int CombineRgn(IntPtr dest, IntPtr a, IntPtr b, int mode);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr h, int f, StringBuilder s, ref int n);
}

/// Windows Credential Manager (generic credentials).
[SupportedOSPlatform("windows")]
sealed class CredentialManager : ISecretStore
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct CREDENTIAL
    {
        public uint Flags, Type;
        public string TargetName, Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist, AttributeCount;
        public IntPtr Attributes;
        public string TargetAlias, UserName;
    }

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CredWrite(ref CREDENTIAL cred, uint flags);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CredRead(string target, uint type, uint flags, out IntPtr cred);
    [DllImport("advapi32.dll")] static extern void CredFree(IntPtr cred);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CredDelete(string target, uint type, uint flags);

    public void Delete(string key) => CredDelete(key, 1, 0);

    public void Write(string key, string user, string secret)
    {
        var blob = Marshal.StringToCoTaskMemUni(secret);
        try
        {
            var c = new CREDENTIAL
            {
                Type = 1, TargetName = key, UserName = user, Persist = 2,
                CredentialBlob = blob, CredentialBlobSize = (uint)(secret.Length * 2),
            };
            CredWrite(ref c, 0);
        }
        finally { Marshal.ZeroFreeCoTaskMemUnicode(blob); }
    }

    public string Read(string key)
    {
        if (!CredRead(key, 1, 0, out var p)) return null;
        try
        {
            var c = Marshal.PtrToStructure<CREDENTIAL>(p);
            return c.CredentialBlob == IntPtr.Zero ? null : Marshal.PtrToStringUni(c.CredentialBlob, (int)c.CredentialBlobSize / 2);
        }
        finally { CredFree(p); }
    }
}

/// Spotify on Windows: its main window title is "Artist - Song" while playing and "Spotify…" when paused;
/// controls go straight to that window as WM_APPCOMMAND so they can't hit another player.
[SupportedOSPlatform("windows")]
sealed class SpotifyWindowTitle : MediaPlayerBase
{
    public override string Name => "Spotify";
    IntPtr window;
    string lastTitle;

    public override void Poll()
    {
        window = Find(out var title);
        if (title == lastTitle) return;
        lastTitle = title;
        if (window == IntPtr.Zero) { Set(null, null, false); return; }
        if (string.IsNullOrWhiteSpace(title) || title.StartsWith("Spotify", StringComparison.OrdinalIgnoreCase) ||
            title.Equals("Advertisement", StringComparison.OrdinalIgnoreCase))
        {
            Set(Song, Artist, false);  // paused: keep the last track
            return;
        }
        var cut = title.IndexOf(" - ", StringComparison.Ordinal);
        Set(cut > 0 ? title[(cut + 3)..].Trim() : title.Trim(), cut > 0 ? title[..cut].Trim() : "", true);
    }

    const int WM_APPCOMMAND = 0x0319;
    public override void PlayPause() => Send(14);
    public override void Next() => Send(11);
    public override void Previous() => Send(12);
    public override void Focus() { if (window != IntPtr.Zero) WindowsPlatform.Bring(window); }

    void Send(int cmd)
    {
        if (window == IntPtr.Zero) window = Find(out _);
        if (window != IntPtr.Zero) SendMessage(window, WM_APPCOMMAND, window, (IntPtr)(cmd << 16));
        lastTitle = null;
    }

    readonly HashSet<uint> pids = new();
    DateTime pidsAt;

    IntPtr Find(out string title)
    {
        if (DateTime.Now - pidsAt > TimeSpan.FromSeconds(10))
        {
            pidsAt = DateTime.Now;
            pids.Clear();
            foreach (var p in Process.GetProcessesByName("Spotify")) { pids.Add((uint)p.Id); p.Dispose(); }
        }
        IntPtr found = IntPtr.Zero;
        string text = null;
        if (pids.Count > 0)
            EnumWindows((h, _) =>
            {
                GetWindowThreadProcessId(h, out uint pid);
                if (!pids.Contains(pid)) return true;
                var cls = new StringBuilder(64);
                GetClassName(h, cls, 64);
                if (!cls.ToString().StartsWith("Chrome_WidgetWin", StringComparison.Ordinal)) return true;
                var sb = new StringBuilder(512);
                GetWindowText(h, sb, 512);
                if (sb.Length == 0) return true;
                found = h; text = sb.ToString();
                return false;
            }, IntPtr.Zero);
        title = text;
        return found;
    }

    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h, int msg, IntPtr w, IntPtr l);
}

using System.Runtime.InteropServices;
using Avalonia;

namespace AiPet;

public static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        // Velopack first of all (Updates.App): the Windows installer, updater and uninstaller start the app with
        // arguments of their own, which it handles before it exits. Any other copy goes on at once. vpk pack checks
        // that Main calls Run.
        Updates.App().Run();

        // one pet per user, .NET or Rust: on Linux first the lock both take, then the mutex (named mutexes work on
        // Linux too, but only within a login session there)
        if (OperatingSystem.IsLinux() && !TakeLock()) return;
        Mutex single;
        bool created;
        try { single = new Mutex(true, @"Local\AiPetApp", out created); }
        // a pet started elevated owns a mutex only Administrators may open: that's another pet running too
        catch (UnauthorizedAccessException) { return; }
        using (single)
        {
            if (!created || OtherSessionsPet()) return;
            // false when an update downloaded earlier is installed first: Update.exe starts the pet again
            if (!Updates.Start()) return;
            BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
        }
    }

    /// Linux: a pet started from another session (install.sh starts it with setsid) holds a mutex of its own, but it
    /// answers on the user's socket. Without this check, this pet would run without the socket and show no chats.
    static bool OtherSessionsPet()
    {
        if (OperatingSystem.IsWindows()) return false;
        using var live = Ipc.Connect();
        return live != null;
    }

    /// Linux: the single-instance lock, which a .NET pet and a Rust pet both take before anything else, so two never
    /// run at once, whatever XDG_RUNTIME_DIR each got: an exclusive, non-blocking flock on LockFile. The file is
    /// opened 0600 without following a symlink, and the programs the pet starts don't inherit it. The lock is held
    /// until the pet ends: the file is never closed, and the kernel lets the lock go when the process ends, a crash
    /// included. False when another pet holds it: this one quits quietly, as it does for the mutex. A lock file that
    /// can't be opened or locked is logged, and the pet goes on to the mutex and the socket probe, as before the lock.
    static bool TakeLock()
    {
        var path = LockFile;
        // the socket's folder may be the data folder, which may not exist yet: for this user only, as the installers
        // make it
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path),
                UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
        }
        catch (Exception) { }  // then the open says what's wrong
        int fd = open(path, O_RDWR | O_CREAT | O_CLOEXEC | NoFollow, 0x180);  // 0600
        if (fd < 0)
        {
            Log.Write($"single instance: can't open {path}: {Marshal.GetPInvokeErrorMessage(Marshal.GetLastPInvokeError())}");
            return true;
        }
        int locked, error;
        do
        {
            locked = flock(fd, LOCK_EX | LOCK_NB);
            error = Marshal.GetLastPInvokeError();
        } while (locked != 0 && error == EINTR);
        if (locked == 0) return true;
        close(fd);
        if (error == EWOULDBLOCK) return false;
        Log.Write($"single instance: can't lock {path}: {Marshal.GetPInvokeErrorMessage(error)}");
        return true;
    }

    /// The lock's file, next to the socket: the socket's path with .lock in place of .sock, so it follows the socket's
    /// rule, or <AIPET_PIPE>.lock with that override.
    static string LockFile => Environment.GetEnvironmentVariable("AIPET_PIPE") is { Length: > 0 }
        ? Ipc.Endpoint + ".lock"
        : Path.ChangeExtension(Ipc.Endpoint, ".lock");

    // Linux's open(2) and flock(2). O_NOFOLLOW is the flag whose value depends on the architecture.
    const int O_RDWR = 0x2, O_CREAT = 0x40, O_CLOEXEC = 0x80000, LOCK_EX = 2, LOCK_NB = 4, EINTR = 4, EWOULDBLOCK = 11;
    static int NoFollow =>
        RuntimeInformation.ProcessArchitecture is Architecture.Arm or Architecture.Arm64 or Architecture.Ppc64le ? 0x8000 : 0x20000;
    [DllImport("libc", SetLastError = true)]
    static extern int open([MarshalAs(UnmanagedType.LPUTF8Str)] string path, int flags, uint mode);
    [DllImport("libc", SetLastError = true)] static extern int flock(int fd, int operation);
    [DllImport("libc")] static extern int close(int fd);

    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>().UsePlatformDetect()
            // the pet doesn't need X11 session management; skipping it avoids needing libICE/libSM
            .With(new X11PlatformOptions
            {
                EnableSessionManagement = false,
                // menus and tooltips drawn inside their window, not as windows of their own: a compositor can stack
                // the pet's window above those (Hyprland does when it's pinned), and with focus following the mouse,
                // moving onto one takes the focus from the pet's window, which closes a menu
                OverlayPopups = true,
                // AIPET_SOFTWARE=1 skips OpenGL, for virtual GPUs (e.g. WSLg) where GL output doesn't show up
                RenderingMode = Environment.GetEnvironmentVariable("AIPET_SOFTWARE") == "1"
                    ? new[] { X11RenderingMode.Software }
                    : new[] { X11RenderingMode.Glx, X11RenderingMode.Software },
            })
            .LogToTrace();
}

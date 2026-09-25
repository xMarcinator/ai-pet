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

        // one pet per user (named mutexes work on Linux too, but only within a login session there)
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

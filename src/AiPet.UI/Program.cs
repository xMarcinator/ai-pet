using Avalonia;

namespace AiPet;

public static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        // one pet per user (named mutexes work on Linux too, but only within a login session there)
        using var single = new Mutex(true, @"Local\AiPetApp", out bool created);
        if (!created || OtherSessionsPet()) return;
        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
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
                // AIPET_SOFTWARE=1 skips OpenGL, for virtual GPUs (e.g. WSLg) where GL output doesn't show up
                RenderingMode = Environment.GetEnvironmentVariable("AIPET_SOFTWARE") == "1"
                    ? new[] { X11RenderingMode.Software }
                    : new[] { X11RenderingMode.Glx, X11RenderingMode.Software },
            })
            .LogToTrace();
}

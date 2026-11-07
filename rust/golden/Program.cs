using System.Runtime.CompilerServices;

namespace AiPet.Golden;

/// The golden generators for the Rust port. The real C# turns fixed inputs into expected outputs, and the Rust tests
/// replay them byte for byte (or bit for bit). One mode per run, each in a file of its own:
///
///   dotnet run --project rust/golden -c Release [-- mode]
///
///   sprite         the default: the pet's frames, for rust/crates/aipet-sprite (Sprite.cs)
///   registration   --install/--uninstall claude|codex and --print-plugin-hooks (Registration.cs)
///   doctor         --doctor claude|codex; Doctor.cs doesn't compile outside the hook, so it runs the built C# hook
///   sessions       AgentSessions' outcomes and snapshots (Sessions.cs)
///   data           config.json and the other data files, both ways (Data.cs)
///   board          the Board's bubbles (Board.cs)
///   ipc            the endpoint and data paths in this process's environment, for aipet-ipc's tests (IpcMode.cs)
///
/// Every mode is written.
///
/// Nothing touches the user's pet or data. Every mode but ipc runs with AIPET_DATA_DIR pointing Core at a temp folder,
/// set before Paths is first used, and removed afterwards; a mode sets up anything else it needs (a temp
/// CLAUDE_CONFIG_DIR or CODEX_HOME, say) itself. ipc reads the environment it was started with, and writes nothing.
static class Program
{
    static int Main(string[] args)
    {
        string mode = args.Length > 0 ? args[0] : "sprite";
        string[] rest = args.Length > 0 ? args[1..] : [];
        if (mode == "ipc") return IpcMode.Run(rest);
        Func<string, string, string[], int> run = mode switch
        {
            "sprite" => SpriteMode.Run,
            "registration" => RegistrationMode.Run,
            "doctor" => DoctorMode.Run,
            "sessions" => SessionsMode.Run,
            "data" => DataMode.Run,
            "board" => BoardMode.Run,
            _ => null,
        };
        if (run == null) return Usage();

        string repo = RepoRoot();
        string data = Path.Combine(Path.GetTempPath(), "aipet-golden-" + Guid.NewGuid().ToString("N"));
        // before anything reads Paths.DataDir (the modes check that it's this folder)
        Environment.SetEnvironmentVariable("AIPET_DATA_DIR", data);
        try
        {
            return run(repo, data, rest);
        }
        finally
        {
            if (Directory.Exists(data)) Directory.Delete(data, recursive: true);
        }
    }

    internal static int Usage()
    {
        Console.Error.WriteLine("usage: dotnet run --project rust/golden -c Release [-- sprite|registration|doctor|sessions|data|board|ipc]");
        return 2;
    }

    /// What a mode that isn't written yet does.
    internal static int NotYet(string mode)
    {
        Console.Error.WriteLine($"the golden mode {mode} isn't written yet");
        return 2;
    }

    static string RepoRoot([CallerFilePath] string self = "") =>
        Path.GetFullPath(Path.Combine(Path.GetDirectoryName(self), "..", ".."));
}

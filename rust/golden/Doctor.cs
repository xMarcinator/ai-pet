namespace AiPet.Golden;

/// Mode doctor: `--doctor claude|codex [--probe]` in sandboxed scenarios. src/AiPet.Hook/Doctor.cs doesn't compile
/// outside the hook's project, so this runs the built C# hook as a subprocess and records its output and exit code.
/// Replayed by rust/crates/aipet-hook/tests. Not written yet.
static class DoctorMode
{
    public static int Run(string repo, string data, string[] args) => Program.NotYet("doctor");
}

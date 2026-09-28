namespace AiPet.Golden;

/// Mode registration: for each fixture, `--install`/`--uninstall claude|codex` run through the linked hook sources
/// (Install.cs, CodexConfig.cs, PluginHooks.cs) against temp config folders, and the files they leave; also
/// `--print-plugin-hooks`. Replayed by rust/crates/aipet-hook/tests. Not written yet.
static class RegistrationMode
{
    public static int Run(string repo, string data, string[] args) => Program.NotYet("registration");
}

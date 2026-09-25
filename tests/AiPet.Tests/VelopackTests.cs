using System.IO;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;
using System.Text.RegularExpressions;
using Xunit;

namespace AiPet.Tests;

/// The Windows app's Velopack side: which hooks its uninstaller removes (HookCleanup), what Settings says about
/// updates, and that the app, release.yml and the installers agree on the channel, the versions and the repository.
public class VelopackTests
{
    const string Hook = @"C:\Users\Ann\AppData\Local\AiPetApp\current\aipet-hook.exe";
    static readonly string[] Hooks = { Hook };

    // ------------------------------------------------------------------ which configs name this install's hook
    [Theory]
    // settings.json, as ClaudeConfig writes it ("/"), and as a user might ("\\" in JSON)
    [InlineData("""{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe","args":["--agent","claude"]}]}]}}""")]
    [InlineData("""{"command":"C:\\Users\\Ann\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe"}""")]
    // Windows paths are case-insensitive
    [InlineData("""{"command":"c:/users/ann/appdata/local/aipetapp/current/AIPET-HOOK.EXE"}""")]
    // config.toml: a basic string (CodexConfig), a literal string (as Codex may rewrite it), PowerShell's form
    [InlineData("command = \"C:\\\\Users\\\\Ann\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe --agent codex\"\n")]
    [InlineData("command = 'C:\\Users\\Ann\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe --agent codex'\n")]
    [InlineData("command = \"& 'C:\\\\Users\\\\Ann\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe' --agent codex\"\n")]
    public void Names_ThisInstallsHook(string config) => Assert.True(HookCleanup.Names(config, Hooks, ignoreCase: true));

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    // another copy: the portable zip, a build from source, the plugin's launcher
    [InlineData("""{"command":"C:/Tools/AiPet/aipet-hook.exe"}""")]
    [InlineData("""{"command":"C:/Users/Ann/src/ai-pet/artifacts/win-x64/hook/aipet-hook.exe"}""")]
    [InlineData("""{"command":"sh","args":["${CLAUDE_PLUGIN_ROOT}/native/aipet-hook.sh","--agent","claude"]}""")]
    // a file whose name only starts with the hook's
    [InlineData("""{"command":"C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe.old"}""")]
    // an install elsewhere
    [InlineData("""{"command":"D:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe"}""")]
    public void DoesntName_AnotherHook(string config) => Assert.False(HookCleanup.Names(config, Hooks, ignoreCase: true));

    [Fact]
    public void Names_CaseMattersWhereTheFileSystemSaysSo() =>
        Assert.False(HookCleanup.Names("""{"command":"c:/users/ann/appdata/local/aipetapp/current/aipet-hook.exe"}""", Hooks, ignoreCase: false));

    /// Codex's command has the folder in 8.3 form when the path has spaces (CodexConfig.BuildCommand), and a path with
    /// a ' in it has the quote doubled when PowerShell quoting is all that's left.
    [Fact]
    public void Names_TheShortAndTheQuotedForms()
    {
        const string spaces = @"C:\Users\Ann Smith\AppData\Local\AiPetApp\current\aipet-hook.exe";
        const string shortForm = @"C:\Users\ANNSMI~1\AppData\Local\AiPetApp\current\aipet-hook.exe";
        var toml = "command = \"C:\\\\Users\\\\ANNSMI~1\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe --agent codex\"\n";
        Assert.True(HookCleanup.Names(toml, new[] { spaces, shortForm }, ignoreCase: true));
        Assert.False(HookCleanup.Names(toml, new[] { spaces }, ignoreCase: true));

        const string quote = @"C:\Users\O'Brien\AppData\Local\AiPetApp\current\aipet-hook.exe";
        var json = """{"command":"& 'C:\\Users\\O''Brien\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe' --agent codex"}""";
        Assert.True(HookCleanup.Names(json, new[] { quote }, ignoreCase: true));
    }

    // ------------------------------------------------------------------ the status Settings shows
    [Fact]
    public void Status_Texts()
    {
        Assert.Contains("doesn't update itself", new UpdateStatus(UpdateStatus.Kinds.Off).Text);
        Assert.Equal("Up to date.", new UpdateStatus(UpdateStatus.Kinds.UpToDate).Text);
        Assert.Equal("Downloading AiPet 0.3.0… 42%", new UpdateStatus(UpdateStatus.Kinds.Downloading, "0.3.0", 42).Text);
        Assert.StartsWith("AiPet 0.3.0 is ready.", new UpdateStatus(UpdateStatus.Kinds.Ready, "0.3.0").Text);
        Assert.Equal("Couldn't check for updates: No such host is known.",
                     new UpdateStatus(UpdateStatus.Kinds.Failed, Error: "No such host is known.").Text);
        Assert.Equal("Couldn't update to AiPet 0.3.0: disk full", new UpdateStatus(UpdateStatus.Kinds.Failed, "0.3.0", Error: "disk full").Text);
        Assert.True(new UpdateStatus(UpdateStatus.Kinds.Checking).Busy);
        Assert.False(new UpdateStatus(UpdateStatus.Kinds.Ready, "0.3.0").Busy);
    }

    // ------------------------------------------------------------------ in step with release.yml and the installers
    static string Release => File.ReadAllText(Path.Combine(Scripts.Repo, ".github", "workflows", "release.yml"));

    /// The app looks for releases.<channel>.json in the releases, which vpk pack names after its --channel.
    [Fact]
    public void Channel_IsTheOneReleaseYmlPacks()
    {
        var pack = Regex.Match(Release, @"""\$vpk"" pack (.*?)--outputDir", RegexOptions.Singleline).Value;
        Assert.Contains("--packId AiPetApp ", pack);
        Assert.Contains($"--channel {UpdateStatus.Channel} ", pack);
        Assert.DoesNotContain("--skipVeloAppCheck", Release);
        Assert.Contains($"releases.{UpdateStatus.Channel}.json", Release);
    }

    /// vpk warns when the app's Velopack library isn't its own version.
    [Fact]
    public void VpkVersion_IsThePackageVersion()
    {
        var vpk = Regex.Match(Release, @"^\s*VPK_VERSION:\s*(\S+)", RegexOptions.Multiline).Groups[1].Value;
        var csproj = File.ReadAllText(Path.Combine(Scripts.Repo, "src", "AiPet.UI", "AiPet.UI.csproj"));
        var package = Regex.Match(csproj, @"<PackageReference Include=""Velopack"" Version=""([^""]+)""").Groups[1].Value;
        Assert.NotEqual("", vpk);
        Assert.Equal(vpk, package);
    }

    [Fact]
    public void Repo_IsTheOneTheInstallersUse()
    {
        Assert.Equal("https://github.com/xMarcinator/ai-pet", UpdateStatus.Repo);
        Assert.Contains("$repo = 'xMarcinator/ai-pet'", File.ReadAllText(Path.Combine(Scripts.Repo, "install.ps1")));
    }

    /// Only the installed copy uses Velopack's own locator: the portable zip and `dotnet AiPet.dll` have no Update.exe
    /// above their folder, and Velopack would log on each of their starts that it found no install.
    [Fact]
    public void Installed_IsTheCopyWithUpdateExeAbove()
    {
        var root = TestEnv.NewDir("AiPetApp");
        var current = Directory.CreateDirectory(Path.Combine(root, "current")).FullName;
        Assert.False(UpdateStatus.Installed(current + Path.DirectorySeparatorChar));
        File.WriteAllText(Path.Combine(root, "Update.exe"), "");
        Assert.True(UpdateStatus.Installed(current + Path.DirectorySeparatorChar));
        Assert.True(UpdateStatus.Installed(current));
        Assert.False(UpdateStatus.Installed(root));
    }

    /// vpk pack refuses an app whose Main doesn't call VelopackApp.Run, and Velopack's installer calls back into the
    /// app before anything else may happen: the call comes before the single-instance mutex.
    [Fact]
    public void Main_RunsVelopackFirst()
    {
        using var app = new Il(ResourceTests.AppDll);
        var main = app.EntryPoint;
        int run = app.At(main, "Velopack.VelopackApp", "Run"), mutex = app.At(main, "System.Threading.Mutex", ".ctor");
        Assert.True(run >= 0, "Program.Main doesn't call VelopackApp.Run");
        Assert.True(mutex >= 0, "Program.Main doesn't make the single-instance mutex");
        Assert.True(run < mutex, "Program.Main makes the mutex before it runs Velopack");
    }

    /// Velopack's own install of an update downloaded earlier runs before the mutex, where a second pet would have
    /// Update.exe stop the running one. It's off, and Updates.Start does it once Main knows this is the only pet.
    [Fact]
    public void PendingUpdate_IsInstalledByTheOnlyPet()
    {
        using var app = new Il(ResourceTests.AppDll);
        var main = app.EntryPoint;
        int mutex = app.At(main, "System.Threading.Mutex", ".ctor"), start = app.At(main, "AiPet.Updates", "Start"),
            avalonia = app.At(main, "Avalonia.ClassicDesktopStyleApplicationLifetimeExtensions", "StartWithClassicDesktopLifetime");
        Assert.True(mutex >= 0 && start > mutex, "Program.Main doesn't start the updates after the single-instance mutex");
        Assert.True(avalonia > start, "Program.Main starts Avalonia before the updates");

        var build = app.Method("AiPet.Updates", "App");
        int off = app.At(build, "Velopack.VelopackApp", "SetAutoApplyOnStartup");
        Assert.True(off > 0, "Updates.App doesn't set Velopack's auto-apply");
        Assert.Equal(0x16, app.Body(build)[off - 1]);  // ldc.i4.0: false
    }

    /// Reads the app's IL, for the checks vpk makes and the order Main does things in.
    sealed class Il : IDisposable
    {
        readonly PEReader pe;
        readonly MetadataReader md;

        public Il(string dll)
        {
            pe = new PEReader(File.OpenRead(dll));
            md = pe.GetMetadataReader();
        }

        public void Dispose() => pe.Dispose();

        public MethodDefinition EntryPoint =>
            md.GetMethodDefinition(MetadataTokens.MethodDefinitionHandle(pe.PEHeaders.CorHeader.EntryPointTokenOrRelativeVirtualAddress));

        public MethodDefinition Method(string type, string name) =>
            md.MethodDefinitions.Select(md.GetMethodDefinition)
              .Single(m => Name(md.GetTypeDefinition(m.GetDeclaringType())) == type && md.GetString(m.Name) == name);

        public byte[] Body(MethodDefinition method) => pe.GetMethodBody(method.RelativeVirtualAddress).GetILBytes();

        string Name(TypeDefinition t) => $"{md.GetString(t.Namespace)}.{md.GetString(t.Name)}";
        string Name(TypeReference t) => $"{md.GetString(t.Namespace)}.{md.GetString(t.Name)}";

        /// The first call or newobj of the member in method (a byte scan: a false match would need a real token too).
        public int At(MethodDefinition method, string type, string member)
        {
            var il = Body(method);
            for (int i = 0; i + 5 <= il.Length; i++)
            {
                if (il[i] is not (0x28 or 0x6F or 0x73)) continue;  // call, callvirt, newobj
                int token = BitConverter.ToInt32(il, i + 1), row = token & 0xFFFFFF;
                string t = null, name = null;
                if (token >> 24 == 0x0A && row <= md.GetTableRowCount(TableIndex.MemberRef))  // a MemberRef
                {
                    var r = md.GetMemberReference(MetadataTokens.MemberReferenceHandle(row));
                    if (r.Parent.Kind != HandleKind.TypeReference) continue;
                    (t, name) = (Name(md.GetTypeReference((TypeReferenceHandle)r.Parent)), md.GetString(r.Name));
                }
                else if (token >> 24 == 0x06 && row <= md.GetTableRowCount(TableIndex.MethodDef))  // a method of the app's
                {
                    var m = md.GetMethodDefinition(MetadataTokens.MethodDefinitionHandle(row));
                    (t, name) = (Name(md.GetTypeDefinition(m.GetDeclaringType())), md.GetString(m.Name));
                }
                if (t == type && name == member) return i;
            }
            return -1;
        }
    }
}

/// HookCleanup against configs the hook's own registration code wrote, in temp config folders. Like the registration
/// tests, these point CLAUDE_CONFIG_DIR and CODEX_HOME at their own folders, so they run alone.
[Collection(RegistrationCollection.Name)]
public sealed class HookCleanupTests : IDisposable
{
    const string Installed = @"C:\Users\Ann\AppData\Local\AiPetApp\current\aipet-hook.exe";
    readonly string claudeDir = TestEnv.NewDir("claude"), codexHome = TestEnv.NewDir("codex");
    readonly string oldClaude = Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR"), oldCodex = Environment.GetEnvironmentVariable("CODEX_HOME");

    public HookCleanupTests()
    {
        Environment.SetEnvironmentVariable("CLAUDE_CONFIG_DIR", claudeDir);
        Environment.SetEnvironmentVariable("CODEX_HOME", codexHome);
    }

    public void Dispose()
    {
        Environment.SetEnvironmentVariable("CLAUDE_CONFIG_DIR", oldClaude);
        Environment.SetEnvironmentVariable("CODEX_HOME", oldCodex);
    }

    /// The uninstaller reads the files the hook registers in.
    [Fact]
    public void Files_AreTheOnesTheHookRegistersIn()
    {
        Assert.Equal(ClaudeConfig.Settings, HookCleanup.ClaudeSettings);
        Assert.Equal(new[] { CodexConfig.ConfigToml, CodexConfig.HooksJson }, HookCleanup.CodexFiles);
    }

    [Fact]
    public void Agents_WhoseConfigNamesTheHook()
    {
        Assert.Empty(HookCleanup.Agents(new[] { Installed }, ignoreCase: true));

        Assert.Equal(0, ClaudeConfig.Install(Installed));
        Assert.Equal(new[] { "claude" }, HookCleanup.Agents(new[] { Installed }, ignoreCase: true));
        Assert.Equal(0, CodexConfig.Install(Installed));
        Assert.Equal(new[] { "claude", "codex" }, HookCleanup.Agents(new[] { Installed }, ignoreCase: true));
        // another copy's uninstaller leaves them alone
        Assert.Empty(HookCleanup.Agents(new[] { @"C:\Tools\AiPet\aipet-hook.exe" }, ignoreCase: true));

        Assert.Equal(0, ClaudeConfig.Uninstall());
        Assert.Equal(new[] { "codex" }, HookCleanup.Agents(new[] { Installed }, ignoreCase: true));
    }

    /// Codex's hooks in hooks.json (where CodexConfig puts them when the user's other hooks are there).
    [Fact]
    public void Agents_FindsCodexHooksJson()
    {
        File.WriteAllText(CodexConfig.HooksJson, """{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-tool"}]}]}}""");
        Assert.Equal(0, CodexConfig.Install(Installed));
        Assert.Contains("aipet-hook", File.ReadAllText(CodexConfig.HooksJson));
        Assert.Equal(new[] { "codex" }, HookCleanup.Agents(new[] { Installed }, ignoreCase: true));
    }

    /// Run starts `<hook> --uninstall <agent>` for each agent that names it, and only those. The hook here is a stand-in
    /// script that notes its arguments (so this runs where sh does).
    [UnixFact]
    public void Run_UninstallsFromTheAgentsThatNameTheHook()
    {
        var dir = TestEnv.NewDir("app");
        var hook = Path.Combine(dir, "aipet-hook");
        var calls = Path.Combine(dir, "calls");
        Scripts.Executable(hook, $"#!/bin/sh\necho \"$*\" >> '{calls}'\necho removed\n");
        File.WriteAllText(HookCleanup.ClaudeSettings, $$$"""{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"{{{hook}}}"}]}]}}""");
        File.WriteAllText(CodexConfig.ConfigToml, "[[hooks.Stop]]\n[[hooks.Stop.hooks]]\ncommand = \"'/elsewhere/aipet-hook' --agent codex\"\n");

        HookCleanup.Run(hook);
        Assert.Equal(new[] { "--uninstall claude" }, File.ReadAllLines(calls));

        // a hook that's gone (a broken install) has nothing to run
        File.Delete(calls);
        HookCleanup.Run(Path.Combine(dir, "missing", "aipet-hook"));
        Assert.False(File.Exists(calls));
    }

    /// A hook that hangs is stopped, and Run returns well within the 30 s Velopack gives the uninstall hook.
    [UnixFact]
    public void Run_StopsAHookThatHangs()
    {
        var dir = TestEnv.NewDir("app");
        var hook = Path.Combine(dir, "aipet-hook");
        Scripts.Executable(hook, "#!/bin/sh\nexec sleep 60\n");
        File.WriteAllText(HookCleanup.ClaudeSettings, $$"""{"command":"{{hook}}"}""");
        var clock = System.Diagnostics.Stopwatch.StartNew();
        HookCleanup.Run(hook);
        Assert.InRange(clock.Elapsed.TotalSeconds, 9, 20);
    }
}

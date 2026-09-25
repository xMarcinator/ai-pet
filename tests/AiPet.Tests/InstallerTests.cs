using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Xunit;

namespace AiPet.Tests;

/// packaging/linux/install.sh, the installer in the Linux tarball, run from a package folder the test makes: its
/// AiPet only writes down the environment it got, and its aipet-hook is the real hook (dotnet aipet-hook.dll).
public class PackageInstallerTests
{
    readonly Scripts.Sandbox box = new();
    readonly string package;

    public PackageInstallerTests()
    {
        package = MakePackage(box, "0.2.0");
        // pkill and pgrep would stop the user's own pet; the menu caches are the user's too
        box.Stub("pkill", "exit 1");
        box.Stub("pgrep", "exit 1");
        box.Stub("gtk-update-icon-cache", "exit 0");
        box.Stub("update-desktop-database", "exit 0");
    }

    string App => Path.Combine(box.Data, "AiPet", "app");

    /// AiPet-<version>-<rid>/ with install.sh, aipet.desktop.in, app/AiPet and app/aipet-hook.
    internal static string MakePackage(Scripts.Sandbox box, string version)
    {
        var dir = box.Dir($"AiPet-{version}-{Rid}");
        File.Copy(Path.Combine(Scripts.Repo, "packaging", "linux", "install.sh"), Path.Combine(dir, "install.sh"));
        File.Copy(Path.Combine(Scripts.Repo, "packaging", "linux", "aipet.desktop.in"), Path.Combine(dir, "aipet.desktop.in"));
        // install.sh compares the ELF header's machine byte (offset 18) with this machine's, so the script has it there
        var pet = new List<byte>(Encoding.ASCII.GetBytes("#!/bin/sh\n#1234567"));
        pet.Add(RuntimeInformation.OSArchitecture == Architecture.Arm64 ? (byte)0xb7 : (byte)0x3e);
        pet.AddRange(Encoding.ASCII.GetBytes("\nenv > \"$AIPET_TEST_OUT/pet-env.tmp\" && mv \"$AIPET_TEST_OUT/pet-env.tmp\" \"$AIPET_TEST_OUT/pet-env\"\n"));
        Directory.CreateDirectory(Path.Combine(dir, "app"));
        File.WriteAllBytes(Path.Combine(dir, "app", "AiPet"), pet.ToArray());
        Scripts.Executable(Path.Combine(dir, "app", "aipet-hook"),
            $"#!/bin/sh\necho \"$*\" >> \"$AIPET_TEST_OUT/hook-calls\"\nexec '{TestEnv.Dotnet}' '{TestEnv.HookDll}' \"$@\"\n");
        return dir;
    }

    internal static string Rid => RuntimeInformation.OSArchitecture == Architecture.Arm64 ? "linux-arm64" : "linux-x64";

    Scripts.Result Install(params string[] args)
    {
        var r = box.Sh(Path.Combine(package, "install.sh"), args);
        Assert.True(r.Exit == 0, r.Output);
        return r;
    }

    string PetEnv()
    {
        var file = Path.Combine(box.Out, "pet-env");
        for (int i = 0; i < 100 && !File.Exists(file); i++) Thread.Sleep(100);
        Assert.True(File.Exists(file), "the installer didn't start the pet");
        return File.ReadAllText(file);
    }

    /// The pet runs for days and opens browsers and apps, so it mustn't keep a token the installer was given.
    [UnixFact]
    public void Install_StartsThePet_WithoutTheToken()
    {
        box.Env["GITHUB_TOKEN"] = "test-token-4242";
        box.Env["DISPLAY"] = ":99";
        Install();
        var env = PetEnv();
        Assert.Contains("AIPET_TEST_OUT=", env);
        Assert.DoesNotContain("GITHUB_TOKEN", env);
        Assert.DoesNotContain("test-token-4242", env);
    }

    /// Older versions wrote a sign-in entry, which would keep starting the pet: the installer removes it. A copy of
    /// the menu entry (Icon=aipet) is the user's own choice and stays until the uninstall.
    [UnixFact]
    public void Install_RemovesAnOlderSignInEntry_ButNotTheUsersOwn()
    {
        var old = Path.Combine(box.Home, ".config", "autostart", "aipet.desktop");
        var own = Path.Combine(box.Config, "autostart", "aipet.desktop");
        Directory.CreateDirectory(Path.GetDirectoryName(old));
        Directory.CreateDirectory(Path.GetDirectoryName(own));
        File.WriteAllText(old, $"[Desktop Entry]\nType=Application\nName=AiPet\nExec=\"{App}/AiPet\"\n");
        File.WriteAllText(own, $"[Desktop Entry]\nType=Application\nName=AiPet\nExec=\"{App}/AiPet\"\nIcon=aipet\nX-GNOME-Autostart-enabled=true\n");

        Install("--no-start");
        Assert.False(File.Exists(old));
        Assert.True(File.Exists(own));

        var r = box.Sh(Path.Combine(App, "uninstall.sh"));
        Assert.True(r.Exit == 0, r.Output);
        Assert.False(File.Exists(own));
        Assert.False(Directory.Exists(App));
    }

    /// Hooks registered at the app's own aipet-hook would fail on every event once the uninstall deletes it.
    [UnixFact]
    public void Uninstall_RemovesTheHooksRegisteredAtTheAppsHook()
    {
        Install("--no-start");
        var hook = Path.Combine(App, "aipet-hook");
        var settings = Path.Combine(box.Claude, "settings.json");
        File.WriteAllText(settings, $$"""
            {
              "hooks": {
                "Stop": [
                  { "hooks": [ { "type": "command", "command": "{{hook}}", "args": ["--agent", "claude"], "timeout": 5 } ] },
                  { "hooks": [ { "type": "command", "command": "/usr/local/bin/other-tool" } ] }
                ]
              }
            }
            """);
        var hooksJson = Path.Combine(box.Codex, "hooks.json");
        File.WriteAllText(hooksJson, $$"""
            { "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "'{{hook}}' --agent codex", "timeout": 5 } ] } ] } }
            """);

        var r = box.Sh(Path.Combine(App, "uninstall.sh"));
        Assert.True(r.Exit == 0, r.Output);
        Assert.False(Directory.Exists(App));
        var calls = File.ReadAllLines(Path.Combine(box.Out, "hook-calls"));
        Assert.Equal(new[] { "--uninstall claude", "--uninstall codex" }, calls);
        Assert.DoesNotContain("aipet-hook", File.ReadAllText(settings));
        Assert.Contains("/usr/local/bin/other-tool", File.ReadAllText(settings));
        Assert.False(File.Exists(hooksJson) && File.ReadAllText(hooksJson).Contains("aipet-hook"));  // no hooks left: no file
    }

    /// aipet-hook --install registers its own path with symlinks resolved (/proc/self/exe), so with the data folder
    /// behind a symlink the config names the real folder, not the one the installer uses.
    [UnixFact]
    public void Uninstall_RemovesTheHooksRegisteredAtTheAppsHook_BehindASymlink()
    {
        var link = Path.Combine(box.Root, "data-link");
        File.CreateSymbolicLink(link, box.Data);
        box.Env["XDG_DATA_HOME"] = link;
        Install("--no-start");
        var real = box.Run(Path.Combine(Scripts.Tools, "sh"), new[] { "-c", "cd \"$1\" && pwd -P", "sh", App }).Stdout.Trim();
        Assert.StartsWith("/", real);
        var settings = Path.Combine(box.Claude, "settings.json");
        File.WriteAllText(settings, $$"""
            { "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "{{real}}/aipet-hook", "args": ["--agent", "claude"] } ] } ] } }
            """);

        var r = box.Sh(Path.Combine(link, "AiPet", "app", "uninstall.sh"));
        Assert.True(r.Exit == 0, r.Output);
        Assert.False(Directory.Exists(App));
        Assert.Equal(new[] { "--uninstall claude" }, File.ReadAllLines(Path.Combine(box.Out, "hook-calls")));
        Assert.False(File.Exists(settings) && File.ReadAllText(settings).Contains("aipet-hook"));
    }

    /// --uninstall removes every AiPet hook, so hooks that run another aipet-hook (a build from source) keep theirs.
    [UnixFact]
    public void Uninstall_LeavesTheHooksOfAnotherAiPetHookAlone()
    {
        Install("--no-start");
        var settings = Path.Combine(box.Claude, "settings.json");
        var text = $$"""
            { "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "{{box.Data}}/AiPet/hooks/aipet-hook", "args": ["--agent", "claude"] } ] } ] } }
            """;
        File.WriteAllText(settings, text);

        var r = box.Sh(Path.Combine(App, "uninstall.sh"));
        Assert.True(r.Exit == 0, r.Output);
        Assert.False(Directory.Exists(App));
        Assert.False(File.Exists(Path.Combine(box.Out, "hook-calls")));
        Assert.Equal(text, File.ReadAllText(settings));
    }
}

/// The one-line installer (install.sh at the root) against a release the test makes: curl is a stand-in that serves
/// the release's files, and the package's install.sh only writes down how it was run.
public class OneLineInstallerTests
{
    readonly Scripts.Sandbox box = new();

    public OneLineInstallerTests()
    {
        var release = box.Dir("release");
        var package = box.Dir($"AiPet-0.2.0-{PackageInstallerTests.Rid}");
        Scripts.Executable(Path.Combine(package, "install.sh"),
            "#!/bin/sh\nenv > \"$AIPET_TEST_OUT/package-env\"\necho \"$*\" > \"$AIPET_TEST_OUT/package-args\"\n");
        var name = $"AiPet-0.2.0-{PackageInstallerTests.Rid}.tar.gz";
        var tar = box.Run(Path.Combine(Scripts.Tools, "tar"), new[] { "-C", box.Root, "-czf", Path.Combine(release, name), Path.GetFileName(package) });
        Assert.True(tar.Exit == 0, tar.Output);
        var sum = Convert.ToHexStringLower(SHA256.HashData(File.ReadAllBytes(Path.Combine(release, name))));
        File.WriteAllText(Path.Combine(release, "SHA256SUMS"), $"{sum}  {name}\n");
        var api = "https://api.github.com/repos/xMarcinator/ai-pet/releases/assets";
        File.WriteAllText(Path.Combine(release, "release.json"),
            $"{{\"tag_name\":\"v0.2.0\",\"assets\":[{{\"url\":\"{api}/1\",\"name\":\"{name}\"}},{{\"url\":\"{api}/2\",\"name\":\"SHA256SUMS\"}}]}}");
        box.Stub("curl", $$"""
            out= url=
            while [ $# -gt 0 ]; do
              case $1 in
                -o) out=$2; shift ;;
                -H|--retry) shift ;;
                -*) ;;
                *) url=$1 ;;
              esac
              shift
            done
            case $url in
              */releases/latest|*/releases/tags/v0.2.0) f=release.json ;;
              */releases/assets/1|*/releases/download/v0.2.0/{{name}}) f={{name}} ;;
              */releases/assets/2|*/releases/download/v0.2.0/SHA256SUMS) f=SHA256SUMS ;;
              *) echo "curl: (22) The requested URL returned error: 404 ($url)" >&2; exit 22 ;;
            esac
            cp "{{release}}/$f" "$out"
            """);
        box.Stub("claude", "echo \"$*\" >> \"$AIPET_TEST_OUT/claude-calls\"");
    }

    Scripts.Result Install()
    {
        var r = box.Sh(Path.Combine(Scripts.Repo, "install.sh"));
        Assert.True(r.Exit == 0, r.Output);
        return r;
    }

    /// The package's installer (and the pet it starts) never needs the token, so it doesn't get it.
    [UnixFact]
    public void Install_RunsThePackagesInstaller_WithoutTheToken()
    {
        box.Env["GITHUB_TOKEN"] = "test-token-4242";
        Install();
        var env = File.ReadAllText(Path.Combine(box.Out, "package-env"));
        Assert.Contains("AIPET_TEST_OUT=", env);
        Assert.DoesNotContain("test-token-4242", env);
        Assert.Contains("plugin install aipet@aipet", File.ReadAllText(Path.Combine(box.Out, "claude-calls")));
    }

    /// AIPET_VERSION picks the app's release, but the plugin comes from the marketplace on main (the latest
    /// release's): the installer says so.
    [UnixTheory]
    [InlineData(true)]
    [InlineData(false)]
    public void AipetVersion_SaysThePluginComesFromTheLatestRelease(bool pinned)
    {
        if (pinned) box.Env["AIPET_VERSION"] = "0.2.0";
        var r = Install();
        Assert.Equal(pinned, r.Stderr.Contains("AIPET_VERSION pins only the app"));
    }
}

/// The docs run these as ./build.sh and so on, which needs the executable bit (git records it).
public class ScriptModeTests
{
    [UnixTheory]
    [InlineData("build.sh")]
    [InlineData("install.sh")]
    [InlineData("packaging/linux/install.sh")]
    [InlineData("scripts/check-plugin.sh")]
    [InlineData("scripts/install-from-source.sh")]
    public void ShellScripts_AreExecutable(string script)
    {
        Assert.True(File.GetUnixFileMode(Path.Combine(Scripts.Repo, script)).HasFlag(UnixFileMode.UserExecute), script + " isn't executable");
    }
}

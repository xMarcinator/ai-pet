using System.Diagnostics;
using System.IO;
using Xunit;

namespace AiPet.Tests;

/// The plugin's launcher, plugins/aipet/native/aipet-hook.sh, run with sh as the plugin runs it, with a fake uname on
/// PATH and fake hooks next to it. Whatever it finds, it prints nothing and exits 0; it runs a hook only where one can
/// load. Unix only (Git Bash on Windows isn't something CI has).
public class LauncherTests
{
    static readonly string Script = Path.Combine(Repo(), "plugins", "aipet", "native", "aipet-hook.sh");

    static string Repo()
    {
        for (var dir = AppContext.BaseDirectory; dir != null; dir = Path.GetDirectoryName(dir))
            if (File.Exists(Path.Combine(dir, "AiPet.slnx"))) return dir;
        throw new DirectoryNotFoundException("no AiPet.slnx above " + AppContext.BaseDirectory);
    }

    /// A plugin folder with the launcher, and a fake hook at each rid that records its arguments and stdin.
    sealed class Plugin
    {
        public readonly string Root = TestEnv.NewDir("plugin"), Native, Bin, Ran, UnameCalls;

        public Plugin()
        {
            Native = Directory.CreateDirectory(Path.Combine(Root, "native")).FullName;
            Bin = Directory.CreateDirectory(Path.Combine(Root, "fakebin")).FullName;
            Ran = Path.Combine(Root, "ran");
            UnameCalls = Path.Combine(Root, "uname-calls");
            File.Copy(Script, Path.Combine(Native, "aipet-hook.sh"));
            foreach (var rid in new[] { "linux-x64/aipet-hook", "linux-arm64/aipet-hook", "win-x64/aipet-hook.exe" })
                Hook(rid, $"#!/bin/sh\necho \"{rid.Split('/')[0]} $*\" >> '{Ran}'\ncat >> '{Ran}'\n");
        }

        /// A file at native/<rid>, executable unless said otherwise.
        public void Hook(string rid, string content, bool executable = true)
        {
            var path = Path.Combine(Native, rid);
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            Put(path, content, executable ? "755" : "644");
        }

        public void Uname(string answer) => Put(Path.Combine(Bin, "uname"), $"#!/bin/sh\necho \"$*\" >> '{UnameCalls}'\necho '{answer}'\n", "755");

        static void Put(string path, string content, string mode) => Scripts.Write(path, content, mode);

        /// Runs `sh <script> --agent claude` with the event on stdin; the exit code, stdout and stderr.
        public (int Exit, string Stdout, string Stderr) Run(string os = null, bool relative = false)
        {
            var psi = new ProcessStartInfo("sh")
            {
                UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
                WorkingDirectory = relative ? Native : Root,
            };
            psi.ArgumentList.Add(relative ? "aipet-hook.sh" : Path.Combine(Native, "aipet-hook.sh"));
            psi.ArgumentList.Add("--agent");
            psi.ArgumentList.Add("claude");
            psi.Environment["PATH"] = Bin + ":/usr/bin:/bin";
            psi.Environment.Remove("OS");
            if (os != null) psi.Environment["OS"] = os;
            using var p = Process.Start(psi);
            // the launcher may exit (for a platform it doesn't run on) without reading its stdin
            try { p.StandardInput.Write("{\"hook_event_name\":\"Stop\"}"); } catch (IOException) { }
            try { p.StandardInput.Close(); } catch (IOException) { }
            var stdout = p.StandardOutput.ReadToEndAsync();
            var stderr = p.StandardError.ReadToEndAsync();
            if (!p.WaitForExit(10000))
            {
                try { p.Kill(true); } catch { }
                Assert.Fail("the launcher didn't exit within 10 s");
            }
            return (p.ExitCode, stdout.Result, stderr.Result);
        }

        public string RanWith => File.Exists(Ran) ? File.ReadAllText(Ran) : null;
        public int Unames => File.Exists(UnameCalls) ? File.ReadAllLines(UnameCalls).Length : 0;
    }

    static readonly bool X64Loader = File.Exists("/lib64/ld-linux-x86-64.so.2"), Arm64Loader = File.Exists("/lib/ld-linux-aarch64.so.1");

    [UnixTheory]
    [InlineData("Linux x86_64", "linux-x64")]
    [InlineData("Linux amd64", "linux-x64")]
    [InlineData("Linux aarch64", "linux-arm64")]
    [InlineData("Linux arm64", "linux-arm64")]
    [InlineData("MINGW64_NT-10.0-19045 x86_64", "win-x64")]
    [InlineData("MSYS_NT-10.0-19045 x86_64", "win-x64")]
    [InlineData("CYGWIN_NT-10.0 x86_64", "win-x64")]
    [InlineData("Linux armv7l", null)]
    [InlineData("Linux i686", null)]
    [InlineData("Linux riscv64", null)]
    [InlineData("Darwin arm64", null)]
    [InlineData("FreeBSD amd64", null)]
    [InlineData("", null)]
    public void RunsOnlyTheHookThisPlatformCanLoad(string uname, string rid)
    {
        // a Linux build runs only with its glibc loader there (not on musl, nor a 32-bit userland on a 64-bit kernel)
        if (rid == "linux-x64" && !X64Loader || rid == "linux-arm64" && !Arm64Loader) rid = null;
        var plugin = new Plugin();
        plugin.Uname(uname);
        Assert.Equal((0, "", ""), plugin.Run());
        Assert.Equal(rid == null ? null : $"{rid} --agent claude\n{{\"hook_event_name\":\"Stop\"}}", plugin.RanWith);
        Assert.Equal(1, plugin.Unames);
    }

    /// Windows sets OS for every program, so Git Bash needs no uname at all.
    [UnixFact]
    public void OnWindows_RunsTheWindowsHook_WithoutUname()
    {
        var plugin = new Plugin();
        plugin.Uname("Linux x86_64");
        Assert.Equal((0, "", ""), plugin.Run(os: "Windows_NT"));
        Assert.Equal("win-x64 --agent claude\n{\"hook_event_name\":\"Stop\"}", plugin.RanWith);
        Assert.Equal(0, plugin.Unames);
    }

    /// Run as `sh aipet-hook.sh`, from its own folder.
    [UnixFact]
    public void FindsTheHook_FromARelativePath()
    {
        var plugin = new Plugin();
        Assert.Equal((0, "", ""), plugin.Run(os: "Windows_NT", relative: true));
        Assert.Equal("win-x64 --agent claude\n{\"hook_event_name\":\"Stop\"}", plugin.RanWith);
    }

    /// The hook's stdout reaches the agent as it is (the real one prints nothing, but the launcher mustn't eat it).
    [UnixFact]
    public void PassesTheHooksStdoutThrough()
    {
        var plugin = new Plugin();
        plugin.Hook("win-x64/aipet-hook.exe", "#!/bin/sh\necho out\n");
        Assert.Equal((0, "out\n", ""), plugin.Run(os: "Windows_NT"));
    }

    public static TheoryData<string, string, bool> Broken => new()
    {
        { "missing", null, true },
        { "not executable, made so", "#!/bin/sh\necho made >> '{ran}'\n", false },
        { "fails loudly", "#!/bin/sh\necho oops >&2\nexit 3\n", true },
        { "missing loader", "#!/nonexistent/ld-linux.so.2\n", true },
        { "not a program", "\u007fELF\u0002\u0001garbage", true },
        { "a folder", null, true },
    };

    /// A hook that isn't there or can't run (a glibc older than the build's, say) still ends in a quiet 0.
    [UnixTheory]
    [MemberData(nameof(Broken))]
    public void ABrokenHook_EndsInAQuietZero(string what, string content, bool executable)
    {
        var plugin = new Plugin();
        var hook = Path.Combine(plugin.Native, "win-x64", "aipet-hook.exe");
        File.Delete(hook);
        if (what == "a folder") Directory.CreateDirectory(hook);
        else if (content != null) plugin.Hook("win-x64/aipet-hook.exe", content.Replace("{ran}", plugin.Ran), executable);
        Assert.Equal((0, "", ""), plugin.Run(os: "Windows_NT"));
        Assert.Equal(what == "not executable, made so" ? "made\n" : null, plugin.RanWith);
    }
}

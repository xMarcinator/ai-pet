using System.Diagnostics;
using System.IO;
using Xunit;

namespace AiPet.Tests;

/// A test of the shell scripts (installers, release.yml's steps). They need sh, so they don't run on Windows.
// Tests for one kind of platform, reported as skipped elsewhere (not as passed, which a test that returns early is).
public sealed class UnixFactAttribute : FactAttribute
{
    public UnixFactAttribute(string why = "runs sh scripts") { if (OperatingSystem.IsWindows()) Skip = why; }
}

public sealed class UnixTheoryAttribute : TheoryAttribute
{
    public UnixTheoryAttribute(string why = "runs sh scripts") { if (OperatingSystem.IsWindows()) Skip = why; }
}

public sealed class LinuxFactAttribute : FactAttribute
{
    public LinuxFactAttribute(string why = "Linux only") { if (!OperatingSystem.IsLinux()) Skip = why; }
}

public sealed class LinuxTheoryAttribute : TheoryAttribute
{
    public LinuxTheoryAttribute(string why = "Linux only") { if (!OperatingSystem.IsLinux()) Skip = why; }
}

public sealed class WindowsFactAttribute : FactAttribute
{
    public WindowsFactAttribute(string why = "Windows only") { if (!OperatingSystem.IsWindows()) Skip = why; }
}

/// Runs the repository's shell scripts in a sandbox: HOME, the XDG folders, the agents' config folders, the data
/// folder and TMPDIR are fresh folders, and PATH has only the tools the scripts need (Tools) behind the test's own
/// stand-ins (Bin). So the real claude and codex are never found, and pkill never reaches the user's pet.
static class Scripts
{
    public static readonly string Repo = FindRepo();

    static string FindRepo()
    {
        for (var d = new DirectoryInfo(AppContext.BaseDirectory); d != null; d = d.Parent)
            if (File.Exists(Path.Combine(d.FullName, "AiPet.slnx"))) return d.FullName;
        throw new InvalidOperationException("no AiPet.slnx above " + AppContext.BaseDirectory);
    }

    static readonly string[] ToolNames =
    {
        "sh", "bash", "cat", "cp", "mv", "rm", "mkdir", "chmod", "ln", "sed", "awk", "grep", "tr", "head", "tail", "od",
        "uname", "id", "dirname", "basename", "mktemp", "tar", "gzip", "sha256sum", "env", "sleep", "setsid", "nohup",
        "base64", "sort", "ls", "touch", "find", "jq", "git", "printf", "wc", "cut", "readlink", "diff", "cmp", "tee", "true", "false",
    };

    static readonly Lazy<string> tools = new(() =>
    {
        var dir = TestEnv.NewDir("tools");
        var path = (Environment.GetEnvironmentVariable("PATH") ?? "").Split(':', StringSplitOptions.RemoveEmptyEntries);
        foreach (var name in ToolNames)
            if (path.Select(p => Path.Combine(p, name)).FirstOrDefault(File.Exists) is { } exe)
                File.CreateSymbolicLink(Path.Combine(dir, name), exe);
        return dir;
    });

    /// Links to the system's copies of ToolNames, and nothing else.
    public static string Tools => tools.Value;

    public sealed record Result(int Exit, string Stdout, string Stderr)
    {
        public string Output => Stdout + Stderr;
    }

    /// One test's sandbox.
    public sealed class Sandbox
    {
        public readonly string Root = TestEnv.NewDir("sandbox");
        public string Home => Dir("home");
        public string Config => Dir("config");
        public string Data => Dir("data");
        public string Claude => Dir("claude");
        public string Codex => Dir("codex");
        /// Where the stand-ins write what they saw.
        public string Out => Dir("out");
        /// The stand-ins, first on PATH.
        public string Bin => Dir("bin");
        public readonly Dictionary<string, string> Env = new();

        public string Dir(string name) => Directory.CreateDirectory(Path.Combine(Root, name)).FullName;

        /// A stand-in command on PATH (a sh script).
        public void Stub(string name, string body) => Executable(Path.Combine(Bin, name), "#!/bin/sh\n" + body + "\n");

        /// The environment a script runs with: nothing of the user's, apart from what the test adds.
        public Dictionary<string, string> Environment()
        {
            var env = new Dictionary<string, string>
            {
                ["PATH"] = Bin + ":" + Tools, ["HOME"] = Home, ["XDG_CONFIG_HOME"] = Config, ["XDG_DATA_HOME"] = Data,
                ["XDG_STATE_HOME"] = Dir("state"), ["XDG_CACHE_HOME"] = Dir("cache"), ["CLAUDE_CONFIG_DIR"] = Claude,
                ["CODEX_HOME"] = Codex, ["AIPET_DATA_DIR"] = Dir("aipet"), ["TMPDIR"] = Dir("tmp"), ["AIPET_PIPE"] = TestEnv.NoPet(),
                ["AIPET_TEST_OUT"] = Out, ["LC_ALL"] = "C", ["GIT_CONFIG_NOSYSTEM"] = "1",
            };
            foreach (var (k, v) in Env) env[k] = v;
            return env;
        }

        /// Runs a program (by path) with the sandbox's environment, stdin closed.
        public Result Run(string exe, IEnumerable<string> args, string cwd = null, int timeoutMs = 60000)
        {
            var psi = new ProcessStartInfo(exe)
            {
                UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
                WorkingDirectory = cwd ?? Root,
            };
            foreach (var a in args) psi.ArgumentList.Add(a);
            psi.Environment.Clear();
            foreach (var (k, v) in Environment()) psi.Environment[k] = v;
            using var p = Process.Start(psi);
            p.StandardInput.Close();
            var stdout = p.StandardOutput.ReadToEndAsync();
            var stderr = p.StandardError.ReadToEndAsync();
            if (!p.WaitForExit(timeoutMs))
            {
                try { p.Kill(true); } catch { }
                Assert.Fail($"{exe} didn't exit within {timeoutMs} ms");
            }
            return new Result(p.ExitCode, stdout.Result, stderr.Result);
        }

        public Result Sh(string script, params string[] args) => Run(Path.Combine(Tools, "sh"), args.Prepend(script));

        /// Runs a script as GitHub Actions runs a `run:` step with shell bash.
        public Result Step(string script, string cwd)
        {
            var file = Path.Combine(Root, "step-" + Guid.NewGuid().ToString("N")[..8] + ".sh");
            File.WriteAllText(file, script);
            return Run(Path.Combine(Tools, "bash"), new[] { "--noprofile", "--norc", "-eo", "pipefail", file }, cwd);
        }

        /// git in a folder, which must succeed.
        public string Git(string cwd, params string[] args)
        {
            var r = Run(Path.Combine(Tools, "git"), new[] { "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "init.defaultBranch=main" }.Concat(args), cwd);
            Assert.True(r.Exit == 0, $"git {string.Join(' ', args)}: {r.Output}");
            return r.Stdout.Trim();
        }
    }

    public static void Executable(string path, string text) => Write(path, text, "700");

    /// Written by a shell of its own: a program this process wrote itself can't be run while any child it is starting
    /// meanwhile (the other tests start hooks and scripts) still holds the file open (ETXTBSY, "Text file busy").
    public static void Write(string path, string text, string mode)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path));
        if (OperatingSystem.IsWindows()) { File.WriteAllText(path, text); return; }
        var psi = new ProcessStartInfo("sh") { UseShellExecute = false, RedirectStandardInput = true };
        foreach (var arg in new[] { "-c", "cat > \"$1\" && chmod \"$2\" \"$1\"", "sh", path, mode }) psi.ArgumentList.Add(arg);
        using var p = Process.Start(psi);
        p.StandardInput.Write(text);
        p.StandardInput.Close();
        p.WaitForExit();
        Assert.Equal(0, p.ExitCode);
    }

    /// The `run: |` script of the step with this name in a workflow, as the runner gets it.
    public static string WorkflowStep(string workflow, string step)
    {
        var lines = File.ReadAllLines(Path.Combine(Repo, ".github", "workflows", workflow));
        int at = Array.FindIndex(lines, l => l.Trim() == "- name: " + step);
        Assert.True(at >= 0, $"{workflow} has no step named '{step}'");
        int stepIndent = lines[at].IndexOf('-');
        int run = -1;
        for (int i = at + 1; i < lines.Length && run < 0; i++)
        {
            var t = lines[i].TrimStart();
            if (t.Length > 0 && lines[i].Length - t.Length <= stepIndent) break;  // the next step
            if (t == "run: |") run = i;
        }
        Assert.True(run >= 0, $"step '{step}' in {workflow} has no run: | block");
        int runIndent = lines[run].Length - lines[run].TrimStart().Length;
        var body = new List<string>();
        for (int i = run + 1; i < lines.Length; i++)
        {
            var t = lines[i].TrimStart();
            if (t.Length > 0 && lines[i].Length - t.Length <= runIndent) break;
            body.Add(lines[i]);
        }
        int indent = body.Where(l => l.Trim().Length > 0).Min(l => l.Length - l.TrimStart().Length);
        return string.Join("\n", body.Select(l => l.Length >= indent ? l[indent..] : "")) + "\n";
    }

    /// The lines of the step with this name, for checking its keys (if:, env:).
    public static string WorkflowStepText(string workflow, string step)
    {
        var lines = File.ReadAllLines(Path.Combine(Repo, ".github", "workflows", workflow));
        int at = Array.FindIndex(lines, l => l.Trim() == "- name: " + step);
        Assert.True(at >= 0, $"{workflow} has no step named '{step}'");
        int stepIndent = lines[at].IndexOf('-');
        var text = new List<string> { lines[at] };
        for (int i = at + 1; i < lines.Length; i++)
        {
            var t = lines[i].TrimStart();
            if (t.Length > 0 && lines[i].Length - t.Length <= stepIndent) break;
            text.Add(lines[i]);
        }
        return string.Join("\n", text);
    }
}

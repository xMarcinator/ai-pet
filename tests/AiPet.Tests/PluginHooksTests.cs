using System.Diagnostics;
using System.IO;
using Xunit;

namespace AiPet.Tests;

/// aipet-hook --print-plugin-hooks prints the plugin's hook files as committed, byte for byte: CI compares the two
/// (scripts/check-plugin.sh --hook), and Codex's file is frozen, so the code must make exactly it.
public class PluginHooksTests
{
    public sealed record Printed(int Exit, byte[] Stdout, string Stderr, string[] Written);

    /// `dotnet aipet-hook.dll <args>` with stdin left open (a hook run would wait for it), and a temp folder and a
    /// data folder of its own, which say whether it wrote anything.
    static Printed Print(params string[] args)
    {
        Assert.True(File.Exists(TestEnv.HookDll), "the hook isn't built at " + TestEnv.HookDll);
        var temp = TestEnv.NewDir("print");
        var psi = new ProcessStartInfo(TestEnv.Dotnet)
        {
            UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
        };
        psi.ArgumentList.Add(TestEnv.HookDll);
        foreach (var a in args) psi.ArgumentList.Add(a);
        foreach (var name in new[] { "TMPDIR", "TMP", "TEMP" }) psi.Environment[name] = temp;
        psi.Environment["AIPET_PIPE"] = TestEnv.NoPet();
        var data = TestEnv.NewDir("print-data");
        psi.Environment["AIPET_DATA_DIR"] = data;
        using var p = Process.Start(psi);
        var stderr = p.StandardError.ReadToEndAsync();
        var stdout = new MemoryStream();
        var copied = p.StandardOutput.BaseStream.CopyToAsync(stdout);
        if (!p.WaitForExit(20000))
        {
            try { p.Kill(true); } catch { }
            Assert.Fail("aipet-hook didn't exit within 20 s");
        }
        copied.Wait();
        p.StandardInput.Close();
        return new Printed(p.ExitCode, stdout.ToArray(), stderr.Result, TestEnv.Contents(temp).Concat(TestEnv.Contents(data)).ToArray());
    }

    /// The file as git has it: a Windows checkout may have turned its LFs into CRLFs (* text=auto), git keeps LF.
    static byte[] Committed(string file) =>
        File.ReadAllBytes(Path.Combine(Scripts.Repo, "plugins", "aipet", "hooks", file)).Where(b => b != '\r').ToArray();

    [Theory]
    [InlineData("claude", "hooks.json")]
    [InlineData("codex", "codex.json")]
    [InlineData("CODEX", "codex.json")]
    public void PrintPluginHooks_IsThePluginsHookFile(string agent, string file)
    {
        var r = Print("--print-plugin-hooks", agent);
        Assert.Equal("", r.Stderr);
        Assert.Equal(0, r.Exit);
        // byte for byte, so a difference in the line endings, the indentation or the last newline counts too
        Assert.Equal(Committed(file), r.Stdout);
        Assert.Empty(r.Written);
    }

    /// Without an agent (or with one it doesn't know) it's a usage error, never a hook run waiting on stdin.
    [Theory]
    [InlineData]
    [InlineData("gemini")]
    public void PrintPluginHooks_WithoutAnAgent_PrintsUsage(params string[] rest)
    {
        var r = Print(rest.Prepend("--print-plugin-hooks").ToArray());
        Assert.Equal(2, r.Exit);
        Assert.Empty(r.Stdout);
        Assert.Contains("usage: aipet-hook --print-plugin-hooks claude|codex", r.Stderr);
        Assert.Empty(r.Written);
    }

    /// check-plugin.sh --hook passes the plugin as it is, and shows the diff when a hook file differs from what the
    /// hook prints, but not for CRLF line endings, which only a Windows checkout gives it.
    [UnixFact]
    public void CheckPlugin_ComparesTheHookFilesWithTheHook()
    {
        var box = new Scripts.Sandbox();
        var hook = new[] { "--hook", TestEnv.Dotnet, TestEnv.HookDll };
        var script = Path.Combine(Scripts.Repo, "scripts", "check-plugin.sh");
        var plugin = Path.Combine(Scripts.Repo, "plugins", "aipet");
        var ok = box.Run(Path.Combine(Scripts.Tools, "bash"), new[] { script, plugin }.Concat(hook));
        Assert.True(ok.Exit == 0, ok.Output);
        Assert.DoesNotContain("JSON only", ok.Stdout);

        // without --hook its OK says the hook files weren't compared
        var json = box.Run(Path.Combine(Scripts.Tools, "bash"), new[] { script, plugin });
        Assert.True(json.Exit == 0, json.Output);
        Assert.Contains("hook files checked as JSON only", json.Stdout);

        // --hook takes the rest of the arguments, so an option of the script's after it is an error, not lost
        var late = box.Run(Path.Combine(Scripts.Tools, "bash"), new[] { script }.Concat(hook).Append("--release").Append(plugin));
        Assert.Equal(2, late.Exit);
        Assert.Contains("put --release before it", late.Stderr);

        var copy = Path.Combine(box.Root, "plugin");
        var cp = box.Run(Path.Combine(Scripts.Tools, "cp"), new[] { "-R", Path.Combine(Scripts.Repo, "plugins", "aipet"), copy });
        Assert.True(cp.Exit == 0, cp.Output);
        var claude = Path.Combine(copy, "hooks", "hooks.json");
        var codex = Path.Combine(copy, "hooks", "codex.json");
        File.WriteAllText(claude, File.ReadAllText(claude).Replace("\r\n", "\n").Replace("\n", "\r\n"));
        File.WriteAllText(codex, File.ReadAllText(codex).Replace("\"timeout\": 30", "\"timeout\": 31"));
        var bad = box.Run(Path.Combine(Scripts.Tools, "bash"), new[] { script, copy }.Concat(hook));
        Assert.Equal(1, bad.Exit);
        Assert.DoesNotContain("hooks.json", bad.Output);
        Assert.Contains($"{codex} isn't what aipet-hook --print-plugin-hooks codex prints", bad.Output);
        Assert.Contains("-            \"timeout\": 31,", bad.Stderr);
        Assert.Contains("+            \"timeout\": 30,", bad.Stderr);
    }
}

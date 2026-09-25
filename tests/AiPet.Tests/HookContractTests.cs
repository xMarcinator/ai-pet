using System.Diagnostics;
using System.IO;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;
using Xunit;

namespace AiPet.Tests;

/// The hook's contract against the real pet: whatever comes on stdin it exits 0, prints nothing, and writes nothing
/// unless the pet was reached.
[Collection(PetCollection.Name)]
public class HookContractTests
{
    readonly AgentSessions sessions = new();

    /// JSON.stringify (Node, so Claude) writes a lone surrogate as an escape: text cut in the middle of an emoji.
    /// System.Text.Json parses it but can't read it back, which used to lose the whole event.
    [Fact]
    public void LoneSurrogates_StillReachThePet()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var temp = TestEnv.NewDir("tmp");
            string claude = Guid.NewGuid().ToString(), codex = Guid.NewGuid().ToString();
            var big = new string('x', Ipc.MaxString);
            var runs = new[]
            {
                ("claude", claude, $"{{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"{claude}\",\"prompt\":\"Half \\ud83d an emoji\"," +
                                   $"\"k\\udc00ey\":\"\\udfff\",\"list\":[\"\\ud800\",{{\"big\":\"{big}\\ud800\"}}]}}", "thinking"),
                ("codex", codex, $"{{\"hook_event_name\":\"PreToolUse\",\"session_id\":\"{codex}\",\"turn_id\":\"{Guid.CreateVersion7()}\"," +
                                 $"\"tool_name\":\"Bash\",\"tool_use_id\":\"call_1\",\"tool_input\":{{\"command\":\"echo \\ud800\"}}}}", "working"),
            };
            foreach (var (agent, sid, stdin, outcome) in runs)
            {
                var r = TestEnv.Finish(TestEnv.StartHook(agent, stdin, tempDir: temp));
                Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
                Assert.Equal(new[] { outcome }, TestEnv.Outcomes(sid));
            }
            // nothing went wrong, so nothing was traced
            Assert.Empty(TestEnv.Contents(temp));
        }
        finally { server.Stop(); }
    }

    /// An agent that never closes the hook's stdin: the hook gives up within Ipc.StdinMs and sends nothing (the pet
    /// would ignore an empty event), well within Claude's 5 s.
    [Fact]
    public void StdinThatStaysOpen_SendsNothing_AndWritesNothing()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var temp = TestEnv.NewDir("tmp");
            var sw = Stopwatch.StartNew();
            using var p = HookRun.Start("claude", TestEnv.Pipe, temp);
            try
            {
                Assert.True(p.WaitForExit(Ipc.HookBudgetMs + 3000), "the hook waited for stdin past its budget");
                long took = sw.ElapsedMilliseconds;
                Assert.Equal((0, "", ""), (p.ExitCode, p.StandardOutput.ReadToEnd(), p.StandardError.ReadToEnd()));
                Assert.True(took < Ipc.StdinMs + 2000, $"took {took} ms");
                // the pet logs every event it gets, ignored ones too
                var log = File.Exists(Paths.HookEventsLog) ? File.ReadAllLines(Paths.HookEventsLog) : Array.Empty<string>();
                Assert.DoesNotContain(log, l => l.Contains($" pid={p.Id} "));
                Assert.Empty(TestEnv.Contents(temp));
            }
            finally { try { p.StandardInput.Close(); } catch { } }
        }
        finally { server.Stop(); }
    }

    /// Unix: the temp folder is shared (/tmp), so the trace log is the user's own there, and only they can read it.
    [UnixFact]
    public void TraceLog_OnUnix_IsPerUser_AndPrivate()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var temp = TestEnv.NewDir("tmp");
            var r = TestEnv.Finish(TestEnv.StartHook("claude", "{not json", tempDir: temp));
            Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            var log = Path.Combine(temp, $"aipet-hook-{HookRun.Euid()}.log");
            Assert.Equal(new[] { log }, TestEnv.Contents(temp));
            Assert.Contains("isn't JSON", File.ReadAllText(log));
            Assert.Equal(UnixFileMode.UserRead | UnixFileMode.UserWrite, File.GetUnixFileMode(log));
        }
        finally { server.Stop(); }
    }

    /// Unix: another user can put a file others can read, or a link, at the log's name first. The hook writes to
    /// neither (the test owns both, which is as close as one user gets).
    [UnixFact]
    public void TraceLog_OnUnix_LeavesAFileItDidntMakeAlone()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            const UnixFileMode shared = UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.GroupRead | UnixFileMode.GroupWrite
                                        | UnixFileMode.OtherRead | UnixFileMode.OtherWrite;
            var open = TestEnv.NewDir("tmp");
            var log = Path.Combine(open, $"aipet-hook-{HookRun.Euid()}.log");
            File.WriteAllText(log, "");
            File.SetUnixFileMode(log, shared);

            var linked = TestEnv.NewDir("tmp");
            var target = Path.Combine(linked, "keys");
            File.WriteAllText(target, "");
            File.SetUnixFileMode(target, UnixFileMode.UserRead | UnixFileMode.UserWrite);
            File.CreateSymbolicLink(Path.Combine(linked, $"aipet-hook-{HookRun.Euid()}.log"), target);

            foreach (var temp in new[] { open, linked })
            {
                var r = TestEnv.Finish(TestEnv.StartHook("claude", "{not json", tempDir: temp));
                Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            }
            Assert.Equal(("", shared), (File.ReadAllText(log), File.GetUnixFileMode(log)));
            Assert.Equal("", File.ReadAllText(target));
        }
        finally { server.Stop(); }
    }
}

/// The hook against sockets that aren't a pet (Unix: a plain socket the test controls).
public class HookSocketTests
{
    /// A pet that has stopped taking connections (suspended, with its queue full): connect() on Unix has no time
    /// limit, so the hook gives up on its own within its budget, and traces it (the socket is there).
    [UnixFact]
    public void SocketThatNeverAccepts_EndsTheHookWithinItsBudget()
    {
        var pipe = TestEnv.NoPet();
        using var listener = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        using var queued = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            listener.Bind(new UnixDomainSocketEndPoint(pipe));
            // a backlog of 0 queues one connection; this one fills it, and nothing ever accepts
            listener.Listen(0);
            queued.Connect(new UnixDomainSocketEndPoint(pipe));
            using (var probe = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified) { Blocking = false })
                Assert.Throws<SocketException>(() => probe.Connect(new UnixDomainSocketEndPoint(pipe)));

            var temp = TestEnv.NewDir("tmp");
            var r = TestEnv.Finish(TestEnv.StartHook("claude", Events.Payload(Guid.NewGuid().ToString(), "Stop").ToJsonString(),
                                                     pipe: pipe, tempDir: temp), 15000);
            Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            // Claude's limit, with room for starting dotnet on a busy machine
            Assert.InRange(r.Ended - r.Started, 0, 5.0);
            Assert.Contains("stayed busy", File.ReadAllText(Path.Combine(temp, $"aipet-hook-{HookRun.Euid()}.log")));
        }
        finally { try { File.Delete(pipe); } catch { } }
    }

    /// What the hook sends for escaped lone surrogates: each one as U+FFFD, and everything around it as it was.
    [UnixFact]
    public async Task LoneSurrogates_AreMended_AndTheRestKept()
    {
        var pipe = TestEnv.NoPet();
        using var listener = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            listener.Bind(new UnixDomainSocketEndPoint(pipe));
            listener.Listen(4);
            var got = Task.Run(() =>
            {
                using var s = listener.Accept();
                using var ns = new NetworkStream(s);
                var line = Ipc.ReadLine(ns, Ipc.MaxRequest);
                ns.Write("{\"ok\":true,\"outcome\":\"thinking\"}\n"u8);
                return line;
            });
            var big = new string('y', Ipc.MaxString - 1);
            var stdin = "{\"hook_event_name\":\"UserPromptSubmit\",\"session_id\":\"s\"," +
                        "\"a\":\"x\\ud800y\",\"b\":\"\\uDC00\",\"c\":\"\\ud83d\\ude00\",\"d\":\"\\\\ud800\",\"e\":\"\\ud800\\ud83d\\ude00\"," +
                        "\"f\":\"\\ud83d\\ud83d\",\"g\":\"end\\ud83d\",\"k\\udfff\":1,\"h\":\"q\\\"\\n\\u0041\"," +
                        $"\"big\":\"{big}\\ud800tail\"}}";
            var temp = TestEnv.NewDir("tmp");
            var r = TestEnv.Finish(TestEnv.StartHook("claude", stdin, pipe: pipe, tempDir: temp));
            Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            var envelope = JsonNode.Parse(await got.WaitAsync(TimeSpan.FromSeconds(5))).AsObject();
            var p = envelope[Ipc.Payload].AsObject();
            string S(string key) => p[key].GetValue<string>();
            Assert.Equal(("x\uFFFDy", "\uFFFD", "\U0001F600", "\\ud800", "\uFFFD\U0001F600", "\uFFFD\uFFFD", "end\uFFFD", "q\"\nA"),
                         (S("a"), S("b"), S("c"), S("d"), S("e"), S("f"), S("g"), S("h")));
            Assert.Equal(1, p["k\uFFFD"].GetValue<int>());
            Assert.Equal(big + "\uFFFD", S("big"));
            // sent is when it was sent, after it started
            double at = envelope[Ipc.At].GetValue<double>(), sent = envelope[Ipc.Sent].GetValue<double>();
            Assert.InRange(at, r.Started - 0.01, sent);
            Assert.InRange(sent, at, r.Ended + 0.01);
            Assert.Empty(TestEnv.Contents(temp));
        }
        finally { try { File.Delete(pipe); } catch { } }
    }
}

/// The request line against the pet's size limit, with sent added after the hook measured it.
public class HookRequestSizeTests
{
    /// A tool call whose line is just under Ipc.MaxRequest before sent is added: it has to be cut down to what the pet
    /// reads, or sent takes it over the limit and the pet drops the whole event.
    [UnixFact]
    public async Task ALineJustUnderTheLimit_StaysWithinIt_OnceSentIsAdded()
    {
        var big = new string('x', Ipc.MaxString);
        string Stdin(int pad)
        {
            var input = new JsonObject { ["file_path"] = "/f", ["pad"] = new string('p', pad) };
            for (int i = 0; i < 15; i++) input["s" + i] = big;
            return Events.Payload("s", "PreToolUse", extra: new JsonObject { ["tool_name"] = "MultiEdit", ["tool_input"] = input }).ToJsonString();
        }
        // the first line says how long the rest of it is; the second is 10 bytes under the limit without sent
        var first = await Send(Stdin(0));
        int rest = first.LastIndexOf(",\"" + Ipc.Sent + "\":", StringComparison.Ordinal) + 1;
        Assert.True(rest > 1 && Encoding.UTF8.GetByteCount(first) == first.Length, first.Length.ToString());
        int pad = Ipc.MaxRequest - 10 - rest;
        Assert.InRange(pad, 1, Ipc.MaxString);
        var line = await Send(Stdin(pad));
        Assert.InRange(Encoding.UTF8.GetByteCount(line), 1, Ipc.MaxRequest);
        var input = JsonNode.Parse(line)[Ipc.Payload]["tool_input"].AsObject();
        Assert.Equal(new[] { "file_path" }, input.Select(kv => kv.Key));
        Assert.NotNull(JsonNode.Parse(line)[Ipc.Sent]);
    }

    /// The line a hook sends to a fake pet that takes at most Ipc.MaxRequest bytes, like the real one.
    static async Task<string> Send(string stdin)
    {
        var pipe = TestEnv.NoPet();
        using var listener = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            listener.Bind(new UnixDomainSocketEndPoint(pipe));
            listener.Listen(4);
            var got = Task.Run(() =>
            {
                using var s = listener.Accept();
                using var ns = new NetworkStream(s);
                var line = Ipc.ReadLine(ns, Ipc.MaxRequest);
                ns.Write("{\"ok\":true,\"outcome\":\"working\"}\n"u8);
                return line;
            });
            var r = TestEnv.Finish(TestEnv.StartHook("claude", stdin, pipe: pipe, tempDir: TestEnv.NewDir("tmp")));
            Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            return await got.WaitAsync(TimeSpan.FromSeconds(5));
        }
        finally { try { File.Delete(pipe); } catch { } }
    }
}

/// Runs the hook the way TestEnv does, but with stdin left open.
static class HookRun
{
    public static Process Start(string agent, string pipe, string temp)
    {
        var psi = new ProcessStartInfo(TestEnv.Dotnet)
        {
            UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
        };
        psi.ArgumentList.Add(TestEnv.HookDll);
        psi.ArgumentList.Add("--agent");
        psi.ArgumentList.Add(agent);
        foreach (var name in Ipc.ClaudeEnv) psi.Environment.Remove(name);
        foreach (var (name, value) in new[] { ("AIPET_PIPE", pipe), ("AIPET_DATA_DIR", TestEnv.DataDir), ("TMPDIR", temp), ("TMP", temp),
                                              ("TEMP", temp), ("CODEX_HOME", TestEnv.CodexHome) })
            psi.Environment[name] = value;
        return Process.Start(psi);
    }

    /// The test's effective uid (Linux), which names its trace log.
    public static string Euid() =>
        File.ReadLines("/proc/self/status").First(l => l.StartsWith("Uid:")).Split((char[])null, StringSplitOptions.RemoveEmptyEntries)[2];
}

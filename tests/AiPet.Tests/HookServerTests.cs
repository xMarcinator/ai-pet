using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json.Nodes;
using Xunit;

namespace AiPet.Tests;

/// The real aipet-hook against a real HookServer on the test endpoint.
[Collection(PetCollection.Name)]
public class EndToEndTests
{
    readonly AgentSessions sessions = new();

    AgentSessions.Entry Chat(string key) => sessions.Snapshot().SingleOrDefault(e => e.Id == key);

    /// Codex's hooks finish in any order (PowerShell starts them late at random): run them in a scrambled order and
    /// the pet still ends up at the latest turn, going by Codex's ids rather than by when each hook ran.
    [Fact]
    public void Codex_ScrambledHooks_EndAtTheLatestTurn()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var sid = Guid.NewGuid().ToString();
            // Board shows a Codex chat only with a transcript; its first line names the client
            var transcript = Path.Combine(TestEnv.CodexHome, "sessions", $"rollout-2026-09-25T10-00-00-{sid}.jsonl");
            Directory.CreateDirectory(Path.GetDirectoryName(transcript));
            File.WriteAllText(transcript, $"{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{sid}\",\"originator\":\"codex-tui\"}}}}\n");
            var start = DateTimeOffset.UtcNow.AddSeconds(-20);
            string turn1 = Guid.CreateVersion7(start).ToString(), turn2 = Guid.CreateVersion7(start.AddSeconds(5)).ToString();
            JsonObject Ev(string ev, string turn, JsonObject extra = null)
            {
                var p = Events.Payload(sid, ev, turn, extra);
                p["transcript_path"] = transcript;
                p["cwd"] = "/work/project";
                return p;
            }
            const string patch = "*** Begin Patch\n*** Update File: src/Main.cs\n@@\n-a\n+b\n*** End Patch\n";
            var steps = new (JsonObject Payload, string Outcome)[]
            {
                (Ev("Stop", turn1), "done"),
                (Ev("PreToolUse", turn1, Events.Bash("ls", "call_1")), "stale"),
                (Ev("UserPromptSubmit", turn2, new JsonObject { ["prompt"] = "second turn\nmore" }), "thinking"),
                (Ev("PostToolUse", turn1, Events.Bash("ls", "call_1")), "stale"),
                (Ev("PreToolUse", turn2, new JsonObject { ["tool_name"] = "apply_patch", ["tool_use_id"] = "call_2", ["tool_input"] = new JsonObject { ["command"] = patch } }), "working"),
                (Ev("Stop", turn1), "stale"),
            };
            foreach (var (payload, _) in steps)
            {
                var r = TestEnv.RunHook("codex", payload);
                Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            }

            var expected = steps.Select(s => s.Outcome).ToList();
            Assert.Equal(expected, TestEnv.Outcomes(sid));
            var ping = TestEnv.Ask("{\"v\":1,\"type\":\"ping\"}");
            Assert.True(ping[Ipc.Ok].GetValue<bool>());
            Assert.Equal(Environment.ProcessId, ping[Ipc.Pid].GetValue<int>());
            Assert.Equal(expected, TestEnv.Outcomes(ping[Ipc.Recent].AsArray().Select(n => n.GetValue<string>()), sid));
            // where the chat runs comes from the transcript's first line
            Assert.All(File.ReadAllLines(Paths.HookEventsLog).Where(l => l.Contains(" " + sid[..13] + " ")), l => Assert.Contains(" where=terminal ", l));

            var chat = Chat("codex:" + sid);
            Assert.Equal(("working", "Editing Main.cs", "Second turn"), (chat.State, chat.Detail, chat.Title));
            Assert.Equal(("terminal", "/work/project", true), (chat.Where, chat.Cwd, chat.HasTranscript));
        }
        finally { server.Stop(); }
    }

    /// Claude's at is when its hook started, which the real binary can't be made to scramble reliably; envelopes
    /// sent over the real socket can.
    [Fact]
    public void Claude_LateEnvelopes_OverTheSocket_AreStale()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var sid = Guid.NewGuid().ToString();
            double t0 = Board.Unix;
            string Send(string ev, double at, JsonObject extra = null)
            {
                var reply = TestEnv.Ask(Events.Envelope("claude", at, Events.Payload(sid, ev, extra: extra)));
                Assert.True(reply[Ipc.Ok].GetValue<bool>());
                return reply[Ipc.Outcome].GetValue<string>();
            }
            Assert.Equal("thinking", Send("UserPromptSubmit", t0, new JsonObject { ["prompt"] = "fix the bug" }));
            Assert.Equal("done", Send("Stop", t0 + 2));
            Assert.Equal("stale", Send("PreToolUse", t0 + 1, Events.Bash("ls")));
            Assert.Equal("stale", Send("PostToolUse", t0 + 2, Events.Bash("ls")));
            Assert.Equal("done", Chat("claude:" + sid).State);
            Assert.Equal("removed", Send("SessionEnd", t0 + 3));
            Assert.Equal("stale", Send("Notification", t0 + 2.5, new JsonObject { ["notification_type"] = "permission_prompt" }));
            Assert.Equal(t0 + 3, Chat("claude:" + sid).Ended);
            Assert.Equal(new[] { "thinking", "done", "stale", "stale", "removed", "stale" }, TestEnv.Outcomes(sid));
        }
        finally { server.Stop(); }
    }

    /// What the Claude hook passes on: the event, when it started, and the environment that says where the chat
    /// runs. It prints nothing, exits 0, and traces nothing when all went well.
    [Fact]
    public void ClaudeHook_ForwardsTheEventAndItsEnvironment_Silently()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var sid = Guid.NewGuid().ToString();
            var host = "local_" + Guid.NewGuid();
            var temp = TestEnv.NewDir("tmp");
            var payload = Events.Payload(sid, "UserPromptSubmit", extra: new JsonObject
            {
                ["prompt"] = "hello there", ["cwd"] = "/work/claude", ["transcript_path"] = Path.Combine(temp, "missing.jsonl"),
            });
            var env = new Dictionary<string, string> { ["CLAUDE_CODE_ENTRYPOINT"] = "claude-desktop", ["CLAUDE_CODE_HOST_SESSION_ID"] = host };
            var r = TestEnv.RunHook("claude", payload, temp, env);
            Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            Assert.Empty(TestEnv.Contents(temp));

            var chat = Chat("claude:" + sid);
            Assert.Equal(("thinking", "Hello there", "/work/claude"), (chat.State, chat.Title, chat.Cwd));
            Assert.Equal(("desktop", host), (chat.Where, chat.HostId));
            // at is the hook's start: after it was launched, before it exited
            Assert.InRange(chat.Ts, r.Started - 0.01, r.Ended + 0.01);
            Assert.Equal(new[] { "thinking" }, TestEnv.Outcomes(sid));

            // without the variables the chat's place is unknown, not taken from anywhere else
            var sid2 = Guid.NewGuid().ToString();
            r = TestEnv.RunHook("claude", Events.Payload(sid2, "SessionStart"), temp);
            Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            Assert.Equal(((string, string, string))("idle", null, null), (Chat("claude:" + sid2).State, Chat("claude:" + sid2).Where, Chat("claude:" + sid2).HostId));
        }
        finally { server.Stop(); }
    }

    /// Stdin that isn't JSON still reaches the pet (as an empty event), and the hook still exits 0 silently. With the
    /// pet running it may trace why.
    [Fact]
    public void Hook_WithGarbageOnStdin_ExitsZeroSilently()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var temp = TestEnv.NewDir("tmp");
            foreach (var agent in new[] { "claude", "codex" })
            {
                var r = TestEnv.Finish(TestEnv.StartHook(agent, "{not json", tempDir: temp));
                Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
            }
            // aipet-hook.log on Windows, aipet-hook-<euid>.log elsewhere
            Assert.Contains("isn't JSON", File.ReadAllText(Directory.GetFiles(temp, "aipet-hook*.log").Single()));
        }
        finally { server.Stop(); }
    }
}

/// The pet's socket against clients that misbehave, and Stop.
[Collection(PetCollection.Name)]
public class HookServerTests
{
    readonly AgentSessions sessions = new();

    /// An event for a new chat; its reply, which must be an accepted one.
    string Serve(string sid)
    {
        var reply = TestEnv.Ask(Events.Envelope("claude", Board.Unix, Events.Payload(sid, "UserPromptSubmit")));
        Assert.NotNull(reply);
        Assert.True(reply[Ipc.Ok].GetValue<bool>(), reply.ToJsonString());
        Assert.Contains(sessions.Snapshot(), e => e.Id == "claude:" + sid);
        return reply[Ipc.Outcome].GetValue<string>();
    }

    /// Waits for the pet to close a connection; how long after `since` it did (ms), or -1 if it didn't within 12 s.
    /// Reads return 0 once the pet closes (Unix) or disconnects (Windows); a broken pipe counts as closed too.
    static Task<long> Closed(NamedPipeClientStream c, Stopwatch since) => Task.Run(async () =>
    {
        using var cts = new CancellationTokenSource(Ipc.TimeoutMs + 10000);
        try
        {
            var buf = new byte[256];
            while (await c.ReadAsync(buf, cts.Token) > 0) { }
        }
        catch (IOException) { }
        catch (OperationCanceledException) { return -1L; }
        return since.ElapsedMilliseconds;
    });

    [Fact]
    public async Task SilentClients_AreCutOff_WhileThePetKeepsServing()
    {
        var server = TestEnv.StartServer(sessions);
        var clients = new List<NamedPipeClientStream>();
        try
        {
            var since = Stopwatch.StartNew();
            // more than the pet's listeners: each connection gets a thread of its own, and its listener makes the next
            // instance at once
            for (int i = 0; i < 12; i++) clients.Add(TestEnv.ConnectSilent());
            var closed = clients.Select(c => Closed(c, since)).ToArray();

            Assert.Equal("thinking", Serve(Guid.NewGuid().ToString()));
            long served = since.ElapsedMilliseconds;
            // a real hook too (it may take long to start on a slow machine, so it isn't timed)
            var sid = Guid.NewGuid().ToString();
            var hook = TestEnv.RunHook("codex", Events.Payload(sid, "SessionStart"));
            Assert.Equal((0, "", ""), (hook.Exit, hook.Stdout, hook.Stderr));
            Assert.Equal(new[] { "idle" }, TestEnv.Outcomes(sid));

            var times = await Task.WhenAll(closed);
            Assert.DoesNotContain(-1L, times);
            Assert.All(times, t => Assert.InRange(t, Ipc.TimeoutMs - 50, Ipc.TimeoutMs + 5000));
            // the others were answered while the silent ones were still waiting to be cut off
            Assert.True(served < times.Min(), $"served after {served} ms, the first cut-off came at {times.Min()} ms");
            Assert.Equal("thinking", Serve(Guid.NewGuid().ToString()));
        }
        finally
        {
            foreach (var c in clients) c.Dispose();
            server.Stop();
        }
    }

    [Fact]
    public async Task Stop_ReturnsPromptly_WithClientsConnected()
    {
        var server = TestEnv.StartServer(sessions);
        var clients = new List<NamedPipeClientStream>();
        try
        {
            for (int i = 0; i < 4; i++) clients.Add(TestEnv.ConnectSilent());
            // and one halfway through a request
            var half = TestEnv.ConnectSilent();
            clients.Add(half);
            half.Write(Encoding.UTF8.GetBytes("{\"v\":1,\"type\":\"eve"));
            half.Flush();
            var since = Stopwatch.StartNew();
            var closed = clients.Select(c => Closed(c, since)).ToArray();

            server.Stop();
            Assert.True(since.ElapsedMilliseconds < 1500, $"Stop took {since.ElapsedMilliseconds} ms");
            // nothing listens any more
            var gone = Stopwatch.StartNew();
            while (true)
            {
                using var c = Ipc.Connect();
                if (c == null) break;
                Assert.True(gone.ElapsedMilliseconds < 10000, "the pet still listens after Stop");
                Thread.Sleep(100);
            }
            // and the clients it was answering are still cut off
            Assert.DoesNotContain(-1L, await Task.WhenAll(closed));
        }
        finally
        {
            foreach (var c in clients) c.Dispose();
            server.Stop();
        }
    }

    [Theory]
    [InlineData("not json")]
    [InlineData("[1,2,3]")]
    [InlineData("\"a string\"")]
    [InlineData("{\"v\":1,\"type\":\"event\",")]
    [InlineData("")]
    public void Garbage_IsDroppedWithoutAReply(string line)
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            Assert.Null(TestEnv.Ask(line));
            Assert.Equal("thinking", Serve(Guid.NewGuid().ToString()));
        }
        finally { server.Stop(); }
    }

    [Fact]
    public void TooDeep_IsDroppedWithoutAReply()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            int depth = Ipc.MaxDepth + 10;
            Assert.Null(TestEnv.Ask(new string('[', depth) + new string(']', depth)));
            Assert.Equal("thinking", Serve(Guid.NewGuid().ToString()));
        }
        finally { server.Stop(); }
    }

    [Fact]
    public void JsonItCantServe_IsRefused()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var reply = TestEnv.Ask("{\"v\":2,\"type\":\"ping\"}");
            Assert.Equal((false, "unsupported version"), (reply[Ipc.Ok].GetValue<bool>(), reply[Ipc.Error].GetValue<string>()));
            reply = TestEnv.Ask("{\"v\":1,\"type\":\"dance\"}");
            Assert.Equal((false, "unknown request"), (reply[Ipc.Ok].GetValue<bool>(), reply[Ipc.Error].GetValue<string>()));
            Assert.Equal("thinking", Serve(Guid.NewGuid().ToString()));
        }
        finally { server.Stop(); }
    }

    [Fact]
    public void OversizedRequest_IsDroppedWithoutAReply()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            var sid = Guid.NewGuid().ToString();
            var line = Events.Envelope("claude", Board.Unix, Events.Payload(sid, "UserPromptSubmit",
                extra: new JsonObject { ["prompt"] = new string('x', Ipc.MaxRequest) })).ToJsonString();
            Assert.True(Encoding.UTF8.GetByteCount(line) > Ipc.MaxRequest);
            var since = Stopwatch.StartNew();
            string reply = null;
            using (var pipe = Ipc.Connect())
            {
                Assert.NotNull(pipe);
                try { reply = Ipc.Ask(pipe, line); }
                catch (IOException) { }  // the pet closed while it was still being written
                catch (System.Net.Sockets.SocketException) { }
            }
            Assert.Null(reply);
            // dropped as soon as it was too big, not left to the silent-client cut-off
            Assert.True(since.ElapsedMilliseconds < Ipc.TimeoutMs, $"dropped after {since.ElapsedMilliseconds} ms");
            Assert.DoesNotContain(sessions.Snapshot(), e => e.Id == "claude:" + sid);
            Assert.Equal("thinking", Serve(Guid.NewGuid().ToString()));
        }
        finally { server.Stop(); }
    }
}

/// The hook with no pet running: it exits 0, prints nothing, and writes nothing anywhere.
public class NoPetTests
{
    static void AssertLeavesNoTrace(string agent, string stdin, string pipe)
    {
        var data = TestEnv.NewDir("data");
        var temp = TestEnv.NewDir("tmp");
        var r = TestEnv.Finish(TestEnv.StartHook(agent, stdin, pipe, data, temp));
        Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr));
        Assert.Empty(TestEnv.Contents(data));
        Assert.Empty(TestEnv.Contents(temp));
    }

    [Theory]
    [InlineData("claude")]
    [InlineData("codex")]
    public void Hook_WithNoPet_LeavesNoTrace(string agent)
    {
        var pipe = TestEnv.NoPet();
        AssertLeavesNoTrace(agent, Events.Payload(Guid.NewGuid().ToString(), "UserPromptSubmit", Guid.CreateVersion7().ToString()).ToJsonString(), pipe);
        AssertLeavesNoTrace(agent, "{not json", pipe);
        AssertLeavesNoTrace(agent, "", pipe);
        if (!OperatingSystem.IsWindows()) Assert.False(File.Exists(pipe));
    }

    /// Linux: a pet that crashed leaves its socket file behind, which refuses connections. Made with libc, since
    /// .NET deletes the file when its socket closes.
    [LinuxFact("a Windows pipe goes away with its last instance")]
    public void Hook_WithAStaleSocketFile_LeavesNoTrace()
    {
        var pipe = TestEnv.NoPet();
        var path = Encoding.UTF8.GetBytes(pipe);
        var addr = new byte[2 + 108];
        addr[0] = 1;  // AF_UNIX, then sun_path
        path.CopyTo(addr, 2);
        int fd = socket(1, 1, 0);
        Assert.True(fd >= 0);
        try { Assert.Equal(0, bind(fd, addr, 2 + path.Length + 1)); Assert.Equal(0, listen(fd, 1)); }
        finally { close(fd); }
        try
        {
            Assert.True(File.Exists(pipe));
            AssertLeavesNoTrace("claude", Events.Payload(Guid.NewGuid().ToString(), "Stop").ToJsonString(), pipe);
            AssertLeavesNoTrace("codex", Events.Payload(Guid.NewGuid().ToString(), "Stop").ToJsonString(), pipe);
        }
        finally { File.Delete(pipe); }
    }

    [DllImport("libc", SetLastError = true)] static extern int socket(int domain, int type, int protocol);
    [DllImport("libc", SetLastError = true)] static extern int bind(int fd, byte[] addr, int len);
    [DllImport("libc", SetLastError = true)] static extern int listen(int fd, int backlog);
    [DllImport("libc")] static extern int close(int fd);
}

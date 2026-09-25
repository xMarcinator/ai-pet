using System.Diagnostics;
using System.IO;
using System.Net.Sockets;
using Xunit;
using Xunit.Abstractions;

namespace AiPet.Tests;

/// The pet answers hooks while its thread pool is completely busy (its watchers, a burst of hooks reading
/// transcripts): its listeners and handlers run on threads of their own (HookServer). A listener or handler that
/// waited for the pool would leave the hooks unanswered until their time is up, and they'd drop their events.
[Collection(PetCollection.Name)]
public class StarvedPoolTests
{
    const int Hooks = 20;
    readonly ITestOutputHelper output;

    public StarvedPoolTests(ITestOutputHelper output) => this.output = output;

    [Fact]
    public void Listeners_AnswerHooks_WhileThePoolIsStarved()
    {
        var sessions = new AgentSessions();
        var server = TestEnv.StartServer(sessions);
        var temp = TestEnv.NewDir("tmp");
        // the first hook start sets up process handling in this process, before the pool is taken away
        TestEnv.Finish(TestEnv.StartHook("claude", "{}", tempDir: TestEnv.NewDir("tmp")));
        try
        {
            using (var pool = new StarvedPool())
            {
                output.WriteLine($"pool: {pool.Workers} workers, {pool.Started} blockers running, {ThreadPool.PendingWorkItemCount} work items waiting");
                var sids = Enumerable.Range(0, Hooks).Select(i => $"{i:x2}starved-{Guid.NewGuid()}").ToList();
                var since = Stopwatch.StartNew();
                // all at once, and waited for without the pool (StartHook and Finish only block)
                var runs = sids.Select(sid => TestEnv.StartHook("claude", Events.Payload(sid, "UserPromptSubmit").ToJsonString(), tempDir: temp)).ToList();
                var results = runs.Select(r => TestEnv.Finish(r, 30000)).ToList();
                long took = since.ElapsedMilliseconds;
                output.WriteLine($"{Hooks} hooks took {took} ms");

                // checked while still starved: once the pool is back, anything left queued would still get through
                Assert.False(pool.ProbeRan, "the pool wasn't starved: a work item ran");
                Assert.All(results, r => Assert.Equal((0, "", ""), (r.Exit, r.Stdout, r.Stderr)));
                // aipet-hook.log on Windows, aipet-hook-<euid>.log elsewhere
                var trace = Directory.GetFiles(temp, "aipet-hook*.log").SingleOrDefault();
                Assert.True(trace == null, "a hook gave up: " + (trace != null ? File.ReadAllText(trace) : ""));
                var chats = sessions.Snapshot().ToDictionary(e => e.Id);
                Assert.All(sids, sid =>
                {
                    Assert.True(chats.TryGetValue("claude:" + sid, out var chat), "the pet never got " + sid);
                    Assert.Equal("thinking", chat.State);
                    Assert.Equal(new[] { "thinking" }, TestEnv.Outcomes(sid));
                });
                // a hook that got no answer waits Ipc.TimeoutMs for it; these were answered at once
                Assert.True(took < 15000, $"the hooks took {took} ms");
                Assert.False(pool.ProbeRan, "the pool wasn't starved: a work item ran");
            }
        }
        finally { server.Stop(); }
    }

    /// Unix: a client that connects and says nothing is still cut off after Ipc.TimeoutMs while the pool is starved,
    /// though the cut-off's Timer can't run then: the socket's own timeout ends the read. And Stop needs no pool.
    /// (Windows has only the Timer; a silent client there waits for the pool.)
    [UnixFact]
    public void SilentClients_AreCutOff_AndStopReturns_WhileThePoolIsStarved()
    {
        var server = TestEnv.StartServer(new AgentSessions());
        try
        {
            using var pool = new StarvedPool();
            using var silent = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified) { ReceiveTimeout = Ipc.TimeoutMs + 8000 };
            var since = Stopwatch.StartNew();
            silent.Connect(new UnixDomainSocketEndPoint(Ipc.Endpoint));
            int n = -1;
            try { n = silent.Receive(new byte[16]); }
            catch (SocketException ex) when (ex.SocketErrorCode == SocketError.TimedOut) { }
            catch (SocketException) { n = 0; }  // reset: closed too
            long took = since.ElapsedMilliseconds;
            output.WriteLine($"cut off after {took} ms");
            Assert.True(n == 0, $"not cut off within {took} ms");
            Assert.InRange(took, Ipc.TimeoutMs - 50, Ipc.TimeoutMs + 2000);

            since.Restart();
            server.Stop();
            Assert.True(since.ElapsedMilliseconds < 1500, $"Stop took {since.ElapsedMilliseconds} ms");
            Assert.False(File.Exists(Ipc.Endpoint), "Stop left the socket file");
            Assert.False(pool.ProbeRan, "the pool wasn't starved: a work item ran");
        }
        finally { server.Stop(); }
    }

    /// Every worker of the pool blocked, and more work queued behind them, until disposed. The cap is as low as the
    /// pool allows (not below its minimum), and put back after.
    sealed class StarvedPool : IDisposable
    {
        readonly ManualResetEvent release = new(false);
        readonly int oldWorkers, oldIo;
        int started;
        volatile bool probeRan;

        public int Workers { get; }
        public int Started => Volatile.Read(ref started);
        /// A work item queued after the blockers: it runs only once a worker is free.
        public bool ProbeRan => probeRan;

        public StarvedPool()
        {
            ThreadPool.GetMaxThreads(out oldWorkers, out oldIo);
            ThreadPool.GetMinThreads(out int minWorkers, out _);
            Workers = Math.Max(Environment.ProcessorCount, minWorkers);
            try
            {
                Assert.True(ThreadPool.SetMaxThreads(Workers, oldIo), $"can't cap the pool at {Workers} workers");
                ThreadPool.GetMaxThreads(out int max, out _);
                Assert.Equal(Workers, max);
                for (int i = 0; i < 2 * Workers + 8; i++)
                    ThreadPool.UnsafeQueueUserWorkItem(_ =>
                    {
                        Interlocked.Increment(ref started);
                        release.WaitOne();
                    }, null);
                // until no more blockers start and some are still waiting: every worker the pool will have is taken
                var since = Stopwatch.StartNew();
                int last = -1;
                long steady = 0;
                while (true)
                {
                    int now = Started;
                    if (now != last) (last, steady) = (now, since.ElapsedMilliseconds);
                    else if (since.ElapsedMilliseconds - steady > 500 && ThreadPool.PendingWorkItemCount > 0) break;
                    Assert.True(since.ElapsedMilliseconds < 20000, "the pool never filled up");
                    Thread.Sleep(20);
                }
                ThreadPool.UnsafeQueueUserWorkItem(_ => probeRan = true, null);
                Thread.Sleep(200);
                Assert.False(probeRan, "the pool isn't starved");
            }
            catch
            {
                Dispose();
                throw;
            }
        }

        public void Dispose()
        {
            release.Set();
            ThreadPool.SetMaxThreads(oldWorkers, oldIo);
        }
    }
}

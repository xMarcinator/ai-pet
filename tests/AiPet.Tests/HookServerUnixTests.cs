using System.Diagnostics;
using System.IO;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using Xunit;

namespace AiPet.Tests;

/// The pet's Unix socket: its file, what a crashed pet left at the path, and a flood of connections. Windows has
/// pipe instances instead, which go away with the pet.
[Collection(PetCollection.Name)]
public class HookServerUnixTests
{
    readonly AgentSessions sessions = new();

    /// An event for a new chat; its reply, which must be an accepted one.
    string Serve()
    {
        var sid = Guid.NewGuid().ToString();
        var reply = TestEnv.Ask(Events.Envelope("claude", Board.Unix, Events.Payload(sid, "UserPromptSubmit")));
        Assert.NotNull(reply);
        Assert.True(reply[Ipc.Ok].GetValue<bool>(), reply.ToJsonString());
        Assert.Contains(sessions.Snapshot(), e => e.Id == "claude:" + sid);
        return reply[Ipc.Outcome].GetValue<string>();
    }

    static Socket ConnectSilent()
    {
        var c = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        c.Connect(new UnixDomainSocketEndPoint(Ipc.Endpoint));
        return c;
    }

    /// The pet closed it: readable with nothing to read (or broken).
    static bool Closed(Socket c)
    {
        try { return c.Poll(0, SelectMode.SelectRead) && c.Available == 0; }
        catch (SocketException) { return true; }
    }

    static int Threads()
    {
        using var me = Process.GetCurrentProcess();
        return me.Threads.Count;
    }

    [UnixFact]
    public void Socket_IsTheUsersOnly_AndGoesWithStop()
    {
        var server = TestEnv.StartServer(sessions);
        try
        {
            Assert.Equal(UnixFileMode.UserRead | UnixFileMode.UserWrite, File.GetUnixFileMode(Ipc.Endpoint));
            var since = Stopwatch.StartNew();
            server.Stop();
            Assert.True(since.ElapsedMilliseconds < 1500, $"Stop took {since.ElapsedMilliseconds} ms");
            Assert.False(File.Exists(Ipc.Endpoint), "Stop left the socket file");
            using var c = Ipc.Connect();
            Assert.Null(c);
            // a second Stop has nothing left to take down
            server.Stop();
        }
        finally { server.Stop(); }
    }

    public enum Leftover { ClosedSocket, SocketNotListening, File }

    /// A crashed pet leaves its socket file, which refuses connections; whatever is at the path and doesn't listen
    /// is replaced. Made with libc, since .NET deletes the file when its socket closes.
    [LinuxTheory("sockaddr_un as Linux has it")]
    [InlineData(Leftover.ClosedSocket)]
    [InlineData(Leftover.SocketNotListening)]
    [InlineData(Leftover.File)]
    public void WhatACrashLeft_IsReplaced(Leftover left)
    {
        File.Delete(Ipc.Endpoint);
        int fd = -1;
        if (left == Leftover.File) File.WriteAllText(Ipc.Endpoint, "");
        else
        {
            fd = Bound(Ipc.Endpoint);
            if (left == Leftover.ClosedSocket)
            {
                Assert.Equal(0, listen(fd, 1));
                close(fd);
                fd = -1;
            }
        }
        var server = new HookServer(sessions);
        try
        {
            Assert.True(File.Exists(Ipc.Endpoint));
            server.Start();
            Assert.Equal("thinking", Serve());
            Assert.Equal(UnixFileMode.UserRead | UnixFileMode.UserWrite, File.GetUnixFileMode(Ipc.Endpoint));
        }
        finally
        {
            server.Stop();
            if (fd >= 0) close(fd);
            File.Delete(Ipc.Endpoint);
        }
    }

    /// A second pet on the same path leaves the live one's socket alone, and its Stop too.
    [UnixFact]
    public void ALivePet_IsNotReplaced()
    {
        var server = TestEnv.StartServer(sessions);
        var other = new HookServer(new AgentSessions());
        try
        {
            other.Start();
            other.Stop();
            Assert.True(File.Exists(Ipc.Endpoint));
            Assert.Equal("thinking", Serve());
        }
        finally
        {
            other.Stop();
            server.Stop();
        }
    }

    /// Some program of this user opens connections by the hundred and says nothing: past HookServer.MaxConnections
    /// they're dropped at once instead of each taking a thread, and once they're gone the pet answers as before.
    [UnixFact]
    public void AFloodOfSilentConnections_IsCapped_AndThePetStillAnswers()
    {
        const int Flood = 400;
        var server = TestEnv.StartServer(sessions);
        var clients = new List<Socket>();
        try
        {
            int before = Threads();
            var since = Stopwatch.StartNew();
            for (int i = 0; i < Flood; i++) clients.Add(ConnectSilent());
            // those past the cap are closed long before the silent-client cut-off
            while (clients.Count(Closed) < Flood - HookServer.MaxConnections)
            {
                Assert.True(since.ElapsedMilliseconds < Ipc.TimeoutMs - 500,
                    $"{clients.Count(Closed)} of {Flood} closed after {since.ElapsedMilliseconds} ms");
                Thread.Sleep(20);
            }
            int grew = Threads() - before;
            Assert.True(grew < HookServer.MaxConnections + 30, $"{grew} more threads for {Flood} connections");

            foreach (var c in clients) c.Dispose();
            clients.Clear();
            // the connections being handled see the end and go
            var gone = Stopwatch.StartNew();
            while (Threads() > before + 8)
            {
                Assert.True(gone.ElapsedMilliseconds < 5000, $"{Threads() - before} more threads than before the flood");
                Thread.Sleep(20);
            }
            Assert.Equal("thinking", Serve());
            var sid = Guid.NewGuid().ToString();
            var hook = TestEnv.RunHook("codex", Events.Payload(sid, "SessionStart"));
            Assert.Equal((0, "", ""), (hook.Exit, hook.Stdout, hook.Stderr));
            Assert.Equal(new[] { "idle" }, TestEnv.Outcomes(sid));
        }
        finally
        {
            foreach (var c in clients) c.Dispose();
            server.Stop();
        }
    }

    /// A socket bound at path, not listening.
    static int Bound(string path)
    {
        var bytes = Encoding.UTF8.GetBytes(path);
        var addr = new byte[2 + 108];
        addr[0] = 1;  // AF_UNIX, then sun_path
        bytes.CopyTo(addr, 2);
        int fd = socket(1, 1, 0);
        Assert.True(fd >= 0);
        if (bind(fd, addr, 2 + bytes.Length + 1) != 0)
        {
            close(fd);
            Assert.Fail("can't bind " + path);
        }
        return fd;
    }

    [DllImport("libc", SetLastError = true)] static extern int socket(int domain, int type, int protocol);
    [DllImport("libc", SetLastError = true)] static extern int bind(int fd, byte[] addr, int len);
    [DllImport("libc", SetLastError = true)] static extern int listen(int fd, int backlog);
    [DllImport("libc")] static extern int close(int fd);
}

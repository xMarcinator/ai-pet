using System.Globalization;
using System.IO;
using System.IO.Pipes;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AiPet;

/// The pet's end of the hooks' local socket (see Ipc). Each hook run connects, sends its event and waits for the
/// outcome: the event goes to AgentSessions, and one line to hook-events.log and to the recent list the doctor pings
/// for. Garbage, oversized or silent connections are dropped.
/// Listening and answering run on threads of their own (Changed too) and only ever block, so nothing waits for the
/// thread pool: while the pool is busy (the pet's watchers, a burst of hooks reading transcripts) a hook would get no
/// answer within Ipc.TimeoutMs and log a failure, or (Windows) find every instance taken and drop its event as if the
/// pet were closed.
///   Windows: synchronous pipe instances, whose WaitForConnection, reads and writes wait in the kernel.
///   Unix: a plain socket that is only ever used synchronously, so .NET keeps it blocking and Accept is accept(2) on
///   the calling thread; the connections it gives block too. (NamedPipeServerStream's WaitForConnection waits for an
///   AcceptAsync that the pool completes.)
/// Only the silent-client cut-off, a Timer, runs on the pool; on Unix the socket's own timeouts stand in for it while
/// the pool is too busy.
public sealed class HookServer
{
    /// Windows: instances waiting for a hook at any time, each with a thread that makes the next one as soon as a hook
    /// takes it. Unix: threads waiting in Accept on the one socket (the kernel queues connects meanwhile).
    const int Listeners = 8;
    /// Connections handled at once: far above a burst of real hooks, each answered in milliseconds. Past it a
    /// connection is dropped at once, so a flood from some program of this user can't take a thread each for
    /// Ipc.TimeoutMs until none can be started.
    public const int MaxConnections = 64;
    const int RecentLines = 30;

    readonly AgentSessions sessions;
    readonly Queue<string> recent = new();
    readonly object logLock = new();
    volatile bool stopping;
    int handling;
    long warned;
    Socket listener;  // Unix
    static PipeSecurity security;

    /// A hook event changed a chat.
    public event Action Changed;

    public HookServer(AgentSessions sessions) => this.sessions = sessions;

    /// Once, after the app's single-instance check: another pet would have the name already.
    public void Start()
    {
        try
        {
            if (OperatingSystem.IsWindows())
            {
                var first = Create(first: true);  // UnauthorizedAccessException when the name is taken
                for (int i = 0; i < Listeners; i++)
                {
                    var s = i == 0 ? first : Create(first: false);
                    Run(() => Listen(s));
                }
            }
            else
            {
                RefuseLiveServer();
                var s = listener = Bind();
                int started = 0;
                try { for (; started < Listeners; started++) Run(() => Accept(s)); }
                catch (Exception ex) when (started > 0) { Log.Write($"hooks: {started} of {Listeners} listeners: {ex.Message}"); }
                catch (Exception)
                {
                    // hooks would wait for answers nobody gives; with no socket they find no pet
                    listener = null;
                    s.Dispose();
                    throw;
                }
            }
            Log.Write("hooks: listening on " + Ipc.Endpoint);
        }
        catch (Exception ex) { Log.Write($"hooks: can't listen on {Ipc.Endpoint}: {ex.Message}"); }
    }

    /// Takes the socket down. Hooks being answered finish.
    /// Windows: a listener's wait for a hook can't be cancelled, so each is woken by a connection of its own, sees the
    /// pet stopping and closes its instance instead of making the next.
    /// Unix: a connection wakes each listener in Accept too (closing the socket does that on Linux, not everywhere),
    /// then the socket is closed, and .NET removes the file it bound.
    public void Stop()
    {
        stopping = true;
        if (!OperatingSystem.IsWindows())
        {
            if (Interlocked.Exchange(ref listener, null) is not { } s) return;  // not listening, or stopped already
            for (int i = 0; i < Listeners; i++)
                try
                {
                    // not blocking: a full queue fails the connect at once rather than hold Stop up
                    using var wake = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified) { Blocking = false };
                    wake.Connect(new UnixDomainSocketEndPoint(Ipc.Endpoint));
                }
                catch (Exception) { break; }
            s.Dispose();
            return;
        }
        for (int i = 0; i < 2 * Listeners; i++)
            try
            {
                using var wake = new NamedPipeClientStream(".", Ipc.Endpoint, PipeDirection.InOut);
                wake.Connect(0);
            }
            catch (Exception) { break; }  // no instance left waiting
    }

    /// One Windows listener: waits for a hook, makes the next instance, hands the connection to a thread of its own,
    /// and waits on the new instance.
    void Listen(NamedPipeServerStream s)
    {
        try
        {
            // stopping is read once the instance exists, so Stop's wake-up either finds the instance or comes before this
            while (s != null && !stopping)
            {
                try { s.WaitForConnection(); }
                catch (Exception) when (!stopping)
                {
                    // a client that left at once. The new instance comes first, so the name never goes away.
                    var fresh = Next();
                    s.Dispose();
                    s = fresh;
                    continue;
                }
                if (stopping) break;
                // likewise the replacement is made before this one is handed off: a hook that found no instance at all
                // would take the pet for not running
                var conn = s;
                s = null;
                try { s = Create(first: false); } catch { }
                Serve(conn);
                s ??= Next();
            }
        }
        catch (Exception ex) when (!stopping) { Log.Write("hooks: a listener stopped: " + ex.Message); }
        catch (Exception) { }  // woken by Stop
        // an exception left on a thread of its own would end the app
        finally { try { s?.Dispose(); } catch (Exception) { } }
    }

    /// A new listening instance. When none can be made just now (every allowed instance in use, say), it tries again
    /// shortly rather than lose a listener; null once the pet is stopping.
    NamedPipeServerStream Next()
    {
        for (int i = 0; !stopping; i++)
        {
            try { return Create(first: false); }
            catch (Exception ex)
            {
                if (i == 0) Log.Write("hooks: " + ex.Message);
                Thread.Sleep(250);
            }
        }
        return null;
    }

    /// One Unix listener: waits for a hook and hands the connection to a thread of its own.
    void Accept(Socket listening)
    {
        try
        {
            while (!stopping)
            {
                Socket c;
                try { c = listening.Accept(); }
                catch (Exception ex) when (!stopping)
                {
                    // out of file descriptors, say: the hooks wait in the kernel's queue meanwhile
                    Warn(ex.Message);
                    Thread.Sleep(100);
                    continue;
                }
                if (stopping || !SameUser(c))
                {
                    c.Dispose();
                    continue;
                }
                NetworkStream conn;
                try
                {
                    // the cut-off without the pool: a read or write that waits this long fails
                    c.ReceiveTimeout = c.SendTimeout = Ipc.TimeoutMs;
                    conn = new NetworkStream(c, ownsSocket: true);
                }
                catch (Exception)
                {
                    c.Dispose();  // the hook left already
                    continue;
                }
                Serve(conn);
            }
        }
        catch (Exception) { }  // woken by Stop; an exception left on a thread of its own would end the app
    }

    /// Hands a connection to a thread of its own. Past MaxConnections, or with no thread to be had, it's dropped at
    /// once: the hook sees the pet close without an answer. Never throws, so no listener ends with it.
    void Serve(Stream c)
    {
        if (Interlocked.Increment(ref handling) <= MaxConnections)
            try
            {
                Run(() =>
                {
                    try { Handle(c); }
                    finally { Interlocked.Decrement(ref handling); }
                });
                return;
            }
            catch (Exception ex) { Warn(ex.Message); }  // the user's task limit, say
        else Warn($"more than {MaxConnections} connections at once, dropping some");
        Interlocked.Decrement(ref handling);
        try { c.Dispose(); } catch (Exception) { }
    }

    /// A log line about connections the pet couldn't take, at most one every 10 s: a flood would fill the log.
    void Warn(string why)
    {
        long now = Environment.TickCount64, last = Interlocked.Read(ref warned);
        if (now - last >= 10000 && Interlocked.CompareExchange(ref warned, now, last) == last) Log.Write("hooks: " + why);
    }

    void Handle(Stream c)
    {
        try
        {
            using (c)
            {
                // a client that connects and then stays silent is cut off after Ipc.TimeoutMs, and again every
                // Ipc.TimeoutMs: a read begun after a cancel (Windows) is cut too. Only that timer runs on the thread
                // pool, so a busy pool makes the cut-off late but never holds up an answer.
                using var cutoff = new Timer(_ => Cut(c), null, Ipc.TimeoutMs, Ipc.TimeoutMs);
                var line = Ipc.ReadLine(c, Ipc.MaxRequest);
                if (line == null) return;  // a liveness probe, or a hook that gave up
                var reply = Answer(line);
                if (reply != null) c.Write(Encoding.UTF8.GetBytes(reply + "\n"));
            }
        }
        catch (Exception) { }  // too big, too slow, or the hook went away: dropped
    }

    /// Ends a read or write still waiting on a connection. Windows cancels it: closing a handle that a blocking read
    /// is using would wait for that read. On Unix, closing the socket wakes the read (.NET shuts it down first).
    static void Cut(Stream c)
    {
        try
        {
            if (OperatingSystem.IsWindows()) CancelIoEx(((PipeStream)c).SafePipeHandle, IntPtr.Zero);
            else c.Dispose();
        }
        catch (Exception) { }  // answered and closed already
    }

    /// The reply to one request line; null (no reply) for a line that isn't JSON.
    string Answer(string line)
    {
        JsonObject req;
        try { req = JsonNode.Parse(line, documentOptions: new JsonDocumentOptions { MaxDepth = Ipc.MaxDepth }) as JsonObject; }
        catch (Exception) { return null; }
        if (req == null) return null;
        if (!(req[Ipc.V] is JsonValue v && v.TryGetValue(out double ver) && ver == Ipc.Version)) return Refuse("unsupported version");
        switch (req[Ipc.Kind] is JsonValue k && k.TryGetValue(out string kind) ? kind : null)
        {
            case "ping":
                lock (logLock)
                    return new JsonObject
                    {
                        [Ipc.Ok] = true, [Ipc.App] = "AiPet", [Ipc.Pid] = Environment.ProcessId,
                        [Ipc.Recent] = new JsonArray(recent.Select(l => (JsonNode)l).ToArray()),
                    }.ToJsonString();
            case "event":
                string outcome, log;
                try { (outcome, log) = sessions.Apply(req); }
                catch (Exception ex)
                {
                    outcome = "error:" + ex.GetType().Name;
                    log = "event -> " + outcome;
                    Log.Write($"hooks: an event failed: {ex}");
                }
                Record(log);
                if (outcome is not ("ignored" or "stale") && !outcome.StartsWith("error:", StringComparison.Ordinal))
                    try { Changed?.Invoke(); } catch (Exception ex) { Log.Write("hooks: " + ex.Message); }
                return outcome.StartsWith("error:", StringComparison.Ordinal) ? Refuse(outcome)
                    : new JsonObject { [Ipc.Ok] = true, [Ipc.Outcome] = outcome }.ToJsonString();
            default:
                return Refuse("unknown request");
        }
    }

    static string Refuse(string why) => new JsonObject { [Ipc.Ok] = false, [Ipc.Error] = why }.ToJsonString();

    /// One line per event, ignored ones too (the doctor's test event is one): kept for pings, and in hook-events.log,
    /// which keeps its last half past 64 KB.
    void Record(string line)
    {
        line = DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss.fff", CultureInfo.InvariantCulture) + " " + line;
        lock (logLock)
        {
            recent.Enqueue(line);
            while (recent.Count > RecentLines) recent.Dequeue();
            try
            {
                var path = Paths.HookEventsLog;
                if (File.Exists(path) && new FileInfo(path).Length > 64 * 1024)
                {
                    var text = File.ReadAllText(path);
                    var half = text[(text.Length / 2)..];
                    File.WriteAllText(path, half[(half.IndexOf('\n') + 1)..]);
                }
                File.AppendAllText(path, line + Environment.NewLine);
            }
            catch (Exception) { }
        }
    }

    static void Run(Action work) => new Thread(() => work()) { IsBackground = true, Name = "AiPet hooks" }.Start();

    // ------------------------------------------------------------------ the socket
    /// Windows: synchronous instances, for threads that block on them: an asynchronous one completes its waits and reads
    /// on the thread pool.
    static NamedPipeServerStream Create(bool first) => OperatingSystem.IsWindows()
        ? NamedPipeServerStreamAcl.Create(Ipc.Endpoint, PipeDirection.InOut, NamedPipeServerStream.MaxAllowedServerInstances,
            PipeTransmissionMode.Byte, first ? PipeOptions.FirstPipeInstance : PipeOptions.None,
            Ipc.BufferSize, Ipc.BufferSize, security ??= OnlyMe())
        : throw new PlatformNotSupportedException("Unix listens on a socket (Bind)");

    /// Windows: only this user, and not over the network. Not CurrentUserOnly, which grants the token's owner: that's
    /// Administrators in an elevated pet, whose pipe a normal hook then couldn't open (dotnet/runtime#123903). The
    /// owner is the user too, which is what the hook checks (Ipc.Connect).
    [SupportedOSPlatform("windows")]
    static PipeSecurity OnlyMe()
    {
        using var me = WindowsIdentity.GetCurrent();
        var sec = new PipeSecurity();
        sec.SetOwner(me.User);
        sec.AddAccessRule(new PipeAccessRule(me.User, PipeAccessRights.FullControl, AccessControlType.Allow));
        // .NET never sets PIPE_REJECT_REMOTE_CLIENTS
        sec.AddAccessRule(new PipeAccessRule(new SecurityIdentifier(WellKnownSidType.NetworkSid, null), PipeAccessRights.FullControl, AccessControlType.Deny));
        return sec;
    }

    /// Unix: a live pet accepts a connect; a crashed one left a socket file that refuses it, which Bind replaces.
    static void RefuseLiveServer()
    {
        using var live = Ipc.Connect();
        if (live != null) throw new IOException("another pet is listening there");
    }

    /// Unix: the socket, in place of what a crashed pet left at the path (only once RefuseLiveServer found no live
    /// one). Its mode is set before it listens: .NET leaves it as the umask made it, and until Listen every connect is
    /// refused anyway. Never an asynchronous call on it or on what it accepts: the first would make it non-blocking,
    /// and .NET would wait for the socket engine, and so for the pool, from then on.
    [UnsupportedOSPlatform("windows")]
    static Socket Bind()
    {
        File.Delete(Ipc.Endpoint);
        var s = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
        try
        {
            s.Bind(new UnixDomainSocketEndPoint(Ipc.Endpoint));
            File.SetUnixFileMode(Ipc.Endpoint, UnixFileMode.UserRead | UnixFileMode.UserWrite);
            s.Listen();
            return s;
        }
        catch (Exception)
        {
            s.Dispose();
            throw;
        }
    }

    /// Unix: whether the peer runs as this user, as CurrentUserOnly checked: the file's mode keeps other users out, but
    /// not root. SO_PEERCRED on Linux, getpeereid elsewhere; a peer it can't tell is refused.
    static bool SameUser(Socket s)
    {
        try
        {
            uint uid;
            if (OperatingSystem.IsLinux())
            {
                Span<byte> cred = stackalloc byte[12];  // struct ucred { pid, uid, gid }
                if (s.GetRawSocketOption(1, 17, cred) != cred.Length) return false;  // SOL_SOCKET, SO_PEERCRED
                uid = BitConverter.ToUInt32(cred[4..]);
            }
            else if (getpeereid((int)s.Handle, out uid, out _) != 0) return false;
            return uid == geteuid();
        }
        catch (Exception) { return false; }
    }

    [DllImport("libc")] static extern uint geteuid();
    [DllImport("libc")] static extern int getpeereid(int fd, out uint uid, out uint gid);

    [DllImport("kernel32.dll")] static extern bool CancelIoEx(SafeHandle file, IntPtr overlapped);
}

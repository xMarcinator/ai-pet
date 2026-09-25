using System.Globalization;
using System.IO;
using System.IO.Pipes;
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
/// Listening and answering run on threads of their own (Changed too), not on the thread pool: while the pool is busy
/// (the pet's watchers, a burst of hooks reading transcripts) a listener waiting on it makes no new instance, and a
/// hook that finds every instance taken for longer than it waits (Ipc.Connect) drops its event as if the pet were closed.
public sealed class HookServer
{
    /// Instances waiting for a hook at any time, each with a thread that makes the next one as soon as a hook takes it.
    const int Listeners = 8;
    const int RecentLines = 30;

    readonly AgentSessions sessions;
    readonly Queue<string> recent = new();
    readonly object logLock = new();
    volatile bool stopping;
    static PipeSecurity security;

    /// A hook event changed a chat.
    public event Action Changed;

    public HookServer(AgentSessions sessions) => this.sessions = sessions;

    /// Once, after the app's single-instance check: another pet would have the name already.
    public void Start()
    {
        try
        {
            if (!OperatingSystem.IsWindows()) RefuseLiveServer();
            var first = Create(first: true);  // Windows: UnauthorizedAccessException when the name is taken
            // .NET leaves the socket as the umask made it; until this, CurrentUserOnly's peer check keeps others out
            if (!OperatingSystem.IsWindows()) File.SetUnixFileMode(Ipc.Endpoint, UnixFileMode.UserRead | UnixFileMode.UserWrite);
            for (int i = 0; i < Listeners; i++)
            {
                var s = i == 0 ? first : Create(first: false);
                Run(() => Listen(s));
            }
            Log.Write("hooks: listening on " + Ipc.Endpoint);
        }
        catch (Exception ex) { Log.Write($"hooks: can't listen on {Ipc.Endpoint}: {ex.Message}"); }
    }

    /// Takes the socket down. A listener's wait for a hook can't be cancelled, so each is woken by a connection of its
    /// own, sees the pet stopping and closes its instance instead of making the next. Hooks being answered finish.
    public void Stop()
    {
        stopping = true;
        for (int i = 0; i < 2 * Listeners; i++)
            try
            {
                using var wake = new NamedPipeClientStream(".", Ipc.Endpoint, PipeDirection.InOut);
                wake.Connect(0);
            }
            catch (Exception) { break; }  // no instance left waiting
    }

    /// One listener: waits for a hook, makes the next instance, hands the connection to a thread of its own, and waits
    /// on the new instance.
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
                    // Unix: a peer with another uid; Windows: a client that left at once. The new instance comes first,
                    // so the name never goes away.
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
                Run(() => Handle(conn));
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

    void Handle(NamedPipeServerStream c)
    {
        // a client that connects and then stays silent is cut off after Ipc.TimeoutMs. Only that timer runs on the
        // thread pool, so a busy pool makes the cut-off late but never holds up an answer.
        using var cutoff = new Timer(_ => Cut(c), null, Ipc.TimeoutMs, Timeout.Infinite);
        try
        {
            using (c)
            {
                var line = Ipc.ReadLine(c, Ipc.MaxRequest);
                if (line == null) return;  // a liveness probe, or a hook that gave up
                var reply = Answer(line);
                if (reply != null) c.Write(Encoding.UTF8.GetBytes(reply + "\n"));
            }
        }
        catch (Exception) { }  // too big, too slow, or the hook went away: dropped
    }

    /// Ends a read or write still waiting on a connection. Windows cancels it: closing a handle that a blocking read
    /// is using would wait for that read. On Unix, closing the socket wakes the read.
    static void Cut(NamedPipeServerStream c)
    {
        try
        {
            if (OperatingSystem.IsWindows()) CancelIoEx(c.SafePipeHandle, IntPtr.Zero);
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
    /// Synchronous instances, for threads that block on them: an asynchronous one completes its waits and reads on the
    /// thread pool (Windows).
    static NamedPipeServerStream Create(bool first)
    {
        if (OperatingSystem.IsWindows())
            return NamedPipeServerStreamAcl.Create(Ipc.Endpoint, PipeDirection.InOut, NamedPipeServerStream.MaxAllowedServerInstances,
                PipeTransmissionMode.Byte, first ? PipeOptions.FirstPipeInstance : PipeOptions.None,
                Ipc.BufferSize, Ipc.BufferSize, security ??= OnlyMe());
        // no FirstPipeInstance: on Unix .NET makes that "fail if the socket file exists", which a crash leaves behind
        // (RefuseLiveServer tells a live pet from that). CurrentUserOnly checks each peer's uid.
        return new NamedPipeServerStream(Ipc.Endpoint, PipeDirection.InOut, NamedPipeServerStream.MaxAllowedServerInstances,
            PipeTransmissionMode.Byte, PipeOptions.CurrentUserOnly, Ipc.BufferSize, Ipc.BufferSize);
    }

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

    /// Unix: a live pet accepts a connect; a crashed one left a socket file that refuses it, which Create replaces.
    static void RefuseLiveServer()
    {
        using var live = Ipc.Connect();
        if (live != null) throw new IOException("another pet is listening there");
    }

    [DllImport("kernel32.dll")] static extern bool CancelIoEx(SafeHandle file, IntPtr overlapped);
}

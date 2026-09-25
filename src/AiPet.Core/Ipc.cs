using System.IO;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using System.Security.Principal;
using System.Text;

namespace AiPet;

/// How the hooks reach the running pet: a local socket only the current user can open. Shared by AiPet.Core (the
/// pet's HookServer) and AiPet.Hook (linked source file), so it uses nothing else from Core but Paths.
///   Windows: the named pipe \\.\pipe\AiPet-<user SID>-<session id>
///   Linux:   the socket /run/user/<uid>/aipet.sock (else $XDG_RUNTIME_DIR, else DataDir/aipet.sock), mode 0600
/// One request per connection: the client writes one UTF-8 JSON line ending in \n, the pet answers with one line,
/// and both close. The answer is what tells the hook the pet has its event.
///   {"v":1,"type":"event","agent":"claude"|"codex","at":..,"sent":..,"pid":..,"env":{..},"payload":{..}}
///       -> {"ok":true,"outcome":"<state>|ignored|stale|removed"}
///   {"v":1,"type":"ping"} -> {"ok":true,"app":"AiPet","pid":..,"recent":["<hook-events.log line>", ..]}   (the doctor)
/// at: when the hook started (its Main, before stdin was read), sent: when it sent the event (Unix seconds, ms
/// precision). Claude starts a hook registered directly itself, so at is about when Claude dispatched the event; the
/// plugin's goes through bash, sh and its launcher first (a few ms on Linux, maybe tens on Git Bash), so there at is
/// that much later and doesn't order events dispatched closer together. Codex's shell starts it late (PowerShell on
/// Windows, by 1-4 s), so for Codex the pet goes by the payload's own ids first (AgentSessions).
/// Older hooks also sent "packaged" and "parent" (Codex), which the pet ignores, and some left out sent (lag=?).
public static class Ipc
{
    public const int Version = 1;
    /// A request's size limit, how long either end waits for the other, and the pipe buffers (big enough that a usual
    /// request is written without waiting).
    public const int MaxRequest = 4 << 20, TimeoutMs = 2000, BufferSize = 64 << 10;
    /// How long a hook waits at most for a free instance of a pipe that exists (all taken: a burst of events, or a pet
    /// busy for a moment; on Linux, a pet that doesn't take connections), and the budget of its whole run from its Main.
    /// Claude gives a hook 5 s: stdin (StdinMs) + an instance (what the budget has left once the reply's time is set
    /// aside, so about 4500 - 2000 - 2000 = 500 ms at least) + the reply (TimeoutMs) = 4500 ms, which leaves 500 ms for
    /// starting it (bash, sh and the launcher for the plugin, then the runtime). A hook that was slow to get its stdin
    /// waits for an instance that much less; one that got none in time sends nothing.
    public const int ConnectMs = 2500, HookBudgetMs = 4500, StdinMs = 2000;
    /// The longest string of an event the hook passes on (Write's tool_input holds the whole file, say).
    public const int MaxString = 256 << 10;
    /// How deep a request may nest: an event at the hook's own parse limit (64) is one level deeper in its envelope.
    public const int MaxDepth = 128;
    /// The only fields of tool_input the pet reads (which file, skill or patch): all the hook keeps of a tool call
    /// that is still over MaxRequest once its strings are cut.
    public static readonly string[] ToolInputKeys = { "file_path", "notebook_path", "skill", "command" };

    public const string V = "v", Kind = "type", Agent = "agent", At = "at", Sent = "sent", Pid = "pid", Env = "env",
        Payload = "payload", Ok = "ok", Outcome = "outcome", Error = "error", App = "app", Recent = "recent";

    /// Claude's environment variables that say where a chat runs; the hook passes them in "env".
    public static readonly string[] ClaudeEnv = { "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID" };

    /// The pipe's name (Windows) or the socket's path. AIPET_PIPE overrides it, for testing next to a running pet.
    public static readonly string Endpoint =
        Environment.GetEnvironmentVariable("AIPET_PIPE") is { Length: > 0 } e ? (OperatingSystem.IsWindows() ? e : Path.GetFullPath(e))
        : OperatingSystem.IsWindows() ? WindowsPipe() : UnixSocket();

    public static double UnixTime(DateTime utc) => new DateTimeOffset(utc).ToUnixTimeMilliseconds() / 1000.0;

    /// A connection to the running pet, or null when it isn't listening (and then nothing was written anywhere).
    public static NamedPipeClientStream Connect() => Connect(ConnectMs, out _);

    /// The same, waiting at most busyMs for a free instance when the pipe exists but every one is taken. busy: it gave
    /// up on a pipe that is still there, so the pet runs but couldn't take the connection in time.
    public static NamedPipeClientStream Connect(int busyMs, out bool busy)
    {
        busy = false;
        // Windows checks the owner itself: CurrentUserOnly compares token owners, which differ between an elevated
        // and a normal process of the same user (dotnet/runtime#123903), so an elevated agent's hook would be refused
        var c = new NamedPipeClientStream(".", Endpoint, PipeDirection.InOut,
            OperatingSystem.IsWindows() ? PipeOptions.Asynchronous : PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
        try
        {
            if (OperatingSystem.IsWindows())
            {
                try { c.Connect(0); }
                // every instance is taken. The pipe exists, so wait for one; the kernel does the waiting. Connect(0)
                // alone would take that for "not running".
                catch (TimeoutException) when (PipeExists())
                {
                    busy = true;
                    c.Connect(Math.Max(0, busyMs));
                    busy = false;
                }
                if (!OwnedByMe(c)) throw new UnauthorizedAccessException("the pipe isn't the pet's");
                return c;
            }
            // Unix: a busy pet queues connects, but connect() has no time limit and blocks for as long as the queue is
            // full (a suspended pet, say). So it runs on a thread of its own, left blocked when the wait is up: the
            // hook's exit ends it, and c stays undisposed since that thread still uses it. At least 50 ms: a pet that
            // takes connections does so at once, but the thread has to get to run first.
            Exception failed = null;
            var connect = new Thread(() => { try { c.Connect(0); } catch (Exception ex) { failed = ex; } }) { IsBackground = true };
            connect.Start();
            if (!connect.Join(Math.Max(50, busyMs)))
            {
                // the socket is there, so something listens on it (a pet that quit left one that refuses at once)
                busy = File.Exists(Endpoint);
                return null;
            }
            if (failed != null) throw failed;
            return c;
        }
        catch (Exception)
        {
            // missing, stale, not ours, or busy past the wait (not a pipe that went away meanwhile: that pet quit)
            busy = busy && PipeExists();
            c.Dispose();
            return null;
        }
    }

    /// Sends one request line and returns the reply line, or null when the pet closed without one. Throws when the
    /// pipe breaks (a request over MaxRequest, say) or nothing comes back within TimeoutMs.
    public static string Ask(Stream pipe, string request)
    {
        using var cts = new CancellationTokenSource(TimeoutMs);
        pipe.WriteAsync(Encoding.UTF8.GetBytes(request + "\n"), cts.Token).AsTask().GetAwaiter().GetResult();
        return ReadLineAsync(pipe, BufferSize, cts.Token).GetAwaiter().GetResult();
    }

    /// One \n-terminated UTF-8 line, or null at the end of the stream. Throws past `max` bytes or when `ct` fires.
    public static async Task<string> ReadLineAsync(Stream s, int max, CancellationToken ct)
    {
        var buf = new byte[16 << 10];
        var line = new MemoryStream();
        while (true)
        {
            int n = await s.ReadAsync(buf, ct);
            if (n == 0) return null;
            if (Append(line, buf, n, max) is { } text) return text;
        }
    }

    /// The same, blocking: the pet reads on threads of its own, which mustn't wait for the thread pool.
    public static string ReadLine(Stream s, int max)
    {
        var buf = new byte[16 << 10];
        var line = new MemoryStream();
        while (true)
        {
            int n = s.Read(buf);
            if (n == 0) return null;
            if (Append(line, buf, n, max) is { } text) return text;
        }
    }

    /// Adds what was read to the line: the whole line once its \n has come, else null.
    static string Append(MemoryStream line, byte[] buf, int n, int max)
    {
        int nl = Array.IndexOf(buf, (byte)'\n', 0, n);
        line.Write(buf, 0, nl >= 0 ? nl : n);
        if (line.Length > max) throw new InvalidDataException("request too large");
        return nl >= 0 ? Encoding.UTF8.GetString(line.GetBuffer(), 0, (int)line.Length) : null;
    }

    // ------------------------------------------------------------------ Windows
    /// Pipe names are machine-wide: the user and the session keep two users' (or one user's two sessions') pets apart.
    [SupportedOSPlatform("windows")]
    static string WindowsPipe()
    {
        try
        {
            using var me = WindowsIdentity.GetCurrent();
            // not Process.GetCurrentProcess().SessionId: .NET gets that by listing every process on the machine, after
            // enabling SeDebugPrivilege, which a program that runs on every agent event has no business doing
            if (!ProcessIdToSessionId((uint)Environment.ProcessId, out uint session)) return "AiPet-" + Environment.UserName;
            return $"AiPet-{me.User.Value}-{session}";
        }
        catch { return "AiPet-" + Environment.UserName; }
    }

    /// False only when no instance of the pipe exists at all. Unlike File.Exists, WaitNamedPipe never takes one.
    static bool PipeExists() => WaitNamedPipeW(@"\\.\pipe\" + Endpoint, 1) || Marshal.GetLastPInvokeError() != 2;  // ERROR_FILE_NOT_FOUND

    /// The pet sets itself (the user, not the token owner) as the pipe's owner, and nobody else can.
    [SupportedOSPlatform("windows")]
    static bool OwnedByMe(PipeStream pipe)
    {
        using var me = WindowsIdentity.GetCurrent();
        return pipe.GetAccessControl().GetOwner(typeof(SecurityIdentifier)) is SecurityIdentifier owner && owner == me.User;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool WaitNamedPipeW(string name, uint timeout);
    [DllImport("kernel32.dll")] static extern bool ProcessIdToSessionId(uint pid, out uint session);

    // ------------------------------------------------------------------ Linux
    /// The pet and a hook must come to the same path whatever their environments say, so the uid's folder comes
    /// before XDG_RUNTIME_DIR: a hook run from a snap (VS Code's, say) gets XDG_RUNTIME_DIR=/run/user/<uid>/snap.<name>,
    /// one from an SSH login may get none. A socket path can't be longer than 107 bytes (103 on macOS).
    static string UnixSocket()
    {
        foreach (var dir in new[] { LoginRuntimeDir(), Environment.GetEnvironmentVariable("XDG_RUNTIME_DIR") })
            if (!string.IsNullOrEmpty(dir) && Directory.Exists(dir) && Encoding.UTF8.GetByteCount(Path.Combine(dir, "aipet.sock")) < 104)
                return Path.Combine(dir, "aipet.sock");
        return Path.Combine(Paths.DataDir, "aipet.sock");
    }

    /// /run/user/<effective uid>, where logind puts the user's XDG_RUNTIME_DIR.
    static string LoginRuntimeDir() => EffectiveUid() is { } uid ? "/run/user/" + uid : null;

    /// The effective uid (Linux), without starting anything: the second of the Uid line's ids in /proc/self/status.
    internal static string EffectiveUid()
    {
        try
        {
            foreach (var line in File.ReadLines("/proc/self/status"))
                if (line.StartsWith("Uid:")) return line.Split((char[])null, StringSplitOptions.RemoveEmptyEntries)[2];
        }
        catch { }
        return null;
    }
}

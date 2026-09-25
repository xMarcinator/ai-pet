using System.Text;
using System.Text.Json;

namespace AiPet;

/// Codex chats seen through Codex's own session files, for when its hooks don't report them: hooks that aren't
/// trusted yet (Codex skips them silently), a desktop app that doesn't run them, or an interrupted turn (on
/// Windows AiPet registers no Interrupt hook). The hook's live state wins whenever it's newer (see Board).
///
/// Reads, never writes:
///   ~/.codex/session_index.jsonl        {id, thread_name, updated_at} per chat: its name, and which chats exist
///   ~/.codex/sessions/**/rollout-*.jsonl one file per chat: session_meta first (id, originator, cwd, source), then
///                                        event_msg lines: task_started, item_completed, task_complete, turn_aborted
/// These are Codex's internal formats (0.155/0.156), so everything here is best effort.
public sealed class CodexWatcher
{
    /// Chats active within this long are shown.
    static readonly TimeSpan Recent = TimeSpan.FromHours(3);

    sealed class Chat
    {
        public string Id, Path, Name, Cwd, Where, State = "idle", Detail = "Ready", Prop;
        public long Offset;
        public long Length = -1;  // the file's size and time when last looked at (-1: not yet)
        public DateTime Mtime, LookAgain;  // LookAgain: when to look at the file of a chat idle for hours again
        public double Ts;
        public bool Skip;  // a sub-agent or one of Codex's own helper threads
    }

    readonly Dictionary<string, Chat> threads = new();
    readonly Dictionary<string, string> pathById = new();
    DateTime lastScan = DateTime.MinValue;
    bool scannedAll;
    long indexLength = -1;
    DateTime indexTime;
    volatile IReadOnlyList<Session> snapshot = Array.Empty<Session>();
    CancellationTokenSource loop;

    /// The chats as bubbles, keyed "codex:<id>" like the hook's.
    public IReadOnlyList<Session> Sessions => snapshot;

    public void Start()
    {
        loop?.Cancel();
        loop = new CancellationTokenSource();
        var ct = loop.Token;
        _ = Task.Run(async () =>
        {
            while (!ct.IsCancellationRequested)
            {
                try { Poll(); } catch (Exception ex) { Log.Write("codex watcher: " + ex.Message); }
                try { await Task.Delay(2000, ct); } catch (TaskCanceledException) { }
            }
        }, ct);
    }

    public void Stop() => loop?.Cancel();

    void Poll()
    {
        var home = Paths.CodexHome;
        if (!Directory.Exists(home)) { snapshot = Array.Empty<Session>(); return; }
        var now = DateTime.UtcNow;

        // which chats exist and their names (read again only when the index changes); the rollout files are located once per chat
        var index = new FileInfo(Path.Combine(home, "session_index.jsonl"));
        if (index.Exists && (index.Length != indexLength || index.LastWriteTimeUtc != indexTime))
        {
            (indexLength, indexTime) = (index.Length, index.LastWriteTimeUtc);
            foreach (var (id, name, _) in ReadIndex(index.FullName))
            {
                if (!threads.TryGetValue(id, out var t)) threads[id] = t = new Chat { Id = id };
                t.Name = name;
            }
        }
        if (now - lastScan > TimeSpan.FromSeconds(30)) { ScanFiles(Path.Combine(home, "sessions")); lastScan = now; }

        var list = new List<Session>();
        foreach (var t in threads.Values)
        {
            if (t.Path == null && !pathById.TryGetValue(t.Id, out t.Path)) continue;
            if (now < t.LookAgain) continue;
            try
            {
                // cheap checks first: an unchanged file isn't opened, and a chat untouched for hours isn't read at all
                // until its file changes (Windows may keep an open file's time stale, so its size counts too)
                var fi = new FileInfo(t.Path);
                if (!fi.Exists) continue;
                bool firstLook = t.Length == -1, changed = fi.Length != t.Length || fi.LastWriteTimeUtc != t.Mtime;
                (t.Length, t.Mtime) = (fi.Length, fi.LastWriteTimeUtc);
                if (t.Offset == 0 && now - fi.LastWriteTimeUtc > Recent && (firstLook || !changed)) t.LookAgain = now.AddSeconds(30);
                else if (changed && !Read(t)) t.Length = -2;  // couldn't open it just now: try again next time
            }
            catch (Exception ex)  // one odd file mustn't hide the other chats (it's tried again once it changes)
            {
                Log.Write($"codex watcher: {Path.GetFileName(t.Path)}: {ex.Message}");
            }
            if (t.Skip || t.Ts == 0 || now - DateTimeOffset.FromUnixTimeMilliseconds((long)(t.Ts * 1000)).UtcDateTime > Recent) continue;
            // only the index's name (the app's own title); Board falls back to the folder, below the hook's first prompt
            list.Add(new Session
            {
                Id = "codex:" + t.Id, Agent = "codex", Ts = t.Ts, Name = t.Name,
                Eff = t.State, Detail = t.Detail, Prop = t.Prop, Cwd = t.Cwd ?? "", Where = t.Where, Source = "log",
            });
        }
        snapshot = list;
    }

    static IEnumerable<(string Id, string Name, DateTime Updated)> ReadIndex(string path)
    {
        string text;
        try
        {
            using var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            fs.Seek(Math.Max(0, fs.Length - 512 * 1024), SeekOrigin.Begin);
            text = new StreamReader(fs, Encoding.UTF8).ReadToEnd();
        }
        catch { yield break; }
        foreach (var line in text.Split('\n'))
        {
            if (line.Length < 10) continue;
            string id = null, name = null; DateTime updated = DateTime.MinValue;
            try
            {
                using var doc = JsonDocument.Parse(line);
                var r = doc.RootElement;
                id = r.TryGetProperty("id", out var i) ? i.GetString() : null;
                name = r.TryGetProperty("thread_name", out var n) ? n.GetString() : null;
                if (r.TryGetProperty("updated_at", out var u) && DateTime.TryParse(u.GetString(), null, System.Globalization.DateTimeStyles.AdjustToUniversal, out var d)) updated = d;
            }
            catch { continue; }
            if (id != null) yield return (id, name, updated);
        }
    }

    /// Map chat ids to their rollout files (rollout-<time>-<id>.jsonl). A chat keeps writing to the file of the day it
    /// started, so the first scan goes through every day folder (for older chats that get resumed); after that only
    /// the last few weeks of folders, for new chats.
    void ScanFiles(string root)
    {
        var found = new List<(string Id, string Rollout)>();
        try
        {
            var days = Directory.EnumerateDirectories(root, "*", SearchOption.AllDirectories).Where(d => d.Length - root.Length >= 11)
                .OrderByDescending(d => d);
            foreach (var day in scannedAll ? days.Take(21) : days)
                foreach (var f in Directory.EnumerateFiles(day, "rollout-*.jsonl"))
                {
                    var name = Path.GetFileNameWithoutExtension(f);
                    if (name.Length >= 36) found.Add((name[^36..], f));
                }
        }
        catch { }
        scannedAll = true;
        foreach (var (id, path) in found)
        {
            pathById[id] = path;
            // chats started in the last few hours that have no name yet aren't in the index
            if (!threads.ContainsKey(id) && DateTime.UtcNow - File.GetLastWriteTimeUtc(path) < Recent)
                threads[id] = new Chat { Id = id, Path = path };
        }
    }

    /// Read what was appended since last time (the first time: the start, for session_meta, and the last 512 KB).
    /// False if the file couldn't be opened or read just now.
    static bool Read(Chat t)
    {
        try
        {
            using var fs = new FileStream(t.Path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            long len = fs.Length;
            if (len == t.Offset) return true;
            if (t.Offset == 0)
            {
                var first = ReadLine(fs, 4 * 1024 * 1024);
                if (first != null) Meta(t, first);  // never throws, so the offset always moves past that line
                t.Offset = Math.Max(fs.Position, len - 512 * 1024);
            }
            if (len < t.Offset) t.Offset = 0;  // rewritten
            fs.Seek(t.Offset, SeekOrigin.Begin);
            var buf = new byte[len - t.Offset];
            int n = fs.Read(buf, 0, buf.Length);
            int end = Array.LastIndexOf(buf, (byte)'\n', Math.Max(0, n - 1));
            if (end < 0) return true;  // no complete line yet
            var text = Encoding.UTF8.GetString(buf, 0, end + 1);
            t.Offset += end + 1;
            foreach (var line in text.Split('\n'))
                if (line.Contains("\"event_msg\"")) Event(t, line);
            return true;
        }
        catch (IOException) { return false; }
        catch (UnauthorizedAccessException) { return false; }
    }

    static string ReadLine(FileStream fs, int max)
    {
        var bytes = new List<byte>();
        int b;
        while (bytes.Count < max && (b = fs.ReadByte()) >= 0 && b != '\n') bytes.Add((byte)b);
        return bytes.Count > 0 ? Encoding.UTF8.GetString(bytes.ToArray()) : null;
    }

    static void Meta(Chat t, string line)
    {
        try
        {
            using var doc = JsonDocument.Parse(line);
            if (!doc.RootElement.TryGetProperty("payload", out var p)) return;
            if (p.TryGetProperty("cwd", out var c) && c.ValueKind == JsonValueKind.String) t.Cwd = c.GetString()!.Replace(@"\\?\", "");
            var origin = p.TryGetProperty("originator", out var o) && o.ValueKind == JsonValueKind.String ? o.GetString() : null;
            t.Where = origin switch
            {
                "Codex Desktop" or "codex_work_desktop" => "desktop",
                "codex-tui" or "codex_cli_rs" => "terminal",
                "codex_exec" => "exec",
                { Length: > 0 } other => other.ToLowerInvariant(),
                _ => null,
            };
            // sub-agents and Codex's own background threads (reviews, memory) aren't chats of their own
            if (p.TryGetProperty("source", out var s) && s.ValueKind == JsonValueKind.Object) t.Skip = true;
            if (p.TryGetProperty("thread_source", out var ts) && ts.ValueKind == JsonValueKind.String &&
                ts.GetString() is { } src && (src == "guardian_review" || src.StartsWith("memory"))) t.Skip = true;
        }
        catch { }  // not JSON, or not the shape expected (another Codex version, a stray file)
    }

    static void Event(Chat t, string line)
    {
        try
        {
            using var doc = JsonDocument.Parse(line);
            var r = doc.RootElement;
            if (!r.TryGetProperty("payload", out var p) || !p.TryGetProperty("type", out var type)) return;
            double ts = r.TryGetProperty("timestamp", out var tse) && DateTimeOffset.TryParse(tse.GetString(), out var when)
                ? when.ToUnixTimeMilliseconds() / 1000.0 : 0;
            void Set(string state, string detail, string prop = null) { t.State = state; t.Detail = detail; t.Prop = prop; if (ts > 0) t.Ts = ts; }
            switch (type.GetString())
            {
                case "task_started": Set("thinking", "Thinking"); break;
                case "task_complete": Set("done", "Done"); break;
                case "turn_aborted": Set("idle", "Interrupted"); break;
                case "item_completed" when t.State is "thinking" or "working":
                    // only finished items are saved, so this is what it just did, not what it's doing
                    var item = p.TryGetProperty("item", out var it) && it.TryGetProperty("type", out var itype) ? itype.GetString() : null;
                    switch (item)
                    {
                        case "CommandExecution": Set("working", "Running commands", "laptop"); break;
                        case "FileChange": Set("working", "Editing files", "laptop"); break;
                        case "WebSearch": Set("working", "Browsing the web", "lens"); break;
                        case "McpToolCall": Set("working", "Using a tool", "laptop"); break;
                        // reasoning, a message: it's back to thinking, so an older tool doesn't outlive the hook's "Thinking"
                        default: Set("thinking", "Thinking"); break;
                    }
                    break;
            }
        }
        catch { }  // not JSON, or not the shape expected
    }
}

using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AiPet.Golden;

/// Mode board: golden data for the Rust Board and Codex log watcher (rust/crates/aipet-core/src/board.rs and
/// codex_watcher.rs), in rust/crates/aipet-core/tests/golden/board/, which tests/board.rs replays:
///
///   dotnet run --project rust/golden -c Release -- board
///
/// board.json holds cases of steps. A step is a Board.Refresh, whose inputs are data (the hooks' chats as AgentSessions
/// entries, CodexWatcher's sessions, the Jira and GitHub watchers' settings, issues, review requests, pull requests by
/// key and errors, the music player and the music switch) and whose outputs are All, Cards, Extra, State, Prop and
/// Dismissed; or a Dismiss. The cases:
///  - tests/AiPet.Tests/BoardTests.cs's, their chats and logs made by the real AgentSessions and CodexWatcher;
///  - the tables: states by age, names, folders and links of the hooks' chats, reviews, music, the caps, the mood,
///    dismissals coming back;
///  - seeded random hook-versus-log merges of Codex chats: turns (v7 or not, ended or not, from a clock set back), times,
///    places, folders and names on either side;
///  - AppOf, AgentLabel and PrLabel over tables.
/// Board.Refresh reads the real clock (Board.Unix). Each step runs at the start of a fresh millisecond, which is its
/// `now`, and the clock is read again after it: a case whose step crossed into the next millisecond is taken again from
/// its start.
///
/// watcher.json holds CodexWatcher's polls (its private Poll, run on this thread) over fixture Codex homes: the files
/// written, their times, and the sessions each poll gives. Times in a file are written as `%T<seconds>%`, and file times
/// and the sessions' ts as offsets from T, a whole second when the case began: each side puts its own T in their place.
///
/// Cases marked os_specific give another answer on another OS (Path.GetFileName's rules, file name casing): the Rust test
/// compares them only on the OS the file was written on ("os").
static class BoardMode
{
    static readonly JsonSerializerOptions Json = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    static string J(object o) => JsonSerializer.Serialize(o, Json);

    const BindingFlags Private = BindingFlags.NonPublic | BindingFlags.Instance;

    static readonly FieldInfo ChatsField = typeof(AgentSessions).GetField("chats", Private)
        ?? throw new InvalidOperationException("AgentSessions.chats not found");
    static readonly FieldInfo SnapshotField = typeof(CodexWatcher).GetField("snapshot", Private)
        ?? throw new InvalidOperationException("CodexWatcher.snapshot not found");
    static readonly MethodInfo PollMethod = typeof(CodexWatcher).GetMethod("Poll", Private)
        ?? throw new InvalidOperationException("CodexWatcher.Poll not found");

    public static int Run(string repo, string data, string[] args)
    {
        if (args.Length > 0) return Program.Usage();
        Generate(repo, data);
        return 0;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    static void Generate(string repo, string data)
    {
        if (Path.GetFullPath(Paths.DataDir) != Path.GetFullPath(data))
            throw new InvalidOperationException($"AiPet.Core reads {Paths.DataDir}, not the temp folder {data}");
        // a PR's number is written in the current culture; the Rust port writes the invariant culture's
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        var dir = Path.Combine(repo, "rust", "crates", "aipet-core", "tests", "golden", "board");
        Directory.CreateDirectory(dir);
        string os = OperatingSystem.IsWindows() ? "windows" : OperatingSystem.IsMacOS() ? "macos" : "linux";

        var cases = new List<BoardCase>();
        cases.AddRange(MergeTests(Path.Combine(data, "merge")));
        cases.AddRange(ReviewTests());
        cases.AddRange(Tables());
        for (int seed = 1; seed <= 12; seed++) cases.Add(RandomMerges(seed));
        var sb = new StringBuilder("{\n");
        sb.Append("  \"about\": ").Append(J("Written by rust/golden (dotnet run --project rust/golden -c Release -- board) with "
            + "AiPet.Core's Board; replayed by rust/crates/aipet-core/tests/board.rs. Don't edit by hand.")).Append(",\n");
        sb.Append("  \"os\": ").Append(J(os)).Append(",\n");
        sb.Append("  \"labels\": ").Append(Labels()).Append(",\n");
        sb.Append("  \"cases\": [\n    ").Append(string.Join(",\n    ", cases.Select(Run))).Append("\n  ]\n}\n");
        Write(Path.Combine(dir, "board.json"), sb.ToString());
        Console.WriteLine($"board.json: {cases.Count} cases, {cases.Sum(c => c.Steps.Count)} steps");

        var watcher = WatcherCases(Path.Combine(data, "codex")).ToList();
        sb = new StringBuilder("{\n");
        sb.Append("  \"about\": ").Append(J("Written by rust/golden (dotnet run --project rust/golden -c Release -- board) with "
            + "AiPet.Core's CodexWatcher; replayed by rust/crates/aipet-core/tests/board.rs. Don't edit by hand.")).Append(",\n");
        sb.Append("  \"os\": ").Append(J(os)).Append(",\n");
        sb.Append("  \"cases\": [\n    ").Append(string.Join(",\n    ", watcher.Select(w => w.ToJson()))).Append("\n  ]\n}\n");
        Write(Path.Combine(dir, "watcher.json"), sb.ToString());
        Console.WriteLine($"watcher.json: {watcher.Count} cases");
    }

    static void Write(string path, string text)
    {
        File.WriteAllText(path, text, new UTF8Encoding(false));
        Console.WriteLine($"wrote {path} ({new FileInfo(path).Length / 1024} KB)");
    }

    /// The first moment of a new millisecond on Board.Unix's clock.
    static double FreshMillisecond()
    {
        double t = Board.Unix, n;
        while ((n = Board.Unix) == t) { }
        return n;
    }

    // ------------------------------------------------------------------ the Board's cases
    /// What one refresh is made from.
    sealed class Inputs
    {
        public List<AgentSessions.Entry> Chats = new();
        public List<Session> Codex = new();
        public bool JiraEnabled, GitHubEnabled, MusicOn;
        public string JiraSite = "", JiraError, GitHubError;
        public List<JiraWatcher.Issue> Issues = new();
        public List<GitHubWatcher.PullRequest> Reviews = new();
        public Dictionary<string, string> PrForKey = new();
        public Media Media;
    }

    /// A music player that says what it's given.
    sealed class Media : IMediaPlayer
    {
        public string Name { get; init; }
        public string Song { get; init; }
        public string Artist { get; init; }
        public bool Playing { get; init; }
        public double TrackSince { get; init; }
#pragma warning disable CS0067 // never raised: the Board doesn't listen
        public event Action Changed;
#pragma warning restore CS0067
        public void Poll() { }
        public void PlayPause() { }
        public void Next() { }
        public void Previous() { }
        public void Focus() { }
    }

    sealed class NoSecrets : ISecretStore
    {
        public string Read(string key) => null;
        public void Write(string key, string user, string secret) { }
        public void Delete(string key) { }
    }

    /// A case: refreshes, each with its inputs made at its `now`, and dismissals.
    sealed class BoardCase(string name, bool osSpecific = false)
    {
        public readonly string Name = name;
        public readonly bool OsSpecific = osSpecific;
        public readonly List<object> Steps = new();

        public BoardCase Refresh(Func<double, Inputs> inputs)
        {
            Steps.Add(inputs);
            return this;
        }

        public BoardCase Dismiss(string id)
        {
            Steps.Add(id);
            return this;
        }
    }

    static void Set(object target, string property, object value) =>
        target.GetType().GetProperty(property)!.SetValue(target, value);

    /// Takes the case's steps on a board of its own, again from the start when a step's clock moved on.
    static string Run(BoardCase c)
    {
        for (int attempt = 1; ; attempt++)
        {
            var board = new Board();
            var jira = new JiraWatcher(new NoSecrets());
            var github = new GitHubWatcher(new NoSecrets());
            var codex = new CodexWatcher();
            var steps = new List<string>();
            bool moved = false;
            foreach (var step in c.Steps)
            {
                var hooks = new AgentSessions();
                var chats = (OrderedDictionary<string, AgentSessions.Entry>)ChatsField.GetValue(hooks)!;
                double now = FreshMillisecond();
                if (step is string id)
                {
                    board.Dismiss(id);
                    if (Board.Unix != now) { moved = true; break; }
                    steps.Add("{\"dismiss\":" + J(id) + ",\"now\":" + J(now) + "}");
                    continue;
                }
                var inputs = ((Func<double, Inputs>)step)(now);
                foreach (var e in inputs.Chats) chats.Add(e.Id, e.Clone());
                SnapshotField.SetValue(codex, (IReadOnlyList<Session>)inputs.Codex.Select(s => s.Clone()).ToList());
                jira.Config.Enabled = inputs.JiraEnabled;
                jira.Config.Site = inputs.JiraSite;
                Set(jira, nameof(JiraWatcher.Issues), inputs.Issues);
                Set(jira, nameof(JiraWatcher.LastError), inputs.JiraError);
                github.Config.Enabled = inputs.GitHubEnabled;
                Set(github, nameof(GitHubWatcher.ReviewRequests), inputs.Reviews);
                Set(github, nameof(GitHubWatcher.PrForKey), inputs.PrForKey);
                Set(github, nameof(GitHubWatcher.LastError), inputs.GitHubError);
                board.Refresh(hooks, jira, github, inputs.Media, inputs.MusicOn, codex);
                if (Board.Unix != now) { moved = true; break; }
                steps.Add("{\"now\":" + J(now) + ",\"inputs\":" + InputsJson(inputs) + ",\n        \"board\":" + BoardJson(board) + "}");
            }
            if (!moved)
            {
                var sb = new StringBuilder("{\"name\":").Append(J(c.Name));
                if (c.OsSpecific) sb.Append(",\"os_specific\":true");
                return sb.Append(",\"steps\":[\n      ").Append(string.Join(",\n      ", steps)).Append("\n    ]}").ToString();
            }
            if (attempt == 50) throw new InvalidOperationException($"{c.Name}: every try crossed a millisecond");
        }
    }

    static string InputsJson(Inputs i) => J(new
    {
        chats = i.Chats.Select(e => new
        {
            id = e.Id, agent = e.Agent, state = e.State, detail = e.Detail, prop = e.Prop, title = e.Title,
            chat_title = e.ChatTitle, cwd = e.Cwd, @where = e.Where, host_id = e.HostId, ts = e.Ts, ended = e.Ended,
            has_transcript = e.HasTranscript, turn = e.Turn, turn_ended = e.TurnEnded,
        }),
        codex = i.Codex.Select(SessionJson),
        jira = new
        {
            enabled = i.JiraEnabled, site = i.JiraSite, error = i.JiraError,
            issues = i.Issues.Select(x => new { key = x.Key, summary = x.Summary, status = x.Status, updated = x.Updated, url = x.Url }),
        },
        github = new
        {
            enabled = i.GitHubEnabled, error = i.GitHubError, pr_for_key = i.PrForKey,
            review_requests = i.Reviews.Select(p => new { repo = p.Repo, number = p.Number, title = p.Title, url = p.Url, updated = p.Updated, keys = p.Keys }),
        },
        media = i.Media == null ? null : new { name = i.Media.Name, song = i.Media.Song, artist = i.Media.Artist, playing = i.Media.Playing, track_since = i.Media.TrackSince },
        music_on = i.MusicOn,
    });

    static object SessionJson(Session s) => new
    {
        id = s.Id, eff = s.Eff, name = s.Name, detail = s.Detail, prop = s.Prop, cwd = s.Cwd, agent = s.Agent, kind = s.Kind,
        url = s.Url, ticket_url = s.TicketUrl, pr_url = s.PrUrl, @where = s.Where, link = s.Link, source = s.Source,
        turn = s.Turn, turn_ended = s.TurnEnded, ts = s.Ts, section = s.Section,
    };

    static string BoardJson(Board b) => J(new
    {
        all = b.All.Select(SessionJson),
        cards = b.Cards.Select(s => s.Id),
        extra = b.Extra,
        state = b.State,
        prop = b.Prop,
        dismissed = b.Dismissed.OrderBy(d => d.Key, StringComparer.Ordinal)
            .Select(d => new { id = d.Key, ts = d.Value.Ts, eff = d.Value.Eff, detail = d.Value.Detail }),
    });

    static AgentSessions.Entry Chat(string id, string state, double ts, string agent = "claude", string detail = "Ready",
        string prop = null, string title = "", string chatTitle = null, string cwd = null, string where = null,
        string hostId = null, bool transcript = false, string turn = null, bool turnEnded = false, double? ended = null) => new()
    {
        Id = id, Agent = agent, State = state, Detail = detail, Prop = prop, Title = title, ChatTitle = chatTitle, Cwd = cwd,
        Where = where, HostId = hostId, Ts = ts, HasTranscript = transcript, Turn = turn, TurnEnded = turnEnded, Ended = ended,
    };

    /// A chat as CodexWatcher reports it.
    static Session Log(string sid, string eff, double ts, string detail = "Thinking", string name = null, string cwd = "",
        string where = null, string prop = null, string turn = null, bool turnEnded = false) => new()
    {
        Id = "codex:" + sid, Agent = "codex", Ts = ts, Name = name, Eff = eff, Detail = detail, Prop = prop, Cwd = cwd,
        Where = where, Source = "log", Turn = turn, TurnEnded = turnEnded,
    };

    static string Sid(int n) => $"00000000-0000-4000-8000-{n:D12}";

    /// A UUIDv7 made at `unix` seconds: its time to the ms, its other bits from n.
    static string V7(double unix, int n)
    {
        long ms = (long)Math.Floor(unix * 1000);
        return $"{ms >> 16:x8}-{ms & 0xFFFF:x4}-7{n & 0xFFF:x3}-8{(n * 7919) & 0xFFF:x3}-{n:x12}";
    }

    // ------------------------------------------------------------------ tests/AiPet.Tests/BoardTests.cs
    /// BoardMergeTests: a Codex chat as its hook (the real AgentSessions) and its rollout file (the real CodexWatcher)
    /// report it.
    sealed class MergeTest
    {
        public readonly AgentSessions Hooks = new();
        public readonly string Sid = Guid.NewGuid().ToString();
        // now minus a little, so a done bubble isn't old enough yet to count as idle
        public readonly double T = Board.Unix - 8;
        public readonly string TurnX = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-20)).ToString();
        public readonly string TurnY = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-10)).ToString();
        readonly string home, rollout;

        public MergeTest(string root, string name)
        {
            home = Path.Combine(root, name);
            var dir = Directory.CreateDirectory(Path.Combine(home, "sessions", "2026", "09", "30")).FullName;
            rollout = Path.Combine(dir, $"rollout-2026-09-30T12-00-00-{Sid}.jsonl");
        }

        public string Hook(string ev, string turn, double at, JsonObject extra = null)
        {
            Environment.SetEnvironmentVariable("CODEX_HOME", home);
            var o = extra ?? new JsonObject();
            o["transcript_path"] = rollout;
            var p = new JsonObject { ["hook_event_name"] = ev, ["session_id"] = Sid };
            if (turn != null) p["turn_id"] = turn;
            foreach (var (k, v) in o) p[k] = v?.DeepClone();
            var envelope = new JsonObject
            {
                [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = "codex", [Ipc.At] = at, [Ipc.Sent] = at + 0.01,
                [Ipc.Pid] = 4242, [Ipc.Env] = new JsonObject(), [Ipc.Payload] = p,
            };
            return Hooks.Apply(envelope).Outcome;
        }

        /// Writes the chat's rollout file and has a CodexWatcher read it once.
        public List<Session> Log(params (double Ts, string Type, string Turn)[] events)
        {
            var lines = new List<string>
            {
                new JsonObject { ["type"] = "session_meta", ["payload"] = new JsonObject { ["id"] = Sid, ["cwd"] = "/work/app", ["originator"] = "codex-tui" } }.ToJsonString(),
            };
            foreach (var (ts, type, turn) in events)
            {
                var payload = new JsonObject { ["type"] = type };
                if (turn != null) payload["turn_id"] = turn;
                var when = DateTimeOffset.FromUnixTimeMilliseconds((long)Math.Round(ts * 1000)).ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture);
                lines.Add(new JsonObject { ["timestamp"] = when, ["type"] = "event_msg", ["payload"] = payload }.ToJsonString());
            }
            File.WriteAllText(rollout, string.Concat(lines.Select(l => l + "\n")));
            Environment.SetEnvironmentVariable("CODEX_HOME", home);
            var watcher = new CodexWatcher();
            PollMethod.Invoke(watcher, null);
            var sessions = watcher.Sessions.ToList();
            if (!sessions.Any(s => s.Id == "codex:" + Sid)) throw new InvalidOperationException("the watcher didn't read the rollout file");
            return sessions;
        }

        public BoardCase Case(string name, List<Session> log)
        {
            var chats = Hooks.Snapshot();
            return new BoardCase("tests/" + name).Refresh(_ => new Inputs { Chats = chats, Codex = log });
        }
    }

    static JsonObject Bash(string command, string toolUseId) => new()
    {
        ["tool_name"] = "Bash", ["tool_input"] = new JsonObject { ["command"] = command }, ["tool_use_id"] = toolUseId,
    };

    static IEnumerable<BoardCase> MergeTests(string root)
    {
        {
            var m = new MergeTest(root, "interrupt");
            m.Hook("UserPromptSubmit", m.TurnX, m.T - 3);
            // Codex sent it just before the user pressed Esc; PowerShell started its hook after
            m.Hook("PreToolUse", m.TurnX, m.T + 2, Bash("rm -rf build", "call_1"));
            yield return m.Case("LogsInterrupt_BeatsALaterStartedHookOfThatTurn",
                m.Log((m.T - 3.5, "task_started", m.TurnX), (m.T, "turn_aborted", m.TurnX)));
        }
        {
            var m = new MergeTest(root, "error-end");
            // the turn ended in an error (no Stop hook) before the prompt's hook had even started
            m.Hook("UserPromptSubmit", m.TurnX, m.T + 2, new JsonObject { ["prompt"] = "go" });
            yield return m.Case("LogsErrorEnd_BeatsTheHooksLatePrompt",
                m.Log((m.T, "task_started", m.TurnX), (m.T + 0.5, "task_complete", m.TurnX)));
        }
        {
            var m = new MergeTest(root, "later-turn");
            m.Hook("UserPromptSubmit", m.TurnX, m.T - 5);
            m.Hook("UserPromptSubmit", m.TurnY, m.T);
            // written after the hook's at, but of the turn before
            yield return m.Case("HookOfALaterTurn_BeatsTheLogsOlderEnd",
                m.Log((m.T - 5.5, "task_started", m.TurnX), (m.T + 1, "turn_aborted", m.TurnX)));
        }
        {
            var m = new MergeTest(root, "both-ended");
            m.Hook("UserPromptSubmit", m.TurnX, m.T - 2);
            m.Hook("PostCompact", m.TurnX, m.T + 2, new JsonObject { ["trigger"] = "manual" });
            yield return m.Case("BothHaveTheTurnsEnd_TheNewerReportWins",
                m.Log((m.T - 2.5, "task_started", m.TurnX), (m.T + 1, "task_complete", m.TurnX)));
        }
        {
            var m = new MergeTest(root, "no-turns");
            // an older Codex whose log names no turns
            m.Hook("UserPromptSubmit", m.TurnX, m.T - 3);
            m.Hook("PreToolUse", m.TurnX, m.T + 2, Bash("ls", "call_1"));
            yield return m.Case("WithoutTurnIds_TheNewerReportWins",
                m.Log((m.T - 3.5, "task_started", null), (m.T, "turn_aborted", null)));
        }
        {
            var m = new MergeTest(root, "same-turn");
            m.Hook("UserPromptSubmit", m.TurnX, m.T - 3);
            m.Hook("PreToolUse", m.TurnX, m.T + 2, Bash("ls", "call_1"));
            yield return m.Case("LogOfTheSameTurnStillGoing_GoesByTime", m.Log((m.T - 3.5, "task_started", m.TurnX)));
        }
    }

    static List<JiraWatcher.Issue> Issues(double now, int n, string status = "In Progress") =>
        Enumerable.Range(1, n).Select(i => new JiraWatcher.Issue($"ABC-{i}", "Task " + i, status, now - i, $"https://example.atlassian.net/browse/ABC-{i}")).ToList();

    /// BoardReviewTests.
    static IEnumerable<BoardCase> ReviewTests()
    {
        yield return new BoardCase("tests/WatchersError_IsShownFirst_AndNotCountedAsAReview").Refresh(now => new Inputs
        {
            Issues = Issues(now, 4), GitHubEnabled = true, GitHubError = "GitHub didn't accept the token",
        });
        yield return new BoardCase("tests/BothWatchersErrors_AreShown_AndOnlyReviewsOverflow").Refresh(now => new Inputs
        {
            Issues = Issues(now, 5), JiraEnabled = true, JiraError = "Jira didn't accept the email/API token",
            GitHubEnabled = true, GitHubError = "GitHub didn't accept the token",
        });
        yield return new BoardCase("tests/FewReviews_AndAnError_AllShow").Refresh(now => new Inputs
        {
            Issues = Issues(now, 2), GitHubEnabled = true, GitHubError = "GitHub didn't accept the token",
        });
        yield return new BoardCase("tests/JiraIssue_SaysItsStatus_NotThatAReviewWasRequested").Refresh(now => new Inputs
        {
            Issues = Issues(now, 2), PrForKey = new() { ["ABC-1"] = "https://github.com/owner/app/pull/12" },
        });
    }

    // ------------------------------------------------------------------ tables
    static IEnumerable<BoardCase> Tables()
    {
        // each state around the ages it goes idle at, and the order: by priority, then newest first
        yield return new BoardCase("states-by-age").Refresh(now =>
        {
            var i = new Inputs();
            int n = 0;
            foreach (var (state, limit) in new[] { ("thinking", 900.0), ("working", 900.0), ("done", 20.0), ("attention", 3600.0), ("idle", 60.0) })
                foreach (var d in new[] { -0.001, 0, 0.001, 5 })
                    i.Chats.Add(Chat($"claude:{Sid(++n)}", state, now - limit - d, detail: state + " " + d, prop: n % 3 == 0 ? "laptop" : n % 3 == 1 ? "lens" : null));
            i.Chats.Add(Chat($"claude:{Sid(++n)}", null, now - 1, detail: null));
            i.Chats.Add(Chat($"claude:{Sid(++n)}", "working", now - 1, detail: "tie a"));
            i.Chats.Add(Chat($"claude:{Sid(++n)}", "thinking", now - 1, detail: "tie b"));
            i.Chats.Add(Chat($"claude:{Sid(++n)}", "done", now + 100, detail: "from the future"));
            return i;
        });

        // names, folders, agents and links of the hooks' chats; ended and transcript-less Codex chats aren't chats
        yield return new BoardCase("hook-chats").Refresh(now => new Inputs
        {
            Chats =
            {
                Chat("claude:a", "working", now - 1, title: "Fix the tests", chatTitle: "The app's title", cwd: "/home/u/proj"),
                Chat("claude:b", "working", now - 2, title: "Fix the tests", cwd: "/home/u/proj"),
                Chat("claude:c", "working", now - 3, title: "", chatTitle: "", cwd: "/home/u/proj/"),
                Chat("claude:d", "working", now - 4, cwd: "/"),
                Chat("claude:e", "working", now - 5, cwd: null, agent: null),
                Chat("codex:f", "working", now - 6, agent: "codex", cwd: "", transcript: true),
                Chat("codex:g", "working", now - 7, agent: "codex", cwd: "/w/g"),
                Chat("claude:h", "working", now - 8, cwd: "/w/h", ended: now - 1),
                Chat("claude:i", "thinking", now - 9, where: "desktop", hostId: "local_" + Sid(1), cwd: "/w/i"),
                Chat("claude:j", "thinking", now - 10, where: "desktop", hostId: "local_" + Sid(2).ToUpperInvariant()),
                Chat("claude:k", "thinking", now - 11, where: "desktop", hostId: "local_+0x00000-0000-4000-8000-000000000003"),
                Chat("claude:l", "thinking", now - 12, where: "desktop", hostId: "local_" + Sid(4)[..35]),
                Chat("claude:m", "thinking", now - 13, where: "terminal", hostId: "local_" + Sid(5)),
                Chat("claude:n", "thinking", now - 14, where: "desktop", hostId: "session_" + Sid(6)),
                Chat("codex:" + Sid(7), "thinking", now - 15, agent: "codex", where: "desktop", transcript: true),
                Chat("codex:" + Sid(8).ToUpperInvariant(), "thinking", now - 16, agent: "codex", where: "desktop", transcript: true),
                Chat("codex:{" + Sid(9) + "}", "thinking", now - 17, agent: "codex", where: "desktop", transcript: true),
                Chat("codex:" + Sid(10), "thinking", now - 18, agent: "codex", where: "codex_vscode", transcript: true),
                Chat("codex:" + Sid(11), "thinking", now - 19, agent: "codex", where: null, transcript: true),
                Chat(Sid(12), "thinking", now - 20, where: "desktop", hostId: "local_" + Sid(12)),
                Chat("codex:x:" + Sid(13), "thinking", now - 21, agent: "codex", where: "desktop", transcript: true),
            },
        });

        // folders by Path.GetFileName's rules, which differ by OS
        yield return new BoardCase("folders", osSpecific: true).Refresh(now => new Inputs
        {
            Chats =
            {
                Chat("claude:a", "working", now - 1, cwd: @"C:\work\app\"),
                Chat("claude:b", "working", now - 2, cwd: @"C:\"),
                Chat("claude:c", "working", now - 3, cwd: @"\\server\share"),
                Chat("claude:d", "working", now - 4, cwd: @"C:app"),
                Chat("claude:e", "working", now - 5, cwd: @"a\b/c"),
            },
            Codex = { Log(Sid(1), "thinking", now - 6, cwd: @"D:\src\tool\"), Log(Sid(2), "thinking", now - 7, cwd: @"E:\") },
        });

        // the watcher's chats on their own: named by the index, else by their folder, else "Codex"; their links
        yield return new BoardCase("log-chats").Refresh(now => new Inputs
        {
            Codex =
            {
                Log(Sid(1), "thinking", now - 1, name: "Codex's title", cwd: "/w/a", where: "desktop"),
                Log(Sid(2), "working", now - 2, detail: "Running commands", prop: "laptop", name: "", cwd: "/w/b/", where: "terminal"),
                Log(Sid(3), "done", now - 3, detail: "Done", cwd: ""),
                Log(Sid(4), "idle", now - 4, detail: "Interrupted", cwd: "/"),
                Log("not-a-guid", "thinking", now - 5, where: "desktop"),
            },
        });

        // reviews: Jira issues and their pull requests, review requests merged into a Jira issue by key (ignoring case),
        // ticket links by the Jira site, errors only while their watcher is on
        yield return new BoardCase("reviews").Refresh(now => new Inputs
        {
            JiraSite = "  example.atlassian.net  ",
            Issues =
            {
                new("ABC-1", "First", "In Review", now - 10, "https://example.atlassian.net/browse/ABC-1"),
                new("ABC-2", "Second", "", now - 20, "https://example.atlassian.net/browse/ABC-2"),
                new("ABC-3", "", "Done", now - 30, "https://example.atlassian.net/browse/ABC-3"),
                new("abc-4", "Lower", "To Do", now - 40, "https://example.atlassian.net/browse/abc-4"),
            },
            PrForKey =
            {
                ["ABC-1"] = "https://github.com/owner/app/pull/12/",
                ["ABC-2"] = "https://x/y",
                ["ABC-3"] = "short",
                ["abc-1"] = "https://github.com/owner/other/pull/1",
            },
            Reviews =
            {
                new("owner/app", 12, "Fix it", "https://github.com/owner/app/pull/12", now - 5, new[] { "ABC-1" }),
                new("owner/app", 13, "Lower key", "https://github.com/owner/app/pull/13", now - 6, new[] { "ABC-4" }),
                new("owner/lib", 7, "No key", "https://github.com/owner/lib/pull/7", now - 7, Array.Empty<string>()),
                new("owner/lib", 8, "Other key", "https://github.com/owner/lib/pull/8", now - 8, new[] { "XYZ-9", "ABC-2" }),
                new("app", 9, "No owner", "https://github.com/app/pull/9", now - 5, new[] { "XYZ-1" }),
                new("a/b/", 10, "Odd repo", "https://github.com/a/b/pull/10", now - 5, new[] { "XYZ-2" }),
            },
            JiraError = "Jira is off, so this isn't shown",
            GitHubEnabled = true,
        });
        yield return new BoardCase("reviews-without-a-site").Refresh(now => new Inputs
        {
            JiraSite = " ",
            Reviews = { new("owner/app", 1, "T", "https://github.com/owner/app/pull/1", now - 1, new[] { "ABC-1" }) },
        }).Refresh(now => new Inputs
        {
            JiraSite = null,
            Reviews = { new("owner/app", 1, "T", "https://github.com/owner/app/pull/1", now - 1, new[] { "ABC-1" }) },
        });
        // the reviews cap with both errors, and the chats' cap
        yield return new BoardCase("caps").Refresh(now =>
        {
            var i = new Inputs
            {
                Issues = Issues(now, 6), JiraEnabled = true, JiraError = "Jira didn't answer in time", GitHubEnabled = true,
                GitHubError = "GitHub didn't accept the token",
            };
            for (int n = 1; n <= 7; n++)
                i.Chats.Add(Chat($"claude:{Sid(n)}", n == 3 ? "idle" : "working", now - n, cwd: "/w/" + n));
            return i;
        });

        // music: playing, then paused (still shown, having played), then off; a board that never heard it play
        Media Song(double since, bool playing, string artist = "Artist") =>
            new() { Name = "Player", Song = "Song", Artist = artist, Playing = playing, TrackSince = since };
        yield return new BoardCase("music").Refresh(now => new Inputs { Media = Song(now - 30, true), MusicOn = true })
            .Refresh(now => new Inputs { Media = Song(now - 30, false, artist: ""), MusicOn = true })
            .Refresh(now => new Inputs { Media = Song(now - 30, false), MusicOn = false })
            .Refresh(now => new Inputs { Media = Song(now - 30, true), MusicOn = false })
            .Refresh(now => new Inputs { Media = new() { Name = "Player", Song = null, Playing = true }, MusicOn = true })
            .Refresh(now => new Inputs
            {
                Media = Song(now - 1, true), MusicOn = true,
                Chats = { Chat("claude:a", "idle", now - 1, cwd: "/w/a") },
            });
        yield return new BoardCase("music-never-played").Refresh(now => new Inputs { Media = Song(now - 30, false), MusicOn = true })
            .Refresh(now => new Inputs { Media = null, MusicOn = true });

        // the pet's mood: the first chat's state and prop; idle with nothing for 5 minutes is sleep
        yield return new BoardCase("mood").Refresh(_ => new Inputs())
            .Refresh(now => new Inputs { Issues = Issues(now, 1) })
            .Refresh(now => new Inputs { Chats = { Chat("claude:a", "idle", now - 300), Chat("claude:b", "idle", now - 301) } })
            .Refresh(now => new Inputs { Chats = { Chat("claude:a", "idle", now - 300.001) } })
            .Refresh(now => new Inputs { Chats = { Chat("claude:a", "working", now - 1, prop: "lens"), Chat("claude:b", "attention", now - 2) } })
            .Refresh(now => new Inputs { Chats = { Chat("claude:a", "working", now - 1, prop: "laptop") } });

        // dismissed bubbles come back when they do something new
        const string Sid1 = "claude:" + "d1", Sid2 = "claude:" + "d2";
        Inputs Dismissable(double at, string detail = "Running command", string state = "working", double jiraTs = 0,
            string jiraStatus = "In Progress", string ghError = "GitHub didn't accept the token", bool second = true)
        {
            var i = new Inputs
            {
                Chats = { Chat(Sid1, state, at, detail: detail, cwd: "/w/one") },
                Issues = { new("ABC-1", "Task", jiraStatus, jiraTs == 0 ? at : jiraTs, "https://example.atlassian.net/browse/ABC-1") },
                GitHubEnabled = true, GitHubError = ghError,
            };
            if (second) i.Chats.Add(Chat(Sid2, "working", at - 1, cwd: "/w/two"));
            return i;
        }
        double t0 = 0;
        yield return new BoardCase("dismiss")
            .Refresh(now => Dismissable(t0 = now - 10))
            .Dismiss(Sid1).Dismiss("jira:ABC-1").Dismiss("gh:_error").Dismiss("nothing-there")
            .Refresh(now => Dismissable(t0))
            // within 2 s of when it was dismissed: still hidden; a Jira issue's new status doesn't count
            .Refresh(now => Dismissable(t0 + 2, jiraTs: t0 + 2, jiraStatus: "In Review"))
            // a new detail brings a chat back; a Jira issue comes back when it changes well after
            .Refresh(now => Dismissable(t0 + 2, detail: "Editing a.rs", jiraTs: t0 + 2.001))
            .Dismiss(Sid1).Dismiss(Sid2)
            .Refresh(now => Dismissable(t0 + 2, detail: "Editing a.rs", state: "attention"))
            // an error with the same text stays hidden, however new; another text brings it back
            .Refresh(now => Dismissable(t0 + 2, detail: "Editing a.rs", state: "attention", ghError: "GitHub didn't answer in time"))
            .Dismiss("gh:_error")
            // gone: forgotten, so it's back when it returns
            .Refresh(now => Dismissable(t0 + 2, second: false, ghError: null))
            .Refresh(now => Dismissable(t0 + 2));
    }

    // ------------------------------------------------------------------ generated hook-versus-log merges
    /// Codex chats that the hook and the log both report (and some only one), with every kind of turn on each side.
    static BoardCase RandomMerges(int seed)
    {
        var rng = new Random(seed);
        T Pick<T>(params T[] items) => items[rng.Next(items.Length)];
        // the chats' plan is drawn once, and each refresh makes it at its own now
        var plan = Enumerable.Range(1, 30).Select(n => new
        {
            N = n,
            Side = Pick("both", "both", "both", "hook", "log"),
            HookTurn = rng.Next(8), LogTurn = rng.Next(8),
            HookEnded = rng.Next(2) == 0, LogEnded = rng.Next(3) == 0,
            HookAge = Pick(1.0, 2.0, 2.0, 3.5, 30.0, 1000.0), LogAge = Pick(1.0, 2.0, 2.0, 3.0, 30.0, 25.0),
            HookState = Pick("thinking", "working", "done", "attention", "idle"),
            LogState = Pick("thinking", "working", "done", "idle"),
            HookName = rng.Next(3), LogNamed = rng.Next(2) == 0,
            HookWhere = Pick<string>(null, "desktop", "terminal", "codex_vscode"), LogWhere = Pick<string>(null, "desktop", "exec"),
            HookCwd = Pick<string>(null, "", "/w/hook"), LogCwd = Pick("", "/w/log/"),
            Prop = Pick<string>(null, "laptop", "lens"),
            Sid = rng.Next(4) == 0 ? $"thread-{n}" : Sid(n),
        }).ToList();
        return new BoardCase($"random-merges/{seed}").Refresh(now =>
        {
            // turns: 0 none, 1-3 v7s a minute back in order, 4 the same as 2 upper-cased, 5 not a v7, 6 from a clock set
            // back (past now + 5 s), 7 v7 just within now + 5 s
            string Turn(int k) => k switch
            {
                0 => null,
                1 => V7(now - 60, 1), 2 => V7(now - 50, 2), 3 => V7(now - 40, 3),
                4 => V7(now - 50, 2).ToUpperInvariant(),
                5 => "turn-" + seed,
                6 => V7(now + 5.002, 6),
                _ => V7(now + 4.999, 7),
            };
            var i = new Inputs();
            foreach (var p in plan)
            {
                if (p.Side != "log")
                    i.Chats.Add(Chat("codex:" + p.Sid, p.HookState, now - p.HookAge, agent: "codex",
                        detail: "hook " + p.HookState, prop: p.Prop, title: p.HookName >= 1 ? "Prompt " + p.N : "",
                        chatTitle: p.HookName == 2 ? "Title " + p.N : null, cwd: p.HookCwd, where: p.HookWhere, transcript: true,
                        turn: Turn(p.HookTurn), turnEnded: p.HookEnded));
                if (p.Side != "hook")
                    i.Codex.Add(Log(p.Sid, p.LogState, now - p.LogAge,
                        detail: "log " + p.LogState, name: p.LogNamed ? "Log name " + p.N : null, cwd: p.LogCwd, where: p.LogWhere,
                        turn: Turn(p.LogTurn), turnEnded: p.LogEnded));
            }
            return i;
        });
    }

    // ------------------------------------------------------------------ AppOf, AgentLabel, PrLabel
    static string Labels()
    {
        var agents = new[] { "claude", "codex", "jira", "github", "music", "other" };
        var wheres = new[] { null, "terminal", "exec", "desktop", "codex_vscode", "vscode", "claude-vscode", "sdk-ts", "sdk", "SDK-ts", "xsdk", "" };
        var appOf = agents.SelectMany(a => wheres.Select(w =>
        {
            var (label, color) = Board.AppOf(new Session { Agent = a, Where = w });
            return new { agent = a, @where = w, label, color };
        }));
        var prs = new[]
        {
            "https://github.com/Owner/repo/pull/123", "https://github.com/Owner/repo/pull/123/", "https://x/y", "a/b/c",
            "a/b/c/d", "", "////", "short",
        };
        return J(new
        {
            app_of = appOf,
            agent_label = agents.Select(a => new { agent = a, label = Board.AgentLabel(a) }),
            pr_label = prs.Select(u => new { url = u, label = Board.PrLabel(u) }),
        });
    }

    // ------------------------------------------------------------------ the Codex log watcher
    /// A piece of a file: UTF-8 text (with `%T<seconds>%` for a time) or bytes in hex, written Repeat times.
    sealed record Part(string Text, string Hex, int Repeat)
    {
        public static Part T(string text, int repeat = 1) => new(text, null, repeat);
        public static Part H(byte[] bytes, int repeat = 1) => new(null, Convert.ToHexString(bytes), repeat);

        public string ToJson()
        {
            var d = new Dictionary<string, object>();
            if (Text != null) d["text"] = Text;
            else d["hex"] = Hex;
            if (Repeat != 1) d["repeat"] = Repeat;
            return J(d);
        }
    }

    /// A fixture Codex home, the steps taken in it, and what each poll of one CodexWatcher gave.
    sealed class WatcherCase
    {
        public readonly string Name;
        readonly bool osSpecific;
        readonly string home;
        /// T, in ms: a whole second.
        readonly long t0 = (long)Math.Floor(Board.Unix) * 1000;
        readonly CodexWatcher watcher = new();
        readonly List<string> steps = new();

        public WatcherCase(string root, string name, bool osSpecific = false)
        {
            Name = name;
            this.osSpecific = osSpecific;
            home = Path.Combine(root, name.Replace('/', '-'));
        }

        string Time(string text)
        {
            var sb = new StringBuilder();
            int at = 0;
            for (int i; (i = text.IndexOf("%T", at, StringComparison.Ordinal)) >= 0; at = text.IndexOf('%', i + 2) + 1)
            {
                sb.Append(text, at, i - at);
                int end = text.IndexOf('%', i + 2);
                long ms = (long)Math.Round(decimal.Parse(text[(i + 2)..end], CultureInfo.InvariantCulture) * 1000);
                sb.Append(DateTimeOffset.FromUnixTimeMilliseconds(t0 + ms).ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture));
            }
            return sb.Append(text, at, text.Length - at).ToString();
        }

        /// Writes a file (made of the parts) and gives it a time: T plus `mtime` seconds.
        public WatcherCase Write(string file, int mtime, params Part[] parts)
        {
            var path = Path.Combine(home, file);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            var bytes = parts.SelectMany(p => Enumerable.Repeat(p.Text != null ? Encoding.UTF8.GetBytes(Time(p.Text)) : Convert.FromHexString(p.Hex), p.Repeat))
                .SelectMany(b => b).ToArray();
            File.WriteAllBytes(path, bytes);
            File.SetLastWriteTimeUtc(path, DateTimeOffset.FromUnixTimeMilliseconds(t0 + mtime * 1000L).UtcDateTime);
            steps.Add("{\"write\":" + J(file) + ",\"mtime\":" + mtime + ",\"parts\":[" + string.Join(",", parts.Select(p => p.ToJson())) + "]}");
            return this;
        }

        public WatcherCase Folder(string folder)
        {
            Directory.CreateDirectory(Path.Combine(home, folder));
            steps.Add("{\"folder\":" + J(folder) + "}");
            return this;
        }

        public WatcherCase Poll()
        {
            Environment.SetEnvironmentVariable("CODEX_HOME", home);
            PollMethod.Invoke(watcher, null);
            var sessions = watcher.Sessions.Select(s => new
            {
                id = s.Id, eff = s.Eff, name = s.Name, detail = s.Detail, prop = s.Prop, cwd = s.Cwd, agent = s.Agent, kind = s.Kind,
                @where = s.Where, source = s.Source, turn = s.Turn, turn_ended = s.TurnEnded,
                ts_ms = (long)Math.Round(s.Ts * 1000) - t0,
            });
            steps.Add("{\"poll\":" + J(sessions) + "}");
            return this;
        }

        public string ToJson()
        {
            var sb = new StringBuilder("{\"name\":").Append(J(Name));
            if (osSpecific) sb.Append(",\"os_specific\":true");
            return sb.Append(",\"steps\":[\n      ").Append(string.Join(",\n      ", steps)).Append("\n    ]}").ToString();
        }
    }

    static string Meta(string id, string payload) => $"{{\"timestamp\":\"%T-600%\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"{payload}}}}}\n";

    static string Ev(double at, string type, string rest = "") =>
        $"{{\"timestamp\":\"%T{at.ToString(CultureInfo.InvariantCulture)}%\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"{type}\"{rest}}}}}\n";

    static string Rollout(string day, string id) => $"sessions/{day}/rollout-2026-09-30T12-00-00-{id}.jsonl";

    static string Index(params (string Id, string Name)[] chats) =>
        string.Concat(chats.Select(c => $"{{\"id\":\"{c.Id}\",\"thread_name\":{(c.Name == null ? "null" : J(c.Name))},\"updated_at\":\"2026-09-30T12:00:00Z\"}}\n"));

    static IEnumerable<WatcherCase> WatcherCases(string root)
    {
        // named chats in the index, found in their day folders; one the index doesn't name yet; one without a file
        yield return new WatcherCase(root, "basic")
            .Write("session_index.jsonl", -60, Part.T(Index((Sid(1), "First chat"), (Sid(2), "Second chat"), (Sid(3), "No file"))))
            .Write(Rollout("2026/09/28", Sid(1)), -60, Part.T(Meta(Sid(1), ",\"cwd\":\"/work/one\",\"originator\":\"codex-tui\"")
                + Ev(-120, "task_started", $",\"turn_id\":\"{V7(1, 1)}\"")
                + Ev(-110, "item_completed", ",\"item\":{\"type\":\"CommandExecution\"}")
                + Ev(-100, "task_complete", $",\"turn_id\":\"{V7(1, 1)}\"")))
            .Write(Rollout("2026/09/29", Sid(2)), -60, Part.T(Meta(Sid(2), ",\"cwd\":\"C:\\\\work\\\\two\",\"originator\":\"Codex Desktop\"")
                + Ev(-90, "task_started", $",\"turn_id\":\"{V7(2, 2)}\"")))
            .Write(Rollout("2026/09/30", Sid(4)), -60, Part.T(Meta(Sid(4), ",\"cwd\":\"/work/four\",\"originator\":\"codex_exec\"")
                + Ev(-80, "task_started", $",\"turn_id\":\"t-4\"") + Ev(-70, "turn_aborted", $",\"turn_id\":\"t-4\"")))
            .Poll();

        // session_meta: where by originator, cwd, and the threads that aren't chats of their own
        {
            var metas = new (string Payload, string Why)[]
            {
                (",\"originator\":\"codex_work_desktop\",\"cwd\":\"\\\\\\\\?\\\\C:\\\\work\\\\app\"", "desktop, a long path"),
                (",\"originator\":\"codex_cli_rs\"", "terminal"),
                (",\"originator\":\"codex_vscode\"", "a client"),
                (",\"originator\":\"Codex_SDK_TS İ\"", "lower-cased"),
                (",\"originator\":\"\"", "empty"),
                (",\"originator\":42,\"cwd\":7", "not text"),
                (",\"source\":{\"subagent\":\"review\"},\"originator\":\"codex-tui\"", "a sub-agent"),
                (",\"source\":\"cli\",\"originator\":\"codex-tui\"", "a source that is text"),
                (",\"thread_source\":\"guardian_review\"", "a review thread"),
                (",\"thread_source\":\"memory_consolidation\"", "a memory thread"),
                (",\"thread_source\":\"user\"", "a user's thread"),
                (",\"thread_source\":5", "not text"),
            };
            var c = new WatcherCase(root, "meta");
            c.Write("session_index.jsonl", -60, Part.T(Index(metas.Select((m, n) => (Sid(n + 1), m.Why)).Append((Sid(20), "payload not an object"))
                .Append((Sid(21), "not JSON")).Append((Sid(22), "empty first line")).Append((Sid(23), "an event first")).ToArray())));
            for (int n = 0; n < metas.Length; n++)
                c.Write(Rollout("2026/09/30", Sid(n + 1)), -60, Part.T(Meta(Sid(n + 1), metas[n].Payload) + Ev(-30, "task_started")));
            c.Write(Rollout("2026/09/30", Sid(20)), -60, Part.T("{\"type\":\"session_meta\",\"payload\":[1]}\n" + Ev(-30, "task_started")));
            c.Write(Rollout("2026/09/30", Sid(21)), -60, Part.T("not json\n" + Ev(-30, "task_started")));
            c.Write(Rollout("2026/09/30", Sid(22)), -60, Part.T("\n" + Ev(-30, "task_started")));
            c.Write(Rollout("2026/09/30", Sid(23)), -60, Part.T(Ev(-40, "task_complete") + Ev(-30, "task_started")));
            yield return c.Poll();
        }

        // events, each chat a sequence of its own
        {
            var sequences = new (string Why, string Lines)[]
            {
                ("item types", Ev(-50, "task_started", ",\"turn_id\":\"a\"") + Ev(-49, "item_completed", ",\"item\":{\"type\":\"FileChange\"}")),
                ("web", Ev(-50, "task_started") + Ev(-49, "item_completed", ",\"item\":{\"type\":\"WebSearch\"}")),
                ("mcp", Ev(-50, "task_started") + Ev(-49, "item_completed", ",\"item\":{\"type\":\"McpToolCall\"}")),
                ("back to thinking", Ev(-50, "task_started") + Ev(-49, "item_completed", ",\"item\":{\"type\":\"CommandExecution\"}")
                    + Ev(-48, "item_completed", ",\"item\":{\"type\":\"Reasoning\"}")),
                ("no item", Ev(-50, "task_started") + Ev(-49, "item_completed", ",\"item\":{\"type\":\"CommandExecution\"}")
                    + Ev(-48, "item_completed")),
                ("item not an object", Ev(-50, "task_started") + Ev(-49, "item_completed", ",\"item\":[1]")),
                ("item type not text", Ev(-50, "task_started") + Ev(-49, "item_completed", ",\"item\":{\"type\":3}")),
                ("done then an item", Ev(-50, "task_started", ",\"turn_id\":\"b\"") + Ev(-49, "task_complete", ",\"turn_id\":\"b\"")
                    + Ev(-48, "item_completed", ",\"item\":{\"type\":\"CommandExecution\"}")),
                ("idle then an item", Ev(-50, "item_completed", ",\"item\":{\"type\":\"CommandExecution\"}")),
                ("interrupted", Ev(-50, "task_started", ",\"turn_id\":\"c\"") + Ev(-49, "turn_aborted", ",\"turn_id\":5")),
                ("a time that is null", Ev(-50, "task_started") + "{\"timestamp\":null,\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n"),
                ("a time that isn't text", Ev(-50, "task_started") + "{\"timestamp\":5,\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n"),
                ("a time that isn't one", Ev(-50, "task_started") + "{\"timestamp\":\"soon\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n"),
                ("no time", Ev(-50, "task_started") + "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n"),
                ("a type that isn't text", Ev(-50, "task_started") + "{\"timestamp\":\"%T-49%\",\"type\":\"event_msg\",\"payload\":{\"type\":1}}\n"),
                ("no type", Ev(-50, "task_started") + "{\"timestamp\":\"%T-49%\",\"type\":\"event_msg\",\"payload\":{}}\n"),
                ("payload not an object", Ev(-50, "task_started") + "{\"timestamp\":\"%T-49%\",\"type\":\"event_msg\",\"payload\":7}\n"),
                ("other lines", Ev(-50, "task_started") + "{\"timestamp\":\"%T-49%\",\"type\":\"response_item\",\"payload\":{\"type\":\"task_complete\"}}\n"
                    + "{\"timestamp\":\"%T-48%\",\"type\":\"response_item\",\"payload\":{\"type\":\"task_complete\",\"note\":\"\\\"event_msg\\\"\"}}\n"
                    + "{\"broken\":\"event_msg\"\n"),
                ("CRLF lines", (Ev(-50, "task_started", ",\"turn_id\":\"d\"") + Ev(-49, "task_complete", ",\"turn_id\":\"d\"")).Replace("\n", "\r\n")),
                ("a time with an offset", "{\"timestamp\":\"2026-09-30T14:00:00.000+02:00\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n"
                    + Ev(-49, "item_completed", ",\"item\":{\"type\":\"WebSearch\"}")),
                ("unknown events", Ev(-50, "task_started") + Ev(-49, "agent_message", ",\"message\":\"hi\"") + Ev(-48, "token_count")),
                ("names a turn, then none", Ev(-50, "task_started", ",\"turn_id\":\"e\"") + Ev(-49, "task_started")),
            };
            var c = new WatcherCase(root, "events");
            c.Write("session_index.jsonl", -60, Part.T(Index(sequences.Select((s, n) => (Sid(n + 1), s.Why)).ToArray())));
            for (int n = 0; n < sequences.Length; n++)
                c.Write(Rollout("2026/09/30", Sid(n + 1)), -60, Part.T(Meta(Sid(n + 1), "") + sequences[n].Lines));
            yield return c.Poll();
        }

        // the index: which lines it reads, the last of a chat's winning, and only its last 512 KB
        {
            var c = new WatcherCase(root, "names");
            var lines = new[]
            {
                $"{{\"id\":\"{Sid(1)}\",\"thread_name\":\"Old name\"}}", $"{{\"id\":\"{Sid(1)}\",\"thread_name\":\"New name\"}}",
                $"{{\"id\":\"{Sid(2)}\",\"thread_name\":\"Named\"}}", $"{{\"id\":\"{Sid(2)}\",\"thread_name\":null}}",
                $"{{\"id\":\"{Sid(3)}\",\"thread_name\":\"Kept\"}}", $"{{\"id\":\"{Sid(3)}\",\"thread_name\":5}}",
                $"{{\"id\":\"{Sid(4)}\",\"thread_name\":\"Kept too\"}}", $"{{\"id\":\"{Sid(4)}\",\"thread_name\":\"Not this\",\"updated_at\":1}}",
                $"{{\"id\":\"{Sid(5)}\",\"thread_name\":\"With null time\",\"updated_at\":null}}",
                $"{{\"id\":\"{Sid(6)}\",\"thread_name\":\"\"}}",
                $"{{\"id\":\"{Sid(7)}\",\"thread_name\":\"First\",\"thread_name\":\"Second\"}}",
                $"{{\"thread_name\":\"No id\"}}", $"{{\"id\":null,\"thread_name\":\"Null id\"}}", $"{{\"id\":8,\"thread_name\":\"Number id\"}}",
                "[1,2,3,4,5,6,7,8]", "{\"id\":1}", "not json at all", $"{{\"id\":\"{Sid(9)}\",\"thread_name\":\"Café ☕\"}}\r",
            };
            c.Write("session_index.jsonl", -60, Part.T(string.Concat(lines.Select(l => l + "\n"))));
            for (int n = 1; n <= 9; n++) c.Write(Rollout("2026/09/30", Sid(n)), -60, Part.T(Meta(Sid(n), "") + Ev(-30, "task_started")));
            c.Poll();
            // the index changes: read again
            c.Write("session_index.jsonl", -50, Part.T(string.Concat(lines.Select(l => l + "\n")) + $"{{\"id\":\"{Sid(2)}\",\"thread_name\":\"Renamed\"}}\n"));
            yield return c.Poll();
        }
        {
            // a name 400 KB from the end is in the last 512 KB, and not in the last 256
            var filler = "{\"id\":\"" + Sid(99) + "\",\"thread_name\":\"" + new string('x', 1000) + "\"}\n";
            yield return new WatcherCase(root, "index-tail")
                .Write("session_index.jsonl", -60, Part.T($"{{\"id\":\"{Sid(1)}\",\"thread_name\":\"Too far back\"}}\n"), Part.T(filler, 300),
                    Part.T($"{{\"id\":\"{Sid(3)}\",\"thread_name\":\"In the tail\"}}\n"), Part.T(filler, 400),
                    Part.T($"{{\"id\":\"{Sid(2)}\",\"thread_name\":\"At the end\"}}\n"))
                .Write(Rollout("2026/09/29", Sid(1)), -60, Part.T(Meta(Sid(1), "") + Ev(-30, "task_started")))
                .Write(Rollout("2026/09/30", Sid(2)), -60, Part.T(Meta(Sid(2), "") + Ev(-30, "task_started")))
                .Write(Rollout("2026/09/28", Sid(3)), -60, Part.T(Meta(Sid(3), "") + Ev(-30, "task_started")))
                .Poll();
        }

        // which chats are recent: by the file's time before it is read, then by the chat's own time
        yield return new WatcherCase(root, "recency")
            .Write("session_index.jsonl", -60, Part.T(Index((Sid(1), "Idle for hours"), (Sid(2), "Old events"), (Sid(3), "From the future"),
                (Sid(4), "Recent"))))
            .Write(Rollout("2026/09/27", Sid(1)), -4 * 3600, Part.T(Meta(Sid(1), "") + Ev(-4 * 3600, "task_started")))
            .Write(Rollout("2026/09/28", Sid(2)), -60, Part.T(Meta(Sid(2), "") + Ev(-4 * 3600, "task_started")))
            .Write(Rollout("2026/09/29", Sid(3)), -60, Part.T(Meta(Sid(3), "") + Ev(3600, "task_started")))
            .Write(Rollout("2026/09/30", Sid(4)), -60, Part.T(Meta(Sid(4), "") + Ev(-2 * 3600, "task_started")))
            // not in the index: an old file isn't a chat, a recent one is, even without events (though not shown then)
            .Write(Rollout("2026/09/25", Sid(5)), -4 * 3600, Part.T(Meta(Sid(5), "") + Ev(-60, "task_started")))
            .Write(Rollout("2026/09/26", Sid(6)), -60, Part.T(Meta(Sid(6), "")))
            .Write(Rollout("2026/09/24", Sid(7)), -60, Part.T(Meta(Sid(7), "") + Ev(-60, "task_started")))
            .Poll();

        // where rollout files are looked for: day folders (11 characters or more past sessions), and the name's pattern
        yield return new WatcherCase(root, "discovery")
            .Write("sessions/2026/09/rollout-2026-09-30T12-00-00-" + Sid(1) + ".jsonl", -60, Part.T(Meta(Sid(1), "") + Ev(-30, "task_started")))
            .Write("sessions/long-folder/rollout-2026-09-30T12-00-00-" + Sid(2) + ".jsonl", -60, Part.T(Meta(Sid(2), "") + Ev(-30, "task_started")))
            .Write("sessions/2026/09/30/deeper/rollout-2026-09-30T12-00-00-" + Sid(3) + ".jsonl", -60, Part.T(Meta(Sid(3), "") + Ev(-30, "task_started")))
            .Write("sessions/2026/09/29/rollout-2026-09-30T12-00-00-" + Sid(4) + ".json", -60, Part.T(Meta(Sid(4), "") + Ev(-30, "task_started")))
            .Write("sessions/2026/09/29/notrollout-" + Sid(5) + ".jsonl", -60, Part.T(Meta(Sid(5), "") + Ev(-30, "task_started")))
            .Write("sessions/2026/09/29/rollout-short.jsonl", -60, Part.T(Meta("short", "") + Ev(-30, "task_started")))
            .Write("sessions/2026/09/28/rollout-" + Sid(6) + ".jsonl", -60, Part.T(Meta(Sid(6), "") + Ev(-30, "task_started")))
            .Folder("sessions/2026/09/27/rollout-2026-09-30T12-00-00-" + Sid(7) + ".jsonl")
            .Poll();
        // a pattern that ignores case on Windows only
        yield return new WatcherCase(root, "discovery-case", osSpecific: true)
            .Write("sessions/2026/09/30/ROLLOUT-2026-09-30T12-00-00-" + Sid(1) + ".JSONL", -60, Part.T(Meta(Sid(1), "") + Ev(-30, "task_started")))
            .Poll();

        // a file read bit by bit: an incomplete line waits, what's appended is read, a file rewritten shorter is read anew;
        // the first read is session_meta and the last 512 KB, and session_meta's first 4 MB
        yield return new WatcherCase(root, "incremental")
            .Write("session_index.jsonl", -60, Part.T(Index((Sid(1), "Bit by bit"))))
            .Write(Rollout("2026/09/30", Sid(1)), -60, Part.T(Meta(Sid(1), ",\"cwd\":\"/w\"") + Ev(-50, "task_started", ",\"turn_id\":\"a\"")
                + "{\"timestamp\":\"%T-49%\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_com"))
            .Poll()
            .Write(Rollout("2026/09/30", Sid(1)), -59, Part.T(Meta(Sid(1), ",\"cwd\":\"/w\"") + Ev(-50, "task_started", ",\"turn_id\":\"a\"")
                + "{\"timestamp\":\"%T-49%\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"a\"}}\n"))
            .Poll()
            .Write(Rollout("2026/09/30", Sid(1)), -58, Part.T(Ev(-40, "task_started", ",\"turn_id\":\"b\"")))
            .Poll()
            .Poll();
        {
            // events 400 KB from the end are in the last 512 KB, and not in the last 256; the lines after them aren't
            // events; a first line of 3 MB is read whole, one past 4 MB isn't
            var events = Ev(-45, "task_complete") + new string(' ', 200) + "\n";
            var other = "{\"timestamp\":\"%T-38%\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\"}}" + new string(' ', 200) + "\n";
            yield return new WatcherCase(root, "tails")
                .Write("session_index.jsonl", -60, Part.T(Index((Sid(1), "Long"), (Sid(2), "Long first line"), (Sid(3), "First line of 3 MB"))))
                .Write(Rollout("2026/09/29", Sid(1)), -60, Part.T(Meta(Sid(1), ",\"cwd\":\"/w/long\"")), Part.T(Ev(-50, "task_complete")),
                    Part.T(events, 3000), Part.T(Ev(-40, "task_started") + Ev(-39, "item_completed", ",\"item\":{\"type\":\"FileChange\"}")),
                    Part.T(other, 1400))
                .Write(Rollout("2026/09/30", Sid(2)), -60, Part.T("{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/w/first\",\"instructions\":\""),
                    Part.T(new string('i', 1024), 4200), Part.T("\"}}\n" + Ev(-40, "task_started")))
                .Write(Rollout("2026/09/28", Sid(3)), -60, Part.T("{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/w/three\",\"instructions\":\""),
                    Part.T(new string('i', 1024), 3000), Part.T("\"}}\n" + Ev(-40, "task_started")))
                .Poll();
        }
        yield return new WatcherCase(root, "no-home").Poll();
        yield return new WatcherCase(root, "no-sessions").Write("session_index.jsonl", -60, Part.T(Index((Sid(1), "No file")))).Poll();
    }
}

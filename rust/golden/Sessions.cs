using System.Globalization;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AiPet.Golden;

/// Mode sessions: golden data for the Rust AgentSessions (rust/crates/aipet-core/src/sessions). Envelope sequences go
/// through the real AgentSessions.Apply, and what it gives goes to rust/crates/aipet-core/tests/golden/sessions/:
/// claude.json for Claude's events, codex.json for Codex's, which tests/sessions.rs replays.
///
///   dotnet run --project rust/golden -c Release -- sessions
///
/// A case is a store of its own and a list of steps. A step is an envelope, as the hook writes it and HookServer parses
/// it, with the clock (`now`) it was applied at, and what Apply gave: the outcome, the hook-events.log line and, at
/// chosen steps, Snapshot(). An exception is given as HookServer reports it (outcome "error:<type>"). Or a step writes
/// or deletes a file in the case's folder: a transcript, which a transcript_path names as "{files}", or Codex's
/// session_index.jsonl, since the folder is the case's CODEX_HOME. The Claude cases:
///  - each Claude case of tests/AiPet.Tests/OrderingTests.cs;
///  - the event and tool tables, where and host ids, titles from transcripts (missing, oversized, odd encodings and
///    lines), when titles are read, pruning, pairing and ordering edges (tool inputs whose keys come in another order,
///    whose numbers are spelled otherwise, or whose lines space and escape them otherwise), and malformed envelopes;
///  - seeded random chats: late hooks, a PermissionRequest either side of its PreToolUse, duplicates, SessionEnd and
///    resumes, a clock set back, renamed transcripts. Chance is only in how the inputs were made: they're all written
///    out.
/// The Codex cases:
///  - each case of CodexOrderingTests (a theory's each inline data);
///  - the event and tool tables (apply_patch's files), where from the transcript's originator (the first line, the
///    first 4 MB, the regex over the raw text), names from session_index.jsonl (lines of every shape, when they're read,
///    the last 256 KB), cwd and threads, the log's lag, malformed envelopes, turn ids (v7 or not, case, the last 16,
///    the forms the Guid parser takes, which can throw), tool calls (the last 64, 5 s spread, reruns' requests), a
///    clock set back, SessionEnd and pruning;
///  - seeded random chats on Windows (hooks 1-4 s late, no PostToolUse) and elsewhere: sub-agents, other threads,
///    reruns after a sandbox denial, late prompts, /compact, a clock set back with its turn ids, duplicates, SessionEnd
///    and resumes, names and originators coming late.
///
/// AgentSessions reads the real clock (Board.Unix). Each envelope is applied at the start of a fresh millisecond, and
/// `now` is that millisecond, which an envelope without a usable `at` gets. Apply reads the clock again later, but the
/// times of the cases keep an hour or more from the limits that compare with now (5 s, a day) on the side the clock
/// moves away from, so a slow Apply doesn't change anything.
///
/// Text that AgentSessions cuts in the middle of a surrogate pair (Short, the log's Clean) keeps a lone surrogate,
/// which any UTF-8 writer (hook-events.log, the ping reply) turns into U+FFFD. It's written here as U+FFFD too.
///
/// Cases marked os_specific give another answer on another OS (Path.GetFileName's rules): the Rust test compares them
/// only on the OS the file was written on ("os").
static class SessionsMode
{
    /// A transcript_path in the case's own folder: each side puts its folder in its place.
    const string Files = "{files}";

    static readonly JsonSerializerOptions Json = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    static string J(object o) => JsonSerializer.Serialize(o, Json);

    /// The mode: Program has pointed AIPET_DATA_DIR at `data`, a temp folder, before anything read Paths.
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
        // the log line's pid is written in the current culture (a minus sign may be U+2212); the Rust port writes the
        // invariant culture's
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;

        var root = Path.Combine(data, "sessions");
        var cases = new List<Replay>();
        var codex = new List<Replay>();
        Replay Case(string name, int every = 1, bool osSpecific = false)
        {
            var r = new Replay(root, name, every, osSpecific);
            cases.Add(r);
            return r;
        }
        Replay CodexCase(string name, int every = 1)
        {
            var r = new Replay(root, name, every, false);
            codex.Add(r);
            return r;
        }

        Ordering(Case);
        Events(Case("claude/events"));
        Describe(Case("claude/describe"), Tools);
        Describe(Case("claude/describe-paths", osSpecific: true), PathTools);
        Short(Case("claude/short", every: int.MaxValue));
        Notifications(Case("claude/notification-messages", every: int.MaxValue));
        WhereAndHost(Case("claude/where-and-host", every: int.MaxValue), Case("claude/where-changes"));
        Titles(Case("claude/titles", every: int.MaxValue));
        TitleReads(Case("claude/title-reads"));
        Prune(Case("claude/prune"));
        Pairing(Case);
        SameTick(Case("claude/same-tick"));
        Clock(Case("claude/clock"));
        Malformed(Case("claude/malformed", every: int.MaxValue));
        for (int seed = 1; seed <= 32; seed++) RandomChats(Case($"random/{seed}", every: 10), seed);

        CodexOrdering(CodexCase);
        CodexEvents(CodexCase("codex/events"));
        CodexDescribe(CodexCase("codex/describe"));
        CodexWhere(CodexCase("codex/where", every: int.MaxValue), CodexCase("codex/where-reads"));
        CodexNames(CodexCase("codex/names", every: int.MaxValue), CodexCase("codex/name-reads"), CodexCase("codex/name-window"));
        CodexCwdAndThreads(CodexCase("codex/cwd-and-threads"));
        CodexLag(CodexCase("codex/lag", every: int.MaxValue));
        CodexMalformed(CodexCase("codex/malformed", every: int.MaxValue));
        CodexTurnIds(CodexCase("codex/turn-ids"));
        CodexCalls(CodexCase("codex/calls"));
        CodexClock(CodexCase("codex/clock"));
        CodexEnd(CodexCase("codex/end-and-prune"));
        for (int seed = 1; seed <= 32; seed++) RandomCodexChats(CodexCase($"random-codex/{seed}", every: 10), seed);

        Write(repo, "claude.json", cases, "random/");
        Write(repo, "codex.json", codex, "random-codex/", "error:FormatException");
    }

    /// Writes one file of cases, and checks that it holds every kind of outcome (and `more`), and a stale event among
    /// its `random` cases.
    static void Write(string repo, string file, List<Replay> cases, string random, params string[] more)
    {
        var output = Path.Combine(repo, "rust", "crates", "aipet-core", "tests", "golden", "sessions", file);
        var sb = new StringBuilder();
        sb.Append("{\n");
        sb.Append("  \"about\": ").Append(J("Written by rust/golden (dotnet run --project rust/golden -c Release -- sessions) "
            + "with AiPet.Core's AgentSessions; replayed by rust/crates/aipet-core/tests/sessions.rs. Don't edit by hand.")).Append(",\n");
        sb.Append("  \"os\": ").Append(J(OperatingSystem.IsWindows() ? "windows" : OperatingSystem.IsMacOS() ? "macos" : "linux")).Append(",\n");
        sb.Append("  \"cases\": [\n    ").Append(string.Join(",\n    ", cases.Select(c => c.ToJson()))).Append("\n  ]\n");
        sb.Append("}\n");
        Directory.CreateDirectory(Path.GetDirectoryName(output));
        File.WriteAllText(output, sb.ToString(), new UTF8Encoding(false));

        var outcomes = cases.SelectMany(c => c.Outcomes).GroupBy(o => o).OrderBy(g => g.Key, StringComparer.Ordinal)
            .ToDictionary(g => g.Key, g => g.Count());
        Console.WriteLine($"{file}: {cases.Count} cases, {outcomes.Values.Sum()} envelopes; outcomes: "
            + string.Join(" ", outcomes.Select(o => $"{o.Key} {o.Value}")));
        foreach (var o in new[] { "idle", "thinking", "working", "attention", "done", "removed", "ignored", "stale" }.Concat(more))
            if (!outcomes.ContainsKey(o)) throw new InvalidOperationException($"{file}: no case gives the outcome {o}");
        var randoms = cases.Where(c => c.Name.StartsWith(random, StringComparison.Ordinal)).SelectMany(c => c.Outcomes).ToList();
        if (randoms.Count(o => o == "stale") == 0) throw new InvalidOperationException($"{file}: no random case has a stale event: pick other seeds");
        Console.WriteLine($"wrote {output} ({new FileInfo(output).Length / 1024} KB)");
    }

    // ------------------------------------------------------------------ a case
    /// One case: an AgentSessions of its own, a folder for its transcripts, and its steps as they're taken.
    sealed class Replay
    {
        public readonly string Name;
        readonly string dir;
        readonly int every;
        readonly bool osSpecific;
        readonly AgentSessions sessions = new();
        readonly List<string> steps = new();
        readonly List<string> outcomes = new();
        /// The last envelope step, and whether it has a snapshot: a case always ends with one.
        int last = -1;
        bool lastHasSnapshot;

        /// When the case began: its times are this plus or minus something.
        public readonly double T0 = Board.Unix;

        public Replay(string root, string name, int every, bool osSpecific)
        {
            Name = name;
            this.every = every;
            this.osSpecific = osSpecific;
            dir = Path.Combine(root, name.Replace('/', '-'));
            Directory.CreateDirectory(dir);
        }

        public IReadOnlyList<string> Outcomes => outcomes;

        public void Write(string file, params Part[] parts)
        {
            var bytes = parts.SelectMany(p => Enumerable.Repeat(p.Bytes(), p.Repeat)).SelectMany(b => b).ToArray();
            Retry(() => File.WriteAllBytes(Path.Combine(dir, file), bytes));
            steps.Add("{\"write\":" + J(file) + ",\"parts\":[" + string.Join(",", parts.Select(p => p.ToJson())) + "]}");
        }

        public void Delete(string file)
        {
            Retry(() => File.Delete(Path.Combine(dir, file)));
            steps.Add("{\"delete\":" + J(file) + "}");
        }

        /// A file just written can be held for a moment by a virus scanner (Windows): try again for a while.
        static void Retry(Action change)
        {
            for (int attempt = 1; ; attempt++)
                try
                {
                    change();
                    return;
                }
                catch (IOException) when (attempt < 100)
                {
                    Thread.Sleep(50);
                }
        }

        /// A Claude event, as the tests' Events.Envelope makes it.
        public string Claude(string sid, string ev, double at, JsonObject extra = null, JsonObject env = null, bool? snapshot = null) =>
            Apply(Envelope(at, Payload(sid, ev, extra), env), snapshot);

        /// A Codex event, as the Codex hook sends it (no env), with its turn_id when given.
        public string Codex(string sid, string ev, string turn, double at, JsonObject extra = null, bool? snapshot = null) =>
            Apply(CodexEnvelope(at, CodexPayload(sid, ev, turn, extra)), snapshot);

        public string Apply(JsonObject envelope, bool? snapshot = null) => Apply(envelope.ToJsonString(Json), snapshot);

        /// A request line as it's written, which may space and escape its JSON as the hook wouldn't.
        public string Apply(string line, bool? snapshot = null)
        {
            // as HookServer.Answer parses it
            var req = (JsonObject)JsonNode.Parse(line, documentOptions: new JsonDocumentOptions { MaxDepth = Ipc.MaxDepth });
            if (req[Ipc.Payload] is JsonObject p && p["transcript_path"] is JsonValue v && v.TryGetValue(out string path)
                && path.StartsWith(Files, StringComparison.Ordinal))
                p["transcript_path"] = dir + path[Files.Length..];
            // Codex's chat names come from $CODEX_HOME/session_index.jsonl: the case's own folder
            Environment.SetEnvironmentVariable("CODEX_HOME", dir);
            double now = FreshMillisecond();
            string outcome, log;
            try { (outcome, log) = sessions.Apply(req); }
            catch (Exception ex)
            {
                // as HookServer.Answer reports it
                outcome = "error:" + ex.GetType().Name;
                log = "event -> " + outcome;
            }
            if (Board.Unix - now > 60) throw new InvalidOperationException($"{Name}: an Apply took over a minute");
            outcomes.Add(outcome);
            bool snap = snapshot ?? outcomes.Count % every == 0;
            var step = new StringBuilder();
            step.Append("{\"now\":").Append(J(now)).Append(",\"envelope\":").Append(line)
                .Append(",\"outcome\":").Append(J(outcome)).Append(",\"log\":").Append(J(Mend(log)));
            if (snap) step.Append(",\"snapshot\":").Append(Snapshot());
            steps.Add(step.Append('}').ToString());
            last = steps.Count - 1;
            lastHasSnapshot = snap;
            return outcome;
        }

        string Snapshot() => "[" + string.Join(",", sessions.Snapshot().Select(e => J(EntryJson(e)))) + "]";

        public string ToJson()
        {
            // nothing changes the store after its last envelope, so the case's final state is its snapshot now
            if (last >= 0 && !lastHasSnapshot)
            {
                steps[last] = steps[last][..^1] + ",\"snapshot\":" + Snapshot() + "}";
                lastHasSnapshot = true;
            }
            var sb = new StringBuilder("{\"name\":").Append(J(Name));
            if (osSpecific) sb.Append(",\"os_specific\":true");
            return sb.Append(",\"steps\":[\n      ").Append(string.Join(",\n      ", steps)).Append("\n    ]}").ToString();
        }
    }

    /// The first moment of a new millisecond on Board.Unix's clock.
    static double FreshMillisecond()
    {
        double t = Board.Unix, n;
        while ((n = Board.Unix) == t) { }
        return n;
    }

    static object EntryJson(AgentSessions.Entry e) => new
    {
        id = Mend(e.Id), agent = Mend(e.Agent), state = Mend(e.State), detail = Mend(e.Detail), prop = Mend(e.Prop),
        title = Mend(e.Title), chat_title = Mend(e.ChatTitle), cwd = Mend(e.Cwd), @where = Mend(e.Where),
        host_id = Mend(e.HostId), ts = e.Ts, dispatched = e.Dispatched, dispatched_rank = e.DispatchedRank, ended = e.Ended,
        has_transcript = e.HasTranscript, turn = Mend(e.Turn), turn_ended = e.TurnEnded,
    };

    /// The text with each lone surrogate as U+FFFD, as a UTF-8 writer has it.
    static string Mend(string s)
    {
        if (s == null) return null;
        var sb = new StringBuilder(s.Length);
        for (int i = 0; i < s.Length; i++)
        {
            if (char.IsHighSurrogate(s[i]) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1])) sb.Append(s, i++, 2);
            else sb.Append(char.IsSurrogate(s[i]) ? '\uFFFD' : s[i]);
        }
        return sb.ToString();
    }

    /// A piece of a file: UTF-8 text or bytes in hex, written Repeat times.
    sealed record Part(string Text, string Hex, int Repeat)
    {
        public static Part T(string text, int repeat = 1) =>
            Mend(text) == text ? new Part(text, null, repeat) : throw new ArgumentException("text with a lone surrogate: write it as bytes");
        public static Part H(byte[] bytes, int repeat = 1) => new Part(null, Convert.ToHexString(bytes), repeat);
        public byte[] Bytes() => Text != null ? Encoding.UTF8.GetBytes(Text) : Convert.FromHexString(Hex);

        public string ToJson()
        {
            var d = new Dictionary<string, object>();
            if (Text != null) d["text"] = Text;
            else d["hex"] = Hex;
            if (Repeat != 1) d["repeat"] = Repeat;
            return J(d);
        }
    }

    // ------------------------------------------------------------------ envelopes
    /// An envelope as tests/AiPet.Tests' Events.Envelope makes it: the hook's fields, with `env` for Claude.
    static JsonObject Envelope(double at, JsonObject payload, JsonObject env = null, long pid = 4242) => new()
    {
        [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = "claude", [Ipc.At] = at, [Ipc.Pid] = pid,
        [Ipc.Env] = env ?? new JsonObject(), [Ipc.Payload] = payload, [Ipc.Sent] = at + 0.01,
    };

    static JsonObject Payload(string sid, string ev, JsonObject extra = null)
    {
        var p = new JsonObject { ["hook_event_name"] = ev, ["session_id"] = sid };
        if (extra != null)
            foreach (var (k, v) in extra) p[k] = v?.DeepClone();
        return p;
    }

    static JsonObject Bash(string command, string toolUseId = null, string description = null)
    {
        var input = new JsonObject { ["command"] = command };
        if (description != null) input["description"] = description;
        var o = new JsonObject { ["tool_name"] = "Bash", ["tool_input"] = input };
        if (toolUseId != null) o["tool_use_id"] = toolUseId;
        return o;
    }

    static JsonObject Obj(params (string Key, JsonNode Value)[] fields)
    {
        var o = new JsonObject();
        foreach (var (k, v) in fields) o[k] = v;
        return o;
    }

    static JsonNode Parse(string json) => json == null ? null : JsonNode.Parse(json);

    static JsonObject Tool(string tool, string input) => Obj(("tool_name", tool), ("tool_input", Parse(input)));

    static string Sid(int n) => $"00000000-0000-4000-8000-{n:D12}";

    /// A transcript line naming the chat, as the Claude app writes it.
    static string TitleLine(string title, string sid = "s") =>
        "{\"type\":\"custom-title\",\"customTitle\":" + J(title) + ",\"sessionId\":" + J(sid) + "}\n";

    static string UserLine(string text) => "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":" + J(text) + "}}\n";

    // ------------------------------------------------------------------ tests/AiPet.Tests/OrderingTests.cs
    /// ClaudeOrderingTests, a case each: the same envelopes at the same times.
    static void Ordering(Func<string, int, bool, Replay> newCase)
    {
        Replay Case(string name) => newCase("ordering/" + name, 1, false);
        string sid = Sid(1);
        {
            var r = Case("PreToolUse_AfterTheStopItPreceded_IsStale");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0, Obj(("prompt", "fix it")));
            r.Claude(sid, "Stop", t0 + 2);
            r.Claude(sid, "PreToolUse", t0 + 1, Bash("ls"));
            r.Claude(sid, "PostToolUse", t0 + 1.5, Bash("ls"));
        }
        {
            var r = Case("SameTick_GoesByRank");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PermissionRequest", t0 + 1);
            r.Claude(sid, "PreToolUse", t0 + 1, Bash("ls"));
            r.Claude(sid, "PreToolUse", t0 + 1.0004, Bash("ls"));
            r.Claude(sid, "Stop", t0 + 1);
        }
        {
            var r = Case("SameTick_InTurnOrder_IsTaken");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PreToolUse", t0 + 1, Bash("ls"));
            r.Claude(sid, "PermissionRequest", t0 + 1);
        }
        {
            var r = Case("SessionEnd_ThenALateEvent_DoesNotBringTheChatBack");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "SessionEnd", t0 + 2);
            r.Claude(sid, "PreToolUse", t0 + 1, Bash("ls"));
            r.Claude(sid, "Stop", t0 + 2);
            r.Claude(sid, "SessionEnd", t0 + 1.5);
            r.Claude(sid, "SessionStart", t0 + 3);
        }
        {
            var r = Case("ClockSetBack_TakesTheNextEvent");
            double t0 = r.T0;
            r.Claude(sid, "Stop", t0 + 3600);
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PreToolUse", t0 - 1, Bash("ls"));
        }
        {
            var r = Case("ClockSetBack_AfterSessionEnd_TakesTheChatUpAgain");
            double t0 = r.T0;
            r.Claude(sid, "SessionEnd", t0 + 3600);
            r.Claude(sid, "SessionStart", t0);
        }
        {
            var r = Case("PermissionRequest_StartedJustBeforeItsPreToolUse_ArrivingAfterIt_IsTaken");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PreToolUse", t0 + 1.003, Bash("rm -rf build", "toolu_1"));
            r.Claude(sid, "PermissionRequest", t0 + 1.001, Bash("rm -rf build", description: "clean"));
        }
        {
            var r = Case("PreToolUse_StartedJustAfterItsPermissionRequest_ArrivingAfterIt_IsStale");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PermissionRequest", t0 + 1.001, Bash("rm -rf build", description: "clean"));
            r.Claude(sid, "PreToolUse", t0 + 1.003, Bash("rm -rf build", "toolu_1"));
            r.Claude(sid, "PostToolUse", t0 + 1.4, Bash("rm -rf build", "toolu_1"));
            r.Claude(sid, "PreToolUse", t0 + 1.5, Bash("rm -rf build", "toolu_2"));
        }
        {
            var r = Case("SameCommandAgain_AfterAPairedRequest_IsTaken");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PreToolUse", t0 + 1, Bash("npm test", "toolu_1"));
            r.Claude(sid, "PermissionRequest", t0 + 1.001, Bash("npm test"));
            r.Claude(sid, "PostToolUse", t0 + 1.4, Bash("npm test", "toolu_1"));
            r.Claude(sid, "PreToolUse", t0 + 1.5, Bash("npm test", "toolu_2"));
        }
        {
            var r = Case("PermissionRequest_BehindAnotherCallsPreToolUse_GoesByAt");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PreToolUse", t0 + 1.003, Bash("rm -rf build", "toolu_1"));
            r.Claude(sid, "PreToolUse", t0 + 1.010, Bash("ls", "toolu_2"));
            r.Claude(sid, "PermissionRequest", t0 + 1.001, Bash("rm -rf build"));
        }
        {
            var r = Case("PreToolUse_OfAnotherCommand_OrLongAfter_ARequest_IsTaken");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PermissionRequest", t0 + 1, Bash("rm -rf build"));
            r.Claude(sid, "PreToolUse", t0 + 1.002, Bash("ls", "toolu_1"));
            r.Claude(sid, "PermissionRequest", t0 + 3, Bash("make"));
            r.Claude(sid, "PreToolUse", t0 + 4.5, Bash("make", "toolu_2"));
        }
        {
            var r = Case("PairedRequest_DoesNotMoveTheLatestBack");
            double t0 = r.T0;
            r.Claude(sid, "UserPromptSubmit", t0);
            r.Claude(sid, "PreToolUse", t0 + 1.000, Bash("ls", "toolu_a"));
            r.Claude(sid, "PreToolUse", t0 + 1.004, Bash("rm -rf build", "toolu_b"));
            r.Claude(sid, "PermissionRequest", t0 + 1.001, Bash("rm -rf build"));
            r.Claude(sid, "PostToolUse", t0 + 1.0025, Bash("ls", "toolu_a"));
            r.Claude(sid, "PostToolUse", t0 + 1.5, Bash("rm -rf build", "toolu_b"));
        }
        {
            var r = Case("UnknownEvent_IsIgnored_AndMakesNoChat");
            r.Claude(sid, "SomethingNew", r.T0);
        }
    }

    // ------------------------------------------------------------------ the event → state table
    /// One chat through every Claude event, notification type, sub-agent kind and StopFailure error.
    static void Events(Replay r)
    {
        string sid = Sid(2);
        double t = r.T0 - 100;
        string Ev(string ev, JsonObject extra = null) => r.Claude(sid, ev, t += 0.01, extra);
        JsonObject Note(string type, string message = null)
        {
            var o = new JsonObject();
            if (type != null) o["notification_type"] = type;
            if (message != null) o["message"] = message;
            return o;
        }

        Ev("SessionStart", Obj(("source", "startup")));
        Ev("UserPromptSubmit", Obj(("prompt", "  fix   the\tbuild  \r\nand more")));
        Ev("UserPromptSubmit", Obj(("prompt", "/compact")));
        Ev("UserPromptSubmit", Obj(("prompt", "")));
        Ev("UserPromptSubmit");
        Ev("UserPromptSubmit", Obj(("prompt", 42)));
        Ev("PreToolUse", Bash("make"));
        Ev("PostToolUseFailure", Bash("make"));
        Ev("PermissionDenied", Bash("rm -rf /"));
        Ev("Notification", Note("idle_prompt", "Claude is waiting for your input"));
        Ev("Notification", Note("permission_prompt", "Claude needs your permission to use Bash"));
        foreach (var type in new[] { "agent_needs_input", "elicitation_dialog", "elicitation_url_dialog" }) Ev("Notification", Note(type));
        foreach (var type in new[] { "auth_success", "elicitation_complete", "elicitation_response", "quota_auto_resume_fired",
                                     "quota_auto_resume_stale", "quota_auto_resume_disabled" })
            Ev("Notification", Note(type, "Claude needs your permission"));
        Ev("Notification", Note("agent_completed"));
        Ev("Notification", Note("idle_prompt"));
        Ev("Notification", Note(null, "Claude is waiting for your input"));
        Ev("Elicitation", Obj(("mcp_server_name", "atlassian")));
        Ev("Notification", Note(null, "Claude is WAITING FOR YOUR INPUT"));
        Ev("Notification", Note(null, "Claude needs your permission to use Bash"));
        Ev("Notification", Note(null, "PERMISSION wanted"));
        Ev("Notification", Note(null, ""));
        Ev("Notification", Note(null, null));
        Ev("Notification", Note(null, "the build server reported that something unusual happened while deploying"));
        Ev("Notification", Note(null, "   "));
        Ev("Notification", Note("something_new", "hello there"));
        Ev("Notification", Obj(("notification_type", 7), ("message", "a typed number")));
        Ev("ElicitationResult");
        Ev("PreToolUse", Tool("Read", "{\"file_path\":\"/home/u/a.txt\"}"));
        Ev("SubagentStop");
        Ev("PreCompact", Obj(("trigger", "auto")));
        Ev("PostCompact", Obj(("trigger", "auto")));
        Ev("PostCompact", Obj(("trigger", "manual")));
        Ev("PostCompact");
        foreach (var kind in new JsonNode[] { null, "general-purpose", "default", "Explore", "  ", 5, "" })
            Ev("SubagentStart", kind == null ? null : Obj(("agent_type", kind)));
        Ev("SessionStart", Obj(("source", "compact")));
        Ev("Stop");
        Ev("SessionStart", Obj(("source", "resume")));
        foreach (var error in new JsonNode[] { "rate_limit", "overloaded", "server_error", "authentication_failed",
                                               "oauth_org_not_allowed", "billing_error", "account_on_hold", "max_output_tokens",
                                               "model_not_found", "invalid_request", "cloud_credential_error", "weird", null, 3 })
            Ev("StopFailure", error == null ? null : Obj(("error_type", error)));
        Ev("Stop");
        Ev("Notification", Note("idle_prompt"));
        Ev("SessionEnd", Obj(("reason", "prompt_input_exit")));
        Ev("SessionEnd");
        Ev("SessionStart");
    }

    // ------------------------------------------------------------------ describing tool calls
    static readonly (string Tool, string Input)[] Tools =
    {
        ("Bash", "{\"command\":\"ls -la\"}"), ("PowerShell", "{\"command\":\"dir\"}"), ("shell", "{\"command\":[\"ls\"]}"),
        ("local_shell", null), ("exec_command", "{\"cmd\":\"make\"}"),
        ("Edit", "{\"file_path\":\"/home/u/src/main.rs\"}"), ("MultiEdit", "{\"file_path\":\"src/lib.rs\"}"),
        ("Write", "{\"file_path\":\"notes.md\"}"), ("NotebookEdit", "{\"notebook_path\":\"/home/u/nb/analysis.ipynb\"}"),
        ("Edit", null), ("Edit", "{\"file_path\":\"/\"}"), ("Edit", "{\"file_path\":\"dir/sub/\"}"), ("Edit", "{\"file_path\":\"\"}"),
        ("Edit", "{\"file_path\":42,\"notebook_path\":\"/n/x.ipynb\"}"), ("Edit", "{\"file_path\":\"///\"}"),
        ("Write", "{\"file_path\":\"/home/u/π/ünïcode.txt\"}"), ("Write", "{\"file_path\":\"/home/u/a b/c d.txt\"}"),
        ("apply_patch", "{\"command\":\"*** Begin Patch\\n*** Update File: a.rs\\n*** End Patch\"}"),
        ("Read", "{\"file_path\":\"/home/u/README.md\"}"), ("Read", "{}"), ("Read", "\"README.md\""), ("Read", "[1,2]"),
        ("Grep", "{\"pattern\":\"TODO\"}"), ("Glob", null), ("ToolSearch", null),
        ("WebFetch", null), ("WebSearch", null), ("web_search", null),
        ("Agent", null), ("Task", null), ("Workflow", null),
        ("TodoWrite", null), ("EnterPlanMode", null), ("update_plan", null), ("AskUserQuestion", null), ("ExitPlanMode", null),
        ("Skill", "{\"skill\":\"flow-next:plan\"}"), ("Skill", "{\"skill\":\"review\"}"), ("Skill", null),
        ("Skill", "{\"skill\":\"a:b:\"}"), ("Skill", "{\"skill\":7}"),
        ("mcp__claude-in-chrome__navigate", null), ("mcp__Claude_Browser__click", null), ("mcp__chrome-devtools__snap", null),
        ("mcp__atlassian__search", null), ("mcp__my_server__do", null), ("mcp__exactly_twenty_chars__x", null),
        ("mcp__twenty_one_characters__x", null), ("mcp__😀😀😀😀😀😀😀😀😀😀__x", null), ("mcp__😀😀😀😀😀😀😀😀😀😀😀__x", null),
        ("mcp__ééééééééééééééééééééé__x", null), ("mcp__", null), ("mcp____x", null), ("mcp__browser__x", null), ("mcp__x", null),
        ("MCP__x__y", null), ("FooTool", null), ("", null), ("Bash", "null"),
    };

    /// File paths whose name Path.GetFileName finds by the OS's rules.
    static readonly (string Tool, string Input)[] PathTools =
        new[]
        {
            @"C:\src\main.rs", @"C:main.rs", @"C:\", @"C:", @"\\server\share", @"\\server\share\x.txt", @"\\?\C:\x\y.txt",
            @"\\.\pipe\name", @"a\b/c\d.txt", "C:/a/b.txt", @"dir\", @"\\?\UNC\server\share\f.txt", @"\\?\UNC\server\share",
            @"\\.\", "   ", " x", ":", "1:\\x", @"\root.txt", "a:b", @"\\?\", @"\\server",
        }.Select(p => ("Edit", J(new Dictionary<string, string> { ["file_path"] = p }))).ToArray();

    /// A PreToolUse for each tool; then one without a tool_name and one with a number for it.
    static void Describe(Replay r, (string Tool, string Input)[] tools)
    {
        string sid = Sid(3);
        double t = r.T0 - 100;
        foreach (var (tool, input) in tools) r.Claude(sid, "PreToolUse", t += 0.01, Tool(tool, input));
        if (tools != Tools) return;
        r.Claude(sid, "PreToolUse", t += 0.01, Obj(("tool_input", Parse("{\"command\":\"ls\"}"))));
        r.Claude(sid, "PreToolUse", t += 0.01, Obj(("tool_name", 12)));
    }

    // ------------------------------------------------------------------ text
    /// Short(first line, 38) through the title a prompt gives a chat of its own.
    static void Short(Replay r)
    {
        var prompts = new[]
        {
            "hello world", "  lots   of\t\tspace  ", "first line\nsecond", "\nsecond line", "é is first", "ß straße", "ı dotless",
            "ǆ digraph", "ǅ title case", "ᾀ greek", "ᾈ greek", "ﬀ ligature", "ſ long s", "µ micro", "ÿ", "ა georgian",
            "ꭰ cherokee", "𐐨 deseret", "𝒶 math", "😀 smile", new string('a', 38), new string('a', 39),
            new string('a', 36) + " bcd", new string('a', 35) + " bcd", new string('a', 36) + "😀tail", new string('a', 35) + "😀tail",
            "\u00a0nbsp\u3000ideographic\u2028line\u2029para\u0085nel\u000bvt\u000cff\u001cfs", "\u180e mongolian", "\u200b zero width",
            "/compact now", " /spaced", "a", "1st", "e\u0301 accent", "tab\u0001x", "\u2003\u2003em", "\ufeffbom first",
        };
        double t = r.T0 - 100;
        for (int i = 0; i < prompts.Length; i++) r.Claude($"short-{i:D2}", "UserPromptSubmit", t += 0.01, Obj(("prompt", prompts[i])));
    }

    /// An untyped Notification's message, matched without case (and Short(message, 40)), each on a chat of its own.
    static void Notifications(Replay r)
    {
        var messages = new[]
        {
            "Waiting for your input", "waiting for your ınput", "waiting for your İnput", "Needs your PERMİSSION",
            "needs your permiſſion", "needs your permıssion", "needs your ｐermission", "Waiting  for your input",
            "a message of exactly forty characters!!!", "a message of exactly forty characters!!!!", new string('m', 38) + "😀tail",
        };
        double t = r.T0 - 100;
        for (int i = 0; i < messages.Length; i++)
            r.Claude($"note-{i:D2}", "Notification", t += 0.01, Obj(("message", messages[i])));
    }

    // ------------------------------------------------------------------ where and host ids
    static void WhereAndHost(Replay r, Replay changes)
    {
        const string guid = "0f8fad5b-d9cb-469f-a165-70867728950e";
        var entrypoints = new JsonNode[] { "cli", "claude-desktop", "claude-vscode", "sdk-ts", "", null, 7, "a\tb", new string('e', 70) };
        var hosts = new[]
        {
            "local_" + guid, "local_" + guid.ToUpperInvariant(), "local_{" + guid + "}", "LOCAL_" + guid, "remote" + guid,
            "local_" + guid.Replace("-", ""), "local_" + guid[..^1], "local_" + guid + "0", "local_" + guid[..^1] + "g",
            "local_+f8fad5b-d9cb-469f-a165-70867728950e", "local_0xf8fad5-d9cb-469f-a165-70867728950e",
            "local_0Xf8fad5-d9cb-469f-a165-70867728950e", "local_+0x8fad5-d9cb-469f-a165-70867728950e",
            "local_0x+8fad5-d9cb-469f-a165-70867728950e", "local_0f8fad5b-+9cb-0x9f-+0xa-70867728950e",
            "local_0f8fad5b-d9cb-469f-a165-+086772895xe", "local_0f8fad5b-d9cb-469f-a165-0x8677289+0e",
            "local_0f8fad5b-d9cb-469f-a165-7086772895+e", "local_ f8fad5b-d9cb-469f-a165-70867728950e",
            "local_0f8fad5b d9cb-469f-a165-70867728950e", "local_0f8fad5bd-9cb-469f-a165-70867728950e",
            "local_++f8fad5-d9cb-469f-a165-70867728950e", "local_0x0x8fad-d9cb-469f-a165-70867728950e",
            "local_0f8fad5b-d9cb-469f-a165-7086772895é", "local_0f8fad5b-d9cb-469f-a165-7086772895😀", "", " local_" + guid,
        };
        double t = r.T0 - 100;
        int n = 0;
        foreach (var e in entrypoints)
            r.Claude($"where-{n++:D2}", "SessionStart", t += 0.01, env: e == null ? new JsonObject() : Obj(("CLAUDE_CODE_ENTRYPOINT", e)));
        foreach (var h in hosts)
            r.Claude($"host-{n++:D2}", "SessionStart", t += 0.01, env: Obj(("CLAUDE_CODE_ENTRYPOINT", "claude-desktop"), ("CLAUDE_CODE_HOST_SESSION_ID", h)));
        r.Claude($"host-{n++:D2}", "SessionStart", t += 0.01, env: Obj(("CLAUDE_CODE_HOST_SESSION_ID", 12)));
        // env that isn't an object
        r.Apply(Envelope(t += 0.01, Payload($"where-{n++:D2}", "SessionStart"), null).Also(o => o[Ipc.Env] = new JsonArray("cli")));

        // where follows each taken event; a host id stays until another valid one comes
        string sid = Sid(4);
        t = changes.T0 - 100;
        changes.Claude(sid, "SessionStart", t += 0.01, env: Obj(("CLAUDE_CODE_ENTRYPOINT", "claude-desktop"), ("CLAUDE_CODE_HOST_SESSION_ID", "local_" + guid)));
        changes.Claude(sid, "UserPromptSubmit", t += 0.01, Obj(("prompt", "go")));
        changes.Claude(sid, "PreToolUse", t += 0.01, Bash("ls"), Obj(("CLAUDE_CODE_ENTRYPOINT", "cli"), ("CLAUDE_CODE_HOST_SESSION_ID", "nope")));
        changes.Claude(sid, "PostToolUse", t - 1, Bash("ls"), Obj(("CLAUDE_CODE_ENTRYPOINT", "claude-vscode")));
        changes.Claude(sid, "SomethingNew", t += 0.01, env: Obj(("CLAUDE_CODE_ENTRYPOINT", "sdk-py")));
        changes.Claude(sid, "Stop", t += 0.01, env: Obj(("CLAUDE_CODE_HOST_SESSION_ID", "local_" + Guid.Empty)));
        changes.Claude(sid, "SessionStart", t += 0.01, Obj(("cwd", "/home/u/p")));
        changes.Claude(sid, "PreToolUse", t += 0.01, Bash("ls").Also(o => o["cwd"] = ""));
        changes.Claude(sid, "PostToolUse", t += 0.01, Bash("ls").Also(o => o["cwd"] = 42));
        changes.Claude(sid, "Stop", t += 0.01, Obj(("cwd", "/other")));
        changes.Claude(sid, "SessionEnd", t += 0.01, env: Obj(("CLAUDE_CODE_ENTRYPOINT", "cli")));
        changes.Claude(sid, "SessionStart", t += 0.01);
    }

    // ------------------------------------------------------------------ titles from transcripts
    /// ChatTitle over transcripts of every shape, each read by a SessionStart of a chat of its own; and paths with no
    /// file to read.
    static void Titles(Replay r)
    {
        const int Window = 512 * 1024;
        string filler = UserLine(new string('x', 1000));   // 1034 bytes
        var bigFiller = Part.T(filler, 600);
        var files = new List<(string Name, Part[] Parts)>
        {
            ("one", new[] { Part.T(UserLine("hi") + TitleLine("my chat")) }),
            ("last-wins", new[] { Part.T(TitleLine("first") + UserLine("more") + TitleLine("second") + UserLine("after")) }),
            ("last-malformed", new[] { Part.T(TitleLine("good one") + "{\"type\":\"custom-title\",\"customTitle\":\"broken") }),
            ("not-a-string", new[] { Part.T(TitleLine("good") + "{\"type\":\"custom-title\",\"customTitle\":42}\n"
                + "{\"type\":\"custom-title\",\"customTitle\":null}\n{\"type\":\"custom-title\",\"customTitle\":{\"a\":1}}\n") }),
            ("no-key", new[] { Part.T(TitleLine("earlier") + "{\"type\":\"custom-title\",\"sessionId\":\"s\"}\n") }),
            ("empty-last", new[] { Part.T(TitleLine("earlier") + TitleLine("")) }),
            ("blank-last", new[] { Part.T(TitleLine("earlier") + TitleLine(" \t ")) }),
            ("long", new[] { Part.T(TitleLine("a rather long name for a chat that goes on and on and on")) }),
            ("escapes", new[] { Part.T("{\"type\":\"custom-title\",\"customTitle\":\"tab\\there\\nnew line \\u00e9 \\\"q\\\"\"}\n") }),
            ("crlf", new[] { Part.T(TitleLine("crlf name").Replace("\n", "\r\n") + UserLine("x").Replace("\n", "\r\n")) }),
            ("no-newline", new[] { Part.T(UserLine("x") + TitleLine("at the end").TrimEnd('\n')) }),
            ("outside-window", new[] { Part.T(TitleLine("too early")), bigFiller }),
            ("inside-window", new[] { bigFiller, Part.T(TitleLine("late enough")), Part.T(filler, 100) }),
            ("cut-line", new[] { Part.T(TitleLine("old")), bigFiller, Part.T(TitleLine("cut")), Part.T("y", Window - TitleLine("cut").Length + 1) }),
            ("exact-window", new[] { Part.T(TitleLine("whole")), Part.T("z", Window - TitleLine("whole").Length) }),
            ("window-plus-one", new[] { Part.T(TitleLine("whole")), Part.T("z", Window - TitleLine("whole").Length + 1) }),
            ("utf8-cut", new[] { Part.T("é", 400000), Part.T("\n" + TitleLine("after the cut")) }),
            ("bom", new[] { Part.H(new byte[] { 0xEF, 0xBB, 0xBF }), Part.T(TitleLine("with a bom")) }),
            ("bom-inner", new[] { Part.T(TitleLine("before") + "\ufeff" + TitleLine("inner bom")) }),
            ("utf16le", new[] { Part.H(new byte[] { 0xFF, 0xFE }.Concat(Encoding.Unicode.GetBytes(UserLine("x") + TitleLine("utf-16 le"))).ToArray()) }),
            ("utf16be", new[] { Part.H(new byte[] { 0xFE, 0xFF }.Concat(Encoding.BigEndianUnicode.GetBytes(TitleLine("utf-16 be"))).ToArray()) }),
            ("utf32le", new[] { Part.H(new byte[] { 0xFF, 0xFE, 0, 0 }.Concat(Encoding.UTF32.GetBytes(TitleLine("utf-32 le"))).ToArray()) }),
            ("invalid-utf8", new[] { Part.T("{\"type\":\"custom-title\",\"customTitle\":\"a"), Part.H(new byte[] { 0xFF, 0xC3 }), Part.T("b\"}\n") }),
            ("depth-64", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"deep 64\",\"x\":"
                + new string('[', 63) + new string(']', 63) + "}\n") }),
            ("depth-65", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"deep 65\",\"x\":"
                + new string('[', 64) + new string(']', 64) + "}\n") }),
            ("depth-in-string", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"brackets "
                + new string('[', 70) + "\",\"x\":\"\\\"{{{{\"}\n") }),
            ("lone-surrogate-title", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"\\ud800x\"}\n") }),
            ("lone-surrogate-other", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"fine\",\"other\":\"\\udc00\"}\n") }),
            ("duplicate-keys", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"one\",\"customTitle\":\"two\"}\n") }),
            ("duplicate-other", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"type\":\"x\",\"customTitle\":\"dup type\"}\n") }),
            ("duplicate-nested", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"dup nested\",\"o\":{\"a\":1,\"a\":2}}\n") }),
            ("duplicate-escaped", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"one\",\"custom\\u0054itle\":\"two\"}\n") }),
            ("array-line", new[] { Part.T(TitleLine("fallback") + "[\"custom-title\",{\"customTitle\":\"in an array\"}]\n") }),
            ("string-line", new[] { Part.T(TitleLine("fallback") + "\"custom-title\"\n") }),
            ("null-line", new[] { Part.T(TitleLine("fallback") + "null \"custom-title\"\n") }),
            ("mention-only", new[] { Part.T(TitleLine("fallback") + UserLine("rename it with \"custom-title\" please")) }),
            ("nested", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"data\":{\"customTitle\":\"nested\"}}\n") }),
            ("key-case", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customtitle\":\"lower\"}\n") }),
            ("nul-char", new[] { Part.T(TitleLine("a\u0000b")) }),
            ("surrogate-cut", new[] { Part.T(TitleLine(new string('a', 42) + "😀 and more")) }),
            ("big-number", new[] { Part.T("{\"type\":\"custom-title\",\"customTitle\":\"big number\",\"n\":1e400,\"m\":-0,\"k\":1.50}\n") }),
            ("trailing-garbage", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"garbage\"} x\n") }),
            ("comment", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"comment\"} // c\n") }),
            ("trailing-comma", new[] { Part.T(TitleLine("fallback") + "{\"type\":\"custom-title\",\"customTitle\":\"comma\",}\n") }),
            ("leading-space", new[] { Part.T("  \t" + TitleLine("indented")) }),
            ("empty", Array.Empty<Part>()),
            ("no-title", new[] { Part.T(UserLine("nothing to see")) }),
        };
        double t = r.T0 - 100;
        foreach (var (name, parts) in files)
        {
            r.Write(name + ".jsonl", parts);
            r.Claude("title-" + name, "SessionStart", t += 0.01, Obj(("transcript_path", Files + "/" + name + ".jsonl")));
        }
        // no file to read
        var paths = new JsonNode[] { "", Files + "/missing.jsonl", Files, Files + "/a\u0000b.jsonl", 5 };
        for (int i = 0; i < paths.Length; i++)
            r.Claude($"title-path-{i}", "SessionStart", t += 0.01, Obj(("transcript_path", paths[i]?.DeepClone())));
        r.Claude("title-path-none", "SessionStart", t += 0.01);
    }

    /// When a Claude event reads the chat's title: always on SessionStart, UserPromptSubmit, Stop, Notification and
    /// PermissionRequest, else only while the chat has none; never for an ignored event, nor into a stale one.
    static void TitleReads(Replay r)
    {
        string sid = Sid(5), file = "t.jsonl", path = Files + "/" + file;
        double t = r.T0 - 100;
        string Ev(string ev, double at, JsonObject extra = null)
        {
            var o = extra ?? new JsonObject();
            o["transcript_path"] = path;
            return r.Claude(sid, ev, at, o);
        }
        void Name(string title) => r.Write(file, Part.T(UserLine("x") + TitleLine(title)));

        Name("first name");
        Ev("PreToolUse", t += 0.01, Bash("ls"));
        Name("second name");
        Ev("PreToolUse", t += 0.01, Bash("pwd"));
        Ev("PostToolUse", t += 0.01, Bash("pwd"));
        Ev("Notification", t += 0.01, Obj(("notification_type", "idle_prompt")));
        Name("third");
        Ev("PermissionRequest", t += 0.01, Bash("make"));
        Name("fourth");
        Ev("Stop", t += 0.01);
        Name("fifth");
        Ev("UserPromptSubmit", t += 0.01, Obj(("prompt", "go")));
        Name("sixth");
        Ev("SessionStart", t += 0.01);
        r.Delete(file);
        Ev("Stop", t += 0.01);
        r.Write(file, Part.T(UserLine("no title")));
        Ev("SessionStart", t += 0.01);
        Name("seventh");
        Ev("Stop", t - 5);
        Ev("SomethingNew", t += 0.01);
        Ev("PreToolUse", t += 0.01, Bash("x"));
        Ev("Stop", t += 0.01);
        Name("eighth");
        Ev("SessionEnd", t += 0.01);
        Ev("PreToolUse", t += 0.01, Bash("after the end"));

        // a chat not heard of for a day: peeking finds nothing (it's about to be pruned), so the title is read
        string old = Sid(6), oldFile = "old.jsonl";
        r.Write(oldFile, Part.T(TitleLine("old name")));
        r.Claude(old, "SessionStart", r.T0 - 90000, Obj(("transcript_path", Files + "/" + oldFile)));
        r.Write(oldFile, Part.T(TitleLine("renewed")));
        r.Claude(old, "PreToolUse", t += 0.01, Bash("ls").Also(o => o["transcript_path"] = Files + "/" + oldFile));
        // a chat without a transcript keeps the name it has
        r.Claude(old, "Stop", t += 0.01);
    }

    // ------------------------------------------------------------------ the store
    /// Chats not heard of for a day go, tombstones too; a chat keeps its place when it ends or is taken up again.
    static void Prune(Replay r)
    {
        double t0 = r.T0;
        r.Claude("a", "SessionStart", t0 - 90000);
        r.Claude("b", "SessionStart", t0 - 100);
        r.Claude("c", "SessionEnd", t0 - 90000);
        r.Claude("d", "SessionEnd", t0 - 100);
        r.Claude("e", "SessionStart", t0 - 50);
        // an ignored event prunes too
        r.Claude("f", "SomethingNew", t0 - 49);
        r.Claude("a", "SessionStart", t0 - 48);
        r.Claude("b", "SessionEnd", t0 - 47);
        r.Claude("b", "SessionStart", t0 - 46);
        r.Claude("d", "UserPromptSubmit", t0 - 45, Obj(("prompt", "back again")));
        r.Claude("e", "Stop", t0 - 86400 - 60);
        r.Claude("g", "SessionStart", t0 - 44);
    }

    // ------------------------------------------------------------------ ordering
    static void Pairing(Func<string, int, bool, Replay> newCase)
    {
        string sid = Sid(7);
        {
            // 17 unpaired halves: the first is dropped, so its PreToolUse no longer pairs
            var r = newCase("claude/pairing-sixteen-halves", 1, false);
            double b = r.T0 - 100;
            r.Claude(sid, "UserPromptSubmit", b);
            for (int i = 0; i < 17; i++) r.Claude(sid, "PermissionRequest", b + 1 + i * 0.001, Bash($"cmd {i}"));
            r.Claude(sid, "PreToolUse", b + 1.5, Bash("cmd 0", "toolu_0"));
            r.Claude(sid, "PreToolUse", b + 1.6, Bash("cmd 1", "toolu_1"));
        }
        {
            // the window: a second apart pairs, a little more doesn't
            var r = newCase("claude/pairing-window", 1, false);
            double b = r.T0 - 100;
            r.Claude(sid, "UserPromptSubmit", b);
            r.Claude(sid, "PermissionRequest", b + 1, Bash("one"));
            r.Claude(sid, "PreToolUse", b + 2, Bash("one", "toolu_1"));
            r.Claude(sid, "PermissionRequest", b + 3, Bash("two"));
            r.Claude(sid, "PreToolUse", b + 4.001, Bash("two", "toolu_2"));
            r.Claude(sid, "PreToolUse", b + 5, Bash("three", "toolu_3"));
            r.Claude(sid, "PermissionRequest", b + 4, Bash("three"));
            r.Claude(sid, "PreToolUse", b + 6.5, Bash("four", "toolu_4"));
            r.Claude(sid, "PermissionRequest", b + 5.4995, Bash("four"));
        }
        {
            // a request pairs with the closest PreToolUse of its call; each half once
            var r = newCase("claude/pairing-closest", 1, false);
            double b = r.T0 - 100;
            r.Claude(sid, "UserPromptSubmit", b);
            r.Claude(sid, "PreToolUse", b + 1, Bash("npm test", "toolu_1"));
            r.Claude(sid, "PreToolUse", b + 1.5, Bash("npm test", "toolu_2"));
            r.Claude(sid, "PermissionRequest", b + 1.4, Bash("npm test"));
            r.Claude(sid, "PermissionRequest", b + 1.45, Bash("npm test"));
            r.Claude(sid, "PreToolUse", b + 1.52, Bash("npm test", "toolu_2"));
            r.Claude(sid, "PostToolUse", b + 2, Bash("npm test", "toolu_2"));
            r.Claude(sid, "PreToolUse", b + 2.1, Bash("npm test", "toolu_3"));
            r.Claude(sid, "PermissionRequest", b + 2.099, Bash("npm test", description: "again"));
        }
        {
            // a paired request whose PreToolUse is the latest by time but not by rank goes by at
            var r = newCase("claude/pairing-rank", 1, false);
            double b = r.T0 - 100;
            r.Claude(sid, "UserPromptSubmit", b);
            r.Claude(sid, "PreToolUse", b + 1, Bash("make", "toolu_1"));
            r.Claude(sid, "PostToolUse", b + 1, Bash("ls", "toolu_0"));
            r.Claude(sid, "PermissionRequest", b + 0.999, Bash("make"));
            r.Claude(sid, "PreToolUse", b + 2, Bash("make", "toolu_2"));
            r.Claude(sid, "PermissionRequest", b + 2, Bash("make"));
        }
        {
            // tools keyed by their file, or their whole input; a duplicate PreToolUse; a new chat that starts with a request
            var r = newCase("claude/pairing-keys", 1, false);
            double b = r.T0 - 100;
            r.Claude(sid, "PermissionRequest", b + 1, Tool("Edit", "{\"file_path\":\"/a/b.rs\",\"old_string\":\"x\"}"));
            r.Claude(sid, "PreToolUse", b + 1.002, Tool("Edit", "{\"file_path\":\"/a/b.rs\",\"old_string\":\"y\"}").Also(o => o["tool_use_id"] = "t1"));
            r.Claude(sid, "PreToolUse", b + 2, Tool("WebFetch", "{\"url\":\"https://example.com\",\"prompt\":\"read\"}"));
            r.Claude(sid, "PermissionRequest", b + 1.999, Tool("WebFetch", "{\"url\":\"https://example.com\",\"prompt\":\"read\"}"));
            r.Claude(sid, "PreToolUse", b + 3, Tool("WebFetch", "{\"url\":\"https://example.com\",\"prompt\":\"other\"}"));
            r.Claude(sid, "PermissionRequest", b + 2.999, Tool("WebFetch", "{\"url\":\"https://example.com\",\"prompt\":\"read\"}"));
            r.Claude(sid, "PreToolUse", b + 4, Tool("NotebookEdit", "{\"notebook_path\":\"/n.ipynb\"}"));
            r.Claude(sid, "PermissionRequest", b + 3.999, Tool("NotebookEdit", "{\"notebook_path\":\"/n.ipynb\",\"cell\":2}"));
            r.Claude(sid, "PreToolUse", b + 5, Tool("Bash", "{\"command\":null,\"file_path\":\"/f\"}"));
            r.Claude(sid, "PermissionRequest", b + 4.999, Tool("Bash", "{\"file_path\":\"/f\"}"));
            r.Claude(sid, "PreToolUse", b + 6, Tool("Bash", "{\"command\":7,\"file_path\":\"/g\"}"));
            r.Claude(sid, "PermissionRequest", b + 5.999, Tool("Bash", "{\"file_path\":\"/g\"}"));
            // a command that reads as another call's whole input
            r.Claude(sid, "PreToolUse", b + 7, Tool("Bash", "{\"command\":\"{\\\"x\\\":1}\"}"));
            r.Claude(sid, "PermissionRequest", b + 6.999, Tool("Bash", "{\"x\":1}"));
            r.Claude(sid, "PreToolUse", b + 8, Tool("Grep", null));
            r.Claude(sid, "PermissionRequest", b + 7.999, Obj(("tool_name", "Grep")));
            r.Claude(sid, "PreToolUse", b + 9, Tool("Grep", "{}").Also(o => o.Remove("tool_name")));
            r.Claude(sid, "PermissionRequest", b + 8.999, Obj(("tool_input", Parse("{}"))));
            r.Claude(sid, "PreToolUse", b + 10, Bash("dup", "toolu_d"));
            r.Claude(sid, "PreToolUse", b + 10, Bash("dup", "toolu_d"));
            r.Claude(sid, "SessionEnd", b + 11);
            r.Claude(sid, "PermissionRequest", b + 12, Bash("after"));
            r.Claude(sid, "PreToolUse", b + 12.001, Bash("after", "toolu_e"));
        }
        {
            // a whole input is the call as System.Text.Json writes it (CallKey's ToJsonString): its keys in their
            // order, its numbers as spelled, its strings escaped as its encoder escapes them, whatever the line did.
            // Each request comes before its PreToolUse: a PreToolUse that pairs with it is stale, one that doesn't
            // works.
            var r = newCase("claude/pairing-whole-inputs", 1, false);
            double b = r.T0 - 100;
            r.Claude(sid, "UserPromptSubmit", b);
            void Both(string tool, string request, string pre)
            {
                b += 2;
                r.Claude(sid, "PermissionRequest", b, Tool(tool, request));
                r.Claude(sid, "PreToolUse", b + 0.5, Tool(tool, pre));
            }
            Both("WebFetch", "{\"url\":\"x\",\"prompt\":\"y\"}", "{\"url\":\"x\",\"prompt\":\"y\"}");
            Both("WebFetch", "{\"url\":\"x\",\"prompt\":\"y\"}", "{\"prompt\":\"y\",\"url\":\"x\"}");
            Both("WebFetch", "{\"n\":1.50}", "{\"n\":1.50}");
            Both("WebFetch", "{\"n\":1.50}", "{\"n\":1.5}");
            Both("WebFetch", "{\"n\":1e2}", "{\"n\":100}");
            Both("WebFetch", "{\"n\":1E2}", "{\"n\":1e2}");
            Both("WebFetch", "{\"n\":1e+2}", "{\"n\":1e2}");
            Both("WebFetch", "{\"n\":-0}", "{\"n\":0}");
            Both("WebFetch", "{\"n\":[1,{\"b\":2,\"a\":null}]}", "{\"n\":[1,{\"a\":null,\"b\":2}]}");

            // a command whose text is another call's whole input as System.Text.Json writes it is that call; as the
            // line wrote it (another encoder), it isn't
            const string odd = @"{""s"":""<é&'+`\""\\/\n\t\b\f\r\u0001\u001f\u007f\u0080 �😀 ~""}";
            foreach (var (text, id) in new[] { (Parse(odd).ToJsonString(), "toolu_w"), (Parse(odd).ToJsonString(Json), "toolu_x") })
            {
                b += 2;
                r.Claude(sid, "PermissionRequest", b, Tool("Bash", odd));
                r.Claude(sid, "PreToolUse", b + 0.5, Bash(text, id));
            }

            // lines as another hook could write them: spaced, and escaped otherwise
            string Line(string ev, double at, string input) =>
                "{ \"v\": 1, \"type\": \"event\", \"agent\": \"claude\", \"at\": " + J(at) + ", \"pid\": 4242, \"env\": { },\t\"payload\": "
                + "{ \"hook_event_name\": " + J(ev) + ", \"session_id\": " + J(sid) + ", \"tool_name\": \"WebFetch\", \"tool_input\": " + input + " } }";
            void Lines(string request, string pre)
            {
                b += 2;
                r.Apply(Line("PermissionRequest", b, request));
                r.Apply(Line("PreToolUse", b + 0.5, pre));
            }
            Lines(@"{ ""url"" : ""x"" ,  ""prompt"":""y"" }", @"{""url"":""x"",""prompt"":""y""}");
            Lines(@"{""url"":""x""}", @"{""url"":""x""}");
            Lines(@"{""s"":""é😀\/""}", @"{""s"":""é😀/""}");
            Lines(@"{""s"":""é""}", @"{""s"":""é""}");
            Lines(@"{""a"":{""k"":1,""k"":2}}", @"{""a"":{""k"":1,""k"":2}}");
            Lines(@"{""a"":{""k"":1,""k"":2}}", @"{""a"":{""k"":2}}");
        }
    }

    /// Events within half a millisecond of the latest are the same tick: Rank decides.
    static void SameTick(Replay r)
    {
        string sid = Sid(8);
        double b = r.T0 - 100;
        r.Claude(sid, "UserPromptSubmit", b);
        r.Claude(sid, "PostToolUse", b + 1, Bash("x", "t1"));
        r.Claude(sid, "PreToolUse", b + 1.0005, Bash("y", "t2"));
        r.Claude(sid, "PreToolUse", b + 0.9995, Bash("y", "t3"));
        r.Claude(sid, "PreToolUse", b + 0.9994, Bash("y", "t4"));
        r.Claude(sid, "SubagentStart", b + 1.0004);
        r.Claude(sid, "SubagentStop", b + 1.0004);
        r.Claude(sid, "SessionStart", b + 1.0004);
        r.Claude(sid, "UserPromptSubmit", b + 1.0006);
        r.Claude(sid, "Notification", b + 1.0006, Obj(("notification_type", "idle_prompt")));
        r.Claude(sid, "Stop", b + 1.0006);
        r.Claude(sid, "PostCompact", b + 1.0006, Obj(("trigger", "manual")));
        r.Claude(sid, "SessionEnd", b + 2);
        r.Claude(sid, "SessionEnd", b + 2.0005);
        r.Claude(sid, "SessionEnd", b + 2.0006);
        r.Claude(sid, "Stop", b + 2.0005);
        r.Claude(sid, "Stop", b + 2.0007);
    }

    /// A clock set back: a time well past now orders nothing, and a ts well past now moves on any taken event.
    static void Clock(Replay r)
    {
        string sid = Sid(9);
        double t0 = r.T0;
        r.Claude(sid, "Stop", t0 + 3600);
        r.Claude(sid, "Notification", t0 - 50, Obj(("notification_type", "agent_completed")));
        r.Claude(sid, "Notification", t0 - 60, Obj(("notification_type", "agent_completed")));
        r.Claude(sid, "PreToolUse", t0 + 3600, Bash("ls"));
        r.Claude(sid, "PermissionRequest", t0 + 3599.999, Bash("ls"));
        r.Claude(sid, "PostToolUse", t0 - 40, Bash("ls"));
        r.Claude(sid, "PostToolUse", t0 - 41, Bash("ls"));
        r.Claude(sid, "SessionEnd", t0 + 7200);
        r.Claude(sid, "SessionEnd", t0 - 30);
        r.Claude(sid, "Stop", t0 - 20);
        r.Claude(sid, "SessionEnd", t0 - 10);
        r.Claude(sid, "Stop", t0 - 10.0001);
    }

    // ------------------------------------------------------------------ malformed envelopes
    /// Envelopes and payloads of the wrong shape, each on a chat of its own where it makes one.
    static void Malformed(Replay r)
    {
        double t = r.T0 - 100;
        int n = 0;
        JsonObject Env(string ev = "SessionStart") => Envelope(t += 0.01, Payload($"bad-{n++:D2}", ev));
        void Take(JsonObject envelope) => r.Apply(envelope);

        Take(Env().Also(o => o.Remove(Ipc.Payload)));
        Take(Env().Also(o => o[Ipc.Payload] = new JsonArray(1, 2)));
        Take(Env().Also(o => o[Ipc.Payload] = "SessionStart"));
        Take(Env().Also(o => o[Ipc.Payload]["hook_event_name"] = 12));
        Take(Env().Also(o => ((JsonObject)o[Ipc.Payload]).Remove("hook_event_name")));
        Take(Env().Also(o => ((JsonObject)o[Ipc.Payload]).Remove("session_id")));
        Take(Env("Stop").Also(o => o[Ipc.Payload]["session_id"] = 99));
        Take(Env().Also(o => o[Ipc.Payload]["session_id"] = ""));
        Take(Env().Also(o => o.Remove(Ipc.At)));
        Take(Env().Also(o => o[Ipc.At] = "soon"));
        Take(Env().Also(o => o[Ipc.At] = null));
        Take(Env().Also(o => o[Ipc.At] = true));
        Take(Env().Also(o => o[Ipc.At] = 0));
        Take(Env().Also(o => o[Ipc.At] = -1.5));
        foreach (var pid in new JsonNode[] { null, "4242", 42.9, -42.9, 1e20, -1e20, 4.2e1, 0, -0.0, 9007199254740993 })
            Take(Env().Also(o => { if (pid == null) o.Remove(Ipc.Pid); else o[Ipc.Pid] = pid.DeepClone(); }));
        foreach (var agent in new JsonNode[] { null, "gemini", 42, "Claude", "cla\nude", new string('z', 80), "" })
            Take(Env().Also(o => { if (agent == null) o.Remove(Ipc.Agent); else o[Ipc.Agent] = agent.DeepClone(); }));
        Take(Env().Also(o => o[Ipc.V] = 2));
        Take(Env().Also(o => o[Ipc.Kind] = "ping"));
        Take(Env("PreToolUse").Also(o => { o[Ipc.Payload]["tool_name"] = "Edit"; o[Ipc.Payload]["tool_input"] = "/a/b.rs"; }));
        Take(Env("PreToolUse").Also(o => { o[Ipc.Payload]["tool_name"] = "Read"; o[Ipc.Payload]["tool_input"] = new JsonArray("/a/b.rs"); }));
        Take(Env("Stop\n"));
        Take(Env("Stop "));
        Take(Env("stop"));
        Take(Env(new string('E', 59) + "😀x"));
        Take(Env(new string('E', 100)));
        Take(Env("SessionEnd").Also(o => o[Ipc.Payload]["session_id"] = "never-seen"));
        foreach (var sid in new[] { "abcdefghijk😀tail", "abcdefghijkl😀tail", "tab\there\u0007bell", "é", new string('s', 13), new string('s', 14) })
            Take(Env().Also(o => o[Ipc.Payload]["session_id"] = sid));
        Take(Envelope(t += 0.01, Payload("bad-codex", "SessionStart")).Also(o => o[Ipc.Agent] = "Codex"));
    }

    // ------------------------------------------------------------------ random chats
    /// One to three Claude chats at once, as their hooks would deliver them.
    static void RandomChats(Replay r, int seed)
    {
        var rng = new Random(seed);
        var steps = new List<(double Arrive, int Seq, Action Take)>();
        void Add(double arrive, Action take) => steps.Add((arrive, steps.Count, take));
        int chats = 1 + rng.Next(3);
        for (int c = 0; c < chats; c++) RandomChat(r, rng, $"r{seed}-{c}-{rng.Next(1000):D3}", Add);
        foreach (var s in steps.OrderBy(s => s.Arrive).ThenBy(s => s.Seq)) s.Take();
    }

    static T Pick<T>(Random rng, params T[] items) => items[rng.Next(items.Length)];

    static void RandomChat(Replay r, Random rng, string sid, Action<double, Action> add)
    {
        // well before now, so nothing is near the 5 s by which a time past now tells a clock set back; a chat whose
        // first events came while the clock was an hour ahead has them 3600 s later
        double start = r.T0 - 400 + rng.NextDouble() * 100, t = start;
        double setBack = rng.Next(10) == 0 ? start + rng.NextDouble() * 30 : double.NegativeInfinity;
        string file = sid + ".jsonl";
        string transcript = rng.Next(4) > 0 ? Files + "/" + file : null;
        var env = new JsonObject();
        var entry = Pick(rng, "cli", "claude-desktop", "claude-vscode", null, "sdk-ts");
        if (entry != null) env["CLAUDE_CODE_ENTRYPOINT"] = entry;
        if (entry == "claude-desktop" && rng.Next(3) > 0)
            env["CLAUDE_CODE_HOST_SESSION_ID"] = "local_" + new Guid(Enumerable.Range(0, 16).Select(_ => (byte)rng.Next(256)).ToArray());
        string cwd = Pick(rng, "/home/u/src/app", "/home/u/src/api", "/tmp/scratch");
        int call = 0;

        void Emit(string ev, double when, JsonObject extra = null, double? lateness = null)
        {
            // the hook starts a few ms late, and `at` has ms precision
            double at = Math.Floor((when + rng.NextDouble() * 0.02) * 1000) / 1000;
            if (when < setBack) at += 3600;
            // most hooks take tens of ms to deliver; a slow one, seconds
            double late = lateness ?? (rng.Next(7) == 0 ? 0.2 + rng.NextDouble() * 3 : rng.NextDouble() * 0.08);
            var p = Payload(sid, ev, extra);
            if (transcript != null) p["transcript_path"] = transcript;
            p["cwd"] = cwd;
            var envelope = Envelope(at, p, (JsonObject)env.DeepClone(), 1000 + rng.Next(60000));
            add(when + late, () => r.Apply(envelope));
            // delivered twice
            if (rng.Next(25) == 0)
            {
                var again = (JsonObject)envelope.DeepClone();
                add(when + late + rng.NextDouble() * 2, () => r.Apply(again));
            }
        }

        if (transcript != null)
        {
            var name = Pick(rng, "fix the login flow", "Refactor: sessions", "", "a very long chat name that goes past the forty four characters");
            add(t - 1, () => r.Write(file, Part.T(UserLine("hi") + TitleLine(name, sid))));
            if (rng.Next(3) == 0)
            {
                var renamed = "renamed " + rng.Next(100);
                add(t + rng.NextDouble() * 60, () => r.Write(file, Part.T(UserLine("hi") + TitleLine(name, sid) + TitleLine(renamed, sid))));
            }
        }

        var commands = new[] { "ls", "npm test", "cargo build", "git status", "rm -rf build" };
        if (rng.Next(5) > 0) Emit("SessionStart", t, Obj(("source", "startup")));
        int turns = 1 + rng.Next(3);
        for (int turn = 0; turn < turns; turn++)
        {
            t += 0.5 + rng.NextDouble() * 4;
            Emit("UserPromptSubmit", t, Obj(("prompt", Pick(rng, "fix the build", "/compact", "", "explain\nthis", "Rename the helpers"))));
            int calls = rng.Next(5);
            for (int k = 0; k < calls; k++)
            {
                t += 0.05 + rng.NextDouble() * 2;
                var (tool, input) = rng.Next(8) switch
                {
                    0 or 1 or 2 => ("Bash", Obj(("command", Pick(rng, commands)))),
                    3 => ("Read", Obj(("file_path", "/home/u/src/" + Pick(rng, "a.rs", "b.rs")))),
                    4 => ("Edit", Obj(("file_path", "/home/u/src/" + Pick(rng, "a.rs", "b.rs")), ("old_string", "x"), ("new_string", "y"))),
                    5 => ("Grep", Obj(("pattern", "TODO"), ("path", "/home/u/src"))),
                    6 => ("mcp__claude-in-chrome__navigate", Obj(("url", "https://example.com/" + rng.Next(3)))),
                    _ => ("WebFetch", Obj(("url", "https://example.com/" + rng.Next(3)), ("prompt", "summarise"))),
                };
                var id = $"toolu_{call++:D3}";
                JsonObject Call(bool withId = true)
                {
                    var o = Obj(("tool_name", tool), ("tool_input", input.DeepClone()));
                    if (withId) o["tool_use_id"] = id;
                    return o;
                }
                double pre = t;
                Emit("PreToolUse", pre, Call());
                if (tool is not ("Read" or "Grep") && rng.Next(3) == 0)
                {
                    // the request's hook starts a few ms either side of its PreToolUse's
                    var ask = Call(withId: false);
                    if (tool == "Bash") ask["tool_input"]["description"] = "run it";
                    Emit("PermissionRequest", pre + rng.NextDouble() * 0.008 - 0.004, ask);
                    if (rng.Next(2) == 0)
                        Emit("Notification", pre + 0.01 + rng.NextDouble() * 0.2,
                            Obj(("notification_type", "permission_prompt"), ("message", "Claude needs your permission to use " + tool)));
                    t += 0.5 + rng.NextDouble() * 6;
                    Emit(Pick(rng, "PostToolUse", "PostToolUse", "PostToolUseFailure", "PermissionDenied"), t, Call());
                }
                else
                {
                    t += 0.05 + rng.NextDouble() * 3;
                    Emit(rng.Next(6) == 0 ? "PostToolUseFailure" : "PostToolUse", t, Call());
                }
            }
            if (rng.Next(5) == 0)
            {
                Emit("SubagentStart", t += 0.1, Obj(("agent_type", Pick<JsonNode>(rng, "Explore", "general-purpose", null))));
                Emit("SubagentStop", t += 1 + rng.NextDouble() * 4);
            }
            if (rng.Next(8) == 0)
            {
                Emit("PreCompact", t += 0.2, Obj(("trigger", "auto")));
                Emit("PostCompact", t += 2, Obj(("trigger", "auto")));
            }
            t += 0.1 + rng.NextDouble();
            // Stop is Claude's only synchronous hook: it comes in right away
            if (rng.Next(10) == 0) Emit("StopFailure", t, Obj(("error_type", Pick(rng, "rate_limit", "overloaded", "server_error"))), rng.NextDouble() * 0.01);
            else Emit("Stop", t, null, rng.NextDouble() * 0.01);
            if (rng.Next(3) == 0) Emit("Notification", t + 5 + rng.NextDouble() * 10, Obj(("notification_type", "idle_prompt")));
            t += 3;
        }
        if (rng.Next(6) == 0)
        {
            Emit("PreCompact", t += 1, Obj(("trigger", "manual")));
            Emit("PostCompact", t += 3, Obj(("trigger", "manual")));
        }
        if (rng.Next(4) == 0)
        {
            Emit("SessionEnd", t += 1, Obj(("reason", "prompt_input_exit")));
            if (rng.Next(3) == 0)
            {
                Emit("SessionStart", t += 5, Obj(("source", "resume")));
                Emit("UserPromptSubmit", t += 1, Obj(("prompt", "and again")));
                Emit("Stop", t += 2, null, 0);
            }
        }
    }

    // ------------------------------------------------------------------ Codex envelopes
    /// A Codex envelope as the hook writes it: no env, and `sent` 10 ms after `at`.
    static JsonObject CodexEnvelope(double at, JsonObject payload, long pid = 4242) => new()
    {
        [Ipc.V] = Ipc.Version, [Ipc.Kind] = "event", [Ipc.Agent] = "codex", [Ipc.At] = at, [Ipc.Pid] = pid,
        [Ipc.Payload] = payload, [Ipc.Sent] = at + 0.01,
    };

    /// A payload as the tests' Events.Payload makes it: hook_event_name and session_id, a turn_id when given, and extra.
    static JsonObject CodexPayload(string sid, string ev, string turn = null, JsonObject extra = null)
    {
        var p = new JsonObject { ["hook_event_name"] = ev, ["session_id"] = sid };
        if (turn != null) p["turn_id"] = turn;
        if (extra != null)
            foreach (var (k, v) in extra) p[k] = v?.DeepClone();
        return p;
    }

    /// A Codex session id, with letters in it (ThreadOf finds it in a path ignoring case).
    static string CodexSid(int n) => $"0199c0de-{n / 10000 % 10000:D4}-7abc-8def-{n:D12}";

    /// A turn id as Codex makes them: a UUIDv7 made at `unix` (ms precision), its other bits from n.
    static string V7(double unix, int n = 0)
    {
        long ms = (long)Math.Floor(unix * 1000);
        var id = $"{ms >> 16:x8}-{ms & 0xFFFF:x4}-7{n & 0xFFF:x3}-{0x8000 | ((n >> 12) & 0x3FFF):x4}-{(uint)n * 2654435761L & 0xFFFFFFFFFFFFL:x12}";
        if (id.Length != 36 || id[14] != '7' || !Guid.TryParseExact(id, "D", out _))
            throw new InvalidOperationException("not a UUIDv7: " + id);
        return id;
    }

    /// A sub-agent's event: its agent_id, on top of `extra`.
    static JsonObject SubAgent(string agentId, JsonObject extra = null)
    {
        var o = extra ?? new JsonObject();
        o["agent_id"] = agentId;
        return o;
    }

    /// The first line of a Codex transcript.
    static string MetaLine(string originator) =>
        "{\"timestamp\":\"2026-09-29T10:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"x\",\"originator\":" + J(originator)
        + ",\"cli_version\":\"0.40.0\",\"instructions\":\"be helpful\"}}\n";

    /// A line of Codex's session index.
    static string IndexLine(string sid, string name) =>
        "{\"id\":" + J(sid) + ",\"thread_name\":" + J(name) + ",\"updated_at\":\"2026-09-29T10:00:00Z\"}\n";

    // ------------------------------------------------------------------ tests/AiPet.Tests/OrderingTests.cs (Codex)
    /// CodexOrderingTests, a case each (a theory's each inline data too): the same envelopes at the same times, with
    /// turn ids made as the tests make them (v7, a second apart, a minute before the case began).
    static void CodexOrdering(Func<string, int, Replay> newCase)
    {
        string sid = CodexSid(1);
        const string agent = "019a0000-0000-7000-8000-00000000a1a1";
        (Replay, double, string, string, string) Case(string name)
        {
            var r = newCase("ordering/codex/" + name, 1);
            return (r, r.T0, V7(r.T0 - 60, 1), V7(r.T0 - 59, 2), V7(r.T0 - 58, 3));
        }
        JsonObject Prompt(string text) => Obj(("prompt", text));
        JsonObject Manual() => Obj(("trigger", "manual"));
        {
            var (r, t0, turn1, turn2, _) = Case("EarlierTurn_AfterTheNextTurnStarted_IsStale_EvenWithALaterAt");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "UserPromptSubmit", turn2, t0 + 1, Prompt("second"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 5, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", turn1, t0 + 6);
        }
        {
            var (r, t0, turn1, _, _) = Case("NothingOfATurn_IsTakenAfterItsStop");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "Stop", turn1, t0 + 1);
            r.Codex(sid, "PreToolUse", turn1, t0 + 2, Bash("ls", "call_1"));
            r.Codex(sid, "PermissionRequest", turn1, t0 + 3, Bash("ls"));
            r.Codex(sid, "PostToolUse", turn1, t0 + 4, Bash("ls", "call_1"));
        }
        {
            var (r, t0, turn1, _, _) = Case("NothingOfATurn_IsTakenAfterItsInterrupt");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "Interrupt", turn1, t0 + 1);
            r.Codex(sid, "PreToolUse", turn1, t0 + 2, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", turn1, t0 + 3);
        }
        {
            var (r, t0, turn1, _, _) = Case("PreToolUse_AfterItsOwnPostToolUse_IsStale");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PostToolUse", turn1, t0 + 2, Bash("ls", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 3, Bash("ls", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 4, Bash("ls", "call_2"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PreToolUse_AfterItsPermissionRequest_IsStale");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PermissionRequest", turn1, t0 + 2, Bash("rm -rf build", description: "clean"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 3, Bash("rm -rf build", "call_1"));
            r.Codex(sid, "PostToolUse", turn1, t0 + 4, Bash("rm -rf build", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 5, Bash("rm -rf build", "call_1"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PreToolUse_OfAnotherCommand_AfterAPermissionRequest_IsTaken");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PermissionRequest", turn1, t0 + 2, Bash("rm -rf build"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 3, Bash("ls", "call_2"));
        }
        {
            var (r, t0, turn1, turn2, _) = Case("OwnNewTurn_IsTaken_EvenWhenItsAtIsBeforeThePreviousStop");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "Stop", turn1, t0 + 5);
            r.Codex(sid, "UserPromptSubmit", turn2, t0 + 1, Prompt("go on"));
            r.Codex(sid, "PreToolUse", turn2, t0 + 2, Bash("make", "call_9"));
            r.Codex(sid, "Stop", turn2, t0 + 1.5);
        }
        {
            var (r, t0, turn1, turn2, _) = Case("SubAgent_AfterTheChatsStop_IsStale_UntilTheNextTurn");
            string sub1 = V7(r.T0 - 30, 11), sub2 = V7(r.T0 - 10, 12);
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PreToolUse", sub1, t0 + 1, SubAgent(agent, Bash("ls", "sub_1")));
            r.Codex(sid, "Stop", turn1, t0 + 2);
            r.Codex(sid, "PostToolUse", sub1, t0 + 3, SubAgent(agent, Bash("ls", "sub_1")));
            r.Codex(sid, "PreToolUse", sub2, t0 + 4, SubAgent(agent, Bash("pwd", "sub_2")));
            r.Codex(sid, "UserPromptSubmit", turn2, t0 + 5);
            r.Codex(sid, "PreToolUse", sub2, t0 + 6, SubAgent(agent, Bash("pwd", "sub_3")));
        }
        {
            var (r, t0, _, _, _) = Case("NonV7TurnIds_OnlyTellASeenTurnFromANewOne");
            string a = "3f2504e0-4f89-41d3-9a0c-0305e82c3301", b = "7c9e6679-7425-40de-944b-e07fc1f90ae7",
                c = "a8098c1a-f86e-41d4-a716-446655440000";
            r.Codex(sid, "UserPromptSubmit", a, t0);
            r.Codex(sid, "UserPromptSubmit", b, t0 + 1);
            r.Codex(sid, "PreToolUse", a, t0 + 2, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", b, t0 + 3);
            r.Codex(sid, "PreToolUse", b, t0 + 4, Bash("ls", "call_2"));
            r.Codex(sid, "UserPromptSubmit", c, t0 + 5);
            r.Codex(sid, "Stop", b, t0 + 6);
        }
        {
            var (r, t0, turn1, _, _) = Case("EventsWithoutATurn_GoByAt");
            r.Codex(sid, "UserPromptSubmit", turn1, t0 + 1);
            r.Codex(sid, "SessionStart", null, t0);
            r.Codex(sid, "SessionStart", null, t0 + 2);
        }
        {
            var (r, t0, turn1, turn2, _) = Case("LateUserPromptSubmit_OfASeenTurn_KeepsTheState_ButNamesTheChat");
            r.Codex(sid, "UserPromptSubmit", turn1, t0, Prompt("first"));
            r.Codex(sid, "Stop", turn1, t0 + 1);
            r.Codex(sid, "PreToolUse", turn2, t0 + 2, Bash("cargo test", "call_a"));
            r.Codex(sid, "PermissionRequest", turn2, t0 + 2.5, Bash("cargo test"));
            r.Codex(sid, "UserPromptSubmit", turn2, t0 + 3.5, Prompt("run the tests"));
            r.Codex(sid, "PreToolUse", turn2, t0 + 3, Bash("ls", "call_b"));
        }
        {
            var (r, t0, turn1, turn2, _) = Case("UserPromptSubmit_ThenItsTurnsEvents_AreTaken");
            r.Codex(sid, "UserPromptSubmit", turn1, t0, Prompt("first"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 1, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", turn1, t0 + 2);
            r.Codex(sid, "UserPromptSubmit", turn2, t0 + 3, Prompt("second"));
            r.Codex(sid, "PreToolUse", turn2, t0 + 4, Bash("ls", "call_2"));
        }
        {
            var (r, t0, turn1, _, _) = Case("NewChat_FirstEventIsItsPrompt_IsTaken");
            r.Codex(sid, "UserPromptSubmit", turn1, t0, Prompt("hello"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PermissionRequest_OfARerun_GoesToTheRerun_NotTheEarlierCallOfTheSameCommand");
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("npm test", "call_1"));
            r.Codex(sid, "PermissionRequest", turn1, b + 21, Bash("npm test", description: "rerun"));
            r.Codex(sid, "PreToolUse", turn1, b + 22.5, Bash("npm test", "call_2"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PermissionRequest_OfARerun_ArrivingAfterItsPreToolUse_StartedBeforeIt_IsTaken");
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("npm test", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, b + 22.5, Bash("npm test", "call_2"));
            r.Codex(sid, "PermissionRequest", turn1, b + 21, Bash("npm test"));
            r.Codex(sid, "PreToolUse", turn1, b + 22.6, Bash("npm test", "call_2"));
        }
        foreach (var (request, rerun) in new[] { (3.5, 4.0), (2.2, 3.9), (4.8, 3.1) })
        {
            var (r, t0, turn1, _, _) = Case(FormattableString.Invariant(
                $"PermissionRequest_OfAFastRerun_ArrivingBeforeItsPreToolUse_KeepsTheChatAsking({request:0.0}, {rerun:0.0})"));
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("npm test", "call_1"));
            r.Codex(sid, "PermissionRequest", turn1, b + request, Bash("npm test", description: "rerun"));
            r.Codex(sid, "PreToolUse", turn1, b + rerun, Bash("npm test", "call_2"));
            r.Codex(sid, "PreToolUse", turn1, b + rerun + 0.1, Bash("npm test", "call_2"));
        }
        foreach (var (request, rerun) in new[] { (3.5, 4.0), (2.2, 3.9), (4.8, 3.1) })
        {
            var (r, t0, turn1, _, _) = Case(FormattableString.Invariant(
                $"PermissionRequest_OfAFastRerun_ArrivingAfterItsPreToolUse_IsTaken({request:0.0}, {rerun:0.0})"));
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("npm test", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, b + rerun, Bash("npm test", "call_2"));
            r.Codex(sid, "PermissionRequest", turn1, b + request, Bash("npm test", description: "rerun"));
            r.Codex(sid, "PreToolUse", turn1, b + rerun + 0.1, Bash("npm test", "call_2"));
        }
        {
            var (r, t0, turn1, _, _) = Case("SameCommandAgain_AfterTheAskingCallsPostToolUse_IsTaken");
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("npm test", "call_1"));
            r.Codex(sid, "PermissionRequest", turn1, b + 1.2, Bash("npm test"));
            r.Codex(sid, "PostToolUse", turn1, b + 2, Bash("npm test", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, b + 3, Bash("npm test", "call_2"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PairedRequest_DoesNotMoveTheLatestBack");
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("ls", "call_a"));
            r.Codex(sid, "PreToolUse", turn1, b + 3, Bash("rm -rf build", "call_b"));
            r.Codex(sid, "PermissionRequest", turn1, b + 2, Bash("rm -rf build"));
            r.Codex(sid, "PostToolUse", turn1, b + 2.5, Bash("ls", "call_a"));
        }
        {
            var (r, t0, turn1, turn2, _) = Case("LateUserPromptSubmit_AfterItsTurnsStop_StillNamesTheChat");
            r.Codex(sid, "UserPromptSubmit", turn1, t0, Prompt("first"));
            r.Codex(sid, "Stop", turn1, t0 + 1);
            r.Codex(sid, "PreToolUse", turn2, t0 + 2, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", turn2, t0 + 3);
            r.Codex(sid, "UserPromptSubmit", turn2, t0 + 4, Prompt("second"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PermissionRequest_BehindAnotherCall_GoesByAt");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PreToolUse", turn1, t0 + 2, Bash("rm -rf build", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, t0 + 3, Bash("ls", "call_2"));
            r.Codex(sid, "PermissionRequest", turn1, t0 + 1.5, Bash("rm -rf build"));
        }
        {
            var (r, t0, turn1, _, _) = Case("TwoIdenticalCalls_FarApart_TheSecondsRequestGoesWithTheSecond");
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PreToolUse", turn1, b + 1, Bash("npm test", "call_1"));
            r.Codex(sid, "PreToolUse", turn1, b + 60, Bash("npm test", "call_2"));
            r.Codex(sid, "PermissionRequest", turn1, b + 60.5, Bash("npm test"));
            r.Codex(sid, "PostToolUse", turn1, b + 70, Bash("npm test", "call_2"));
            r.Codex(sid, "PreToolUse", turn1, b + 71, Bash("npm test", "call_2"));
        }
        {
            var (r, t0, turn1, _, _) = Case("PendingRequest_IsNotTakenByTheSameCommandLongAfter");
            double b = t0 - 100;
            r.Codex(sid, "UserPromptSubmit", turn1, b);
            r.Codex(sid, "PermissionRequest", turn1, b + 1, Bash("npm test"));
            r.Codex(sid, "PreToolUse", turn1, b + 30, Bash("npm test", "call_3"));
        }
        {
            var (r, t0, turn1, turn2, turn3) = Case("ManualCompact_EndsItsTurn");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "Stop", turn1, t0 + 1);
            r.Codex(sid, "PreCompact", turn2, t0 + 2, Manual());
            r.Codex(sid, "PostCompact", turn2, t0 + 3, Manual());
            r.Codex(sid, "UserPromptSubmit", turn3, t0 + 4);
        }
        {
            var (r, t0, turn1, turn2, _) = Case("ManualCompact_PreCompactArrivingLast_IsStale");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "Stop", turn1, t0 + 1);
            r.Codex(sid, "PostCompact", turn2, t0 + 3, Manual());
            r.Codex(sid, "PreCompact", turn2, t0 + 4, Manual());
        }
        {
            var (r, t0, turn1, _, _) = Case("AutoCompact_StaysInItsTurn");
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PreCompact", turn1, t0 + 1, Obj(("trigger", "auto")));
            r.Codex(sid, "PostCompact", turn1, t0 + 2, Obj(("trigger", "auto")));
            r.Codex(sid, "PreToolUse", turn1, t0 + 3, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", turn1, t0 + 4);
        }
        {
            var (r, t0, turn1, _, _) = Case("SubAgentsCompact_DoesNotEndTheChatsTurn");
            string sub = V7(r.T0 - 30, 11);
            r.Codex(sid, "UserPromptSubmit", turn1, t0);
            r.Codex(sid, "PostCompact", sub, t0 + 1, SubAgent(agent, Manual()));
            r.Codex(sid, "PreToolUse", sub, t0 + 2, SubAgent(agent, Bash("ls", "sub_1")));
            r.Codex(sid, "PreToolUse", turn1, t0 + 3, Bash("ls", "call_1"));
        }
        {
            var (r, t0, _, _, _) = Case("ClockSetBack_NewTurnsAreTaken_AndTheOldTurnStaysStale");
            // turn 1 began while the clock was an hour ahead, so its id sorts after every turn made once it was set right
            string ahead = V7(r.T0 + 3600, 21), after = V7(r.T0, 22);
            r.Codex(sid, "UserPromptSubmit", ahead, t0 + 3600);
            r.Codex(sid, "Stop", ahead, t0 + 3601);
            r.Codex(sid, "UserPromptSubmit", after, t0, Prompt("after"));
            r.Codex(sid, "PreToolUse", after, t0 + 1, Bash("ls", "call_1"));
            r.Codex(sid, "Stop", after, t0 + 2);
            r.Codex(sid, "PreToolUse", ahead, t0 + 3602, Bash("ls", "call_0"));
        }
        {
            var (r, t0, turn1, turn2, turn3) = Case("NewChat_StartsFromItsFirstEventsTurn");
            r.Codex(sid, "PreToolUse", turn2, t0 + 2, Bash("ls", "call_1"));
            r.Codex(sid, "UserPromptSubmit", turn1, t0 + 3);
            r.Codex(sid, "Stop", turn2, t0 + 4);
            r.Codex(sid, "UserPromptSubmit", turn3, t0 + 5);
        }
    }

    // ------------------------------------------------------------------ Codex: the event → state table
    /// One chat through every Codex event (and others it doesn't take), prompts of every shape, a helper's prompt, and
    /// each compaction. No turn ids: each goes by `at`.
    static void CodexEvents(Replay r)
    {
        string sid = CodexSid(2);
        double t = r.T0 - 100;
        string Ev(string ev, JsonObject extra = null) => r.Codex(sid, ev, null, t += 0.01, extra);

        Ev("SessionStart", Obj(("source", "startup")));
        Ev("UserPromptSubmit", Obj(("prompt", "  fix   the\tbuild  \r\nand more")));
        Ev("UserPromptSubmit", Obj(("prompt", "/review")));
        Ev("UserPromptSubmit", Obj(("prompt", "")));
        Ev("UserPromptSubmit");
        Ev("UserPromptSubmit", Obj(("prompt", 42)));
        Ev("UserPromptSubmit", SubAgent("019a0000-0000-7000-8000-00000000a1a1", Obj(("prompt", "a helper's prompt"))));
        Ev("UserPromptSubmit", Obj(("prompt", "an empty agent id"), ("agent_id", "")));
        Ev("UserPromptSubmit", Obj(("prompt", "a numeric agent id"), ("agent_id", 7)));
        Ev("PreToolUse", Bash("make"));
        Ev("PermissionRequest", Bash("make", description: "build it"));
        Ev("PostToolUse", Bash("make"));
        Ev("PreCompact", Obj(("trigger", "auto")));
        Ev("PostCompact", Obj(("trigger", "auto")));
        Ev("PostCompact", Obj(("trigger", "manual")));
        Ev("PreCompact", Obj(("trigger", "manual")));
        Ev("PostCompact");
        Ev("PostCompact", SubAgent("019a0000-0000-7000-8000-00000000a1a1", Obj(("trigger", "manual"))));
        Ev("PostCompact", Obj(("trigger", "manual"), ("transcript_path", Files + "/rollout-another-thread.jsonl")));
        Ev("PostCompact", Obj(("trigger", "manual"), ("transcript_path", Files + "/rollout-" + sid.ToUpperInvariant() + ".jsonl")));
        Ev("PostCompact", Obj(("trigger", "MANUAL")));
        foreach (var kind in new JsonNode[] { null, "general-purpose", "default", "explorer", "  ", 5, "" })
            Ev("SubagentStart", kind == null ? null : Obj(("agent_type", kind)));
        Ev("SubagentStop");
        Ev("Stop");
        Ev("SessionStart", Obj(("source", "resume")));
        Ev("UserPromptSubmit", Obj(("prompt", "go on")));
        Ev("SessionStart", Obj(("source", "compact")));
        Ev("Interrupt");
        Ev("Interrupt");
        // not Codex's
        foreach (var ev in new[] { "PostToolUseFailure", "Notification", "PermissionDenied", "Elicitation", "StopFailure", "SomethingNew", "stop", "Stop " })
            Ev(ev);
        Ev("Stop");
        Ev("SessionEnd", Obj(("reason", "exit")));
        Ev("SessionEnd");
        Ev("Stop");
        Ev("SessionStart");
    }

    // ------------------------------------------------------------------ Codex: describing tool calls
    static readonly (string Tool, string Input)[] CodexTools =
    {
        ("apply_patch", J(new { command = "*** Begin Patch\n*** Update File: src/main.rs\n@@\n-a\n+b\n*** End Patch" })),
        ("apply_patch", J(new { command = "*** Begin Patch\n*** Add File: docs/new.md\n+x\n*** Delete File: old.txt\n*** End Patch" })),
        ("apply_patch", J(new { command = "*** Update File: a.rs\n*** Update File: a.rs\n" })),
        ("apply_patch", J(new { command = "*** Update File: a.rs \r\n*** Update File: a.rs\r\n*** Update File:  a.rs" })),
        ("apply_patch", J(new { command = "*** Update File: a.rs\n*** Update File: b.rs\n*** Add File: c.rs\n*** Delete File: a.rs" })),
        ("apply_patch", J(new { command = "*** Update File: \n*** Update File:  \t\n" })),
        ("apply_patch", J(new { command = "*** Update File: \n*** Update File: " })),
        ("apply_patch", J(new { command = "***Update File: a.rs\n *** Update File: b.rs\n*** update File: c.rs\n*** Move File: d.rs\n*** Update File:e.rs\n*** Update file: f.rs" })),
        ("apply_patch", J(new { command = "*** Update File: dir/sub/\n" })),
        ("apply_patch", J(new { command = "*** Update File: /\n" })),
        ("apply_patch", J(new { command = "\u3000*** Update File: x\n*** Delete File: \u3000spaced name.txt\u3000\u00a0\u2028\n" })),
        ("apply_patch", J(new { command = "*** Update File: é/ü😀.rs" })),
        ("apply_patch", J(new { command = "prefix\n*** Update File: a.rs\n*** Update File: A.rs\n" })),
        ("apply_patch", J(new { command = "*** Update File: a.rs\r*** Update File: b.rs" })),
        ("apply_patch", J(new { command = "*** Update File: *** Update File: x.rs\n" })),
        ("apply_patch", J(new { command = "" })), ("apply_patch", "{}"), ("apply_patch", J(new { command = 7 })), ("apply_patch", "null"),
        ("apply_patch", "\"*** Update File: a.rs\""), ("apply_patch", "[\"*** Update File: a.rs\"]"),
        ("apply_patch", J(new { input = "*** Update File: a.rs", file_path = "/x/b.rs" })),
        ("apply_patch", J(new { command = "*** Update File: a.rs", file_path = "/x/b.rs" })),
        ("spawn_agent", null), ("spawn_agent", "{\"prompt\":\"go\"}"), ("view_image", "{\"path\":\"/tmp/a.png\"}"), ("view_image", null),
        ("shell", "{\"command\":[\"ls\"]}"), ("exec_command", "{\"cmd\":\"make\"}"), ("local_shell", null), ("web_search", null),
        ("update_plan", null), ("mcp__atlassian__search", null), ("mcp__Claude_Browser__x", null), ("write_stdin", null),
        ("Bash", null), ("", null), ("Spawn_agent", null), ("APPLY_PATCH", J(new { command = "*** Update File: a.rs" })),
    };

    /// A PreToolUse for each tool, on one chat; then one without a tool_name and one with a number for it.
    static void CodexDescribe(Replay r)
    {
        string sid = CodexSid(3);
        double t = r.T0 - 100;
        foreach (var (tool, input) in CodexTools) r.Codex(sid, "PreToolUse", null, t += 0.01, Tool(tool, input));
        r.Codex(sid, "PreToolUse", null, t += 0.01, Obj(("tool_input", Parse("{\"command\":\"*** Update File: a.rs\"}"))));
        r.Codex(sid, "PreToolUse", null, t += 0.01, Obj(("tool_name", 12)));
    }

    // ------------------------------------------------------------------ Codex: where, from the transcript
    /// The originator of transcripts of every shape, each read by the SessionStart of a chat of its own; then when the
    /// transcript is read.
    static void CodexWhere(Replay r, Replay reads)
    {
        const int Head = 4 * 1024 * 1024;
        double t = r.T0 - 100;
        int n = 0;
        void Chat(params Part[] transcript)
        {
            string sid = CodexSid(100 + n++), file = "rollout-" + sid + ".jsonl";
            r.Write(file, transcript);
            r.Codex(sid, "SessionStart", null, t += 0.01, Obj(("transcript_path", Files + "/" + file)));
        }
        foreach (var o in new[] { "Codex Desktop", "codex_work_desktop", "codex-tui", "codex_cli_rs", "codex_exec", "codex_vscode",
                                  "CODEX-TUI", "Codex_Web App", "İSTANBUL ΣΑΣ", "ẞig Ωhm Kelvin Å 𐐀 Ა", "", " ", "tab\there", "a\u0001b",
                                  new string('o', 80) })
            Chat(Part.T(MetaLine(o) + "{\"type\":\"response_item\"}\n"));
        // the regex over the first line's raw text
        foreach (var line in new[]
        {
            "{\"type\":\"session_meta\",\"payload\":{\"originator\" : \"spaced\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\"\t:\u3000\"unicode spaces\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\"\u0085:\u00a0\"nel and nbsp\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\"\r:\"carriage return\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\"\u200b:\"zero width\",\"x\":{\"originator\":\"after zero width\"}}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":\"a\\\"b\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":\"\\u0041pp\"}}",
            "{\"x\":\"originator\",\"type\":\"session_meta\",\"payload\":{\"originator\":\"second try\"}}",
            "\"originator\"originator\":\"overlapping\" \"session_meta\"",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":\"no closing quote}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":null}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":42,\"o\":{\"originator\":\"nested later\"}}}",
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"x\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"Originator\":\"case\"}}",
            "{\"type\":\"session-meta\",\"payload\":{\"originator\":\"not the meta\"}}",
            "{\"type\":\"SESSION_META\",\"payload\":{\"originator\":\"upper meta\"}}",
            "session_meta \"session_meta\" \"originator\":\"not json at all\"",
            "{\"payload\":{\"originator\":\"meta after\"},\"type\":\"session_meta\"}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":\"\"}}",
            // raw (a JSON writer escapes what's past U+FFFF): lower-cased a character, a surrogate pair too, at a time
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":\"\U00010400\U00010401 Ω K Å İ ΣΣ ẞ ǅ\"}}",
            "{\"type\":\"session_meta\",\"payload\":{\"originator\":\"" + new string('o', 59) + "\U00010400 cut by the log\"}}",
        })
            Chat(Part.T(line + "\n{\"type\":\"session_meta\",\"payload\":{\"originator\":\"second line\"}}\n"));
        // the first line only, within the first 4 MB
        Chat(Part.T("{\"type\":\"response_item\"}\n" + MetaLine("codex-tui")));
        Chat(Part.T(MetaLine("no newline").TrimEnd('\n')));
        Chat(Part.T(MetaLine("codex-tui").Replace("\n", "\r\n")));
        Chat(Part.T("{\"type\":\"session_meta\",\"payload\":{\"originator\":\"early\",\"instructions\":\""), Part.T("x", Head), Part.T("\"}}\n"));
        Chat(Part.T("{\"type\":\"session_meta\",\"payload\":{\"instructions\":\""), Part.T("x", Head), Part.T("\",\"originator\":\"late\"}}\n"));
        Chat(Part.T("{\"payload\":{\"originator\":\"meta too late\",\"instructions\":\""), Part.T("x", Head), Part.T("\"},\"type\":\"session_meta\"}\n"));
        // its closing quote the 4 MB's last byte, and one past it
        string head = "{\"type\":\"session_meta\",\"payload\":{\"instructions\":\"", tail = "\",\"originator\":\"edge\"";
        int fill = Head - Encoding.UTF8.GetByteCount(head) - Encoding.UTF8.GetByteCount(tail);
        Chat(Part.T(head), Part.T("x", fill), Part.T(tail + "}}\n"));
        Chat(Part.T(head), Part.T("x", fill + 1), Part.T(tail + "}}\n"));
        // a character cut at 4 MB
        Chat(Part.T("{\"type\":\"session_meta\",\"payload\":{\"originator\":\"cut char\",\"instructions\":\""), Part.T("x", Head - 80), Part.T("é", 100));
        Chat(Part.H(new byte[] { 0xEF, 0xBB, 0xBF }), Part.T(MetaLine("after a bom")));
        Chat(Part.T("{\"type\":\"session_meta\",\"payload\":{\"originator\":\"bad "), Part.H(new byte[] { 0xFF, 0xC3, 0x28, 0xED, 0xA0, 0x80 }), Part.T(" bytes\"}}\n"));
        Chat(Part.H(new byte[] { 0xFF, 0xFE }.Concat(Encoding.Unicode.GetBytes(MetaLine("utf-16"))).ToArray()));
        Chat();
        // no file to read
        var paths = new JsonNode[] { "", Files + "/missing.jsonl", Files, Files + "/a\u0000b.jsonl", 5 };
        for (int i = 0; i < paths.Length; i++)
            r.Codex(CodexSid(190 + i), "SessionStart", null, t += 0.01, Obj(("transcript_path", paths[i]?.DeepClone())));
        r.Codex(CodexSid(199), "SessionStart", null, t += 0.01);

        // read while the chat has no where, whatever the event, stale ones too; not once it has one; again after its end
        string sid = CodexSid(200), file = "rollout-" + sid + ".jsonl", path = Files + "/" + file;
        double b = reads.T0 - 100;
        string Ev(string ev, JsonObject extra = null, string turn = null) =>
            reads.Codex(sid, ev, turn, b += 0.01, (extra ?? new JsonObject()).Also(o => o["transcript_path"] = path));
        Ev("SessionStart");
        reads.Write(file, Part.T(MetaLine("")));
        Ev("UserPromptSubmit", Obj(("prompt", "hi")));
        reads.Write(file, Part.T(MetaLine("codex_exec")));
        reads.Codex(sid, "Stop", null, b - 5, Obj(("transcript_path", path)));
        reads.Write(file, Part.T(MetaLine("codex-tui")));
        Ev("SomethingNew");
        Ev("PreToolUse", Bash("ls"));
        reads.Write(file, Part.T(MetaLine("Codex Desktop")));
        Ev("Stop");
        Ev("SessionEnd");
        Ev("SessionStart");
        // another transcript names nothing new once the chat has its where
        reads.Codex(sid, "Stop", null, b += 0.01, Obj(("transcript_path", Files + "/missing.jsonl")));
        // a sub-agent's event reads the transcript it names
        string helper = CodexSid(201), helperFile = "rollout-" + helper + ".jsonl";
        reads.Write(helperFile, Part.T(MetaLine("codex_vscode")));
        reads.Codex(helper, "PreToolUse", V7(b - 50, 1), b += 0.01,
            SubAgent("019a0000-0000-7000-8000-00000000a1a1", Bash("ls", "c1")).Also(o => o["transcript_path"] = Files + "/" + helperFile));
    }

    // ------------------------------------------------------------------ Codex: names from the session index
    /// Codex's names from session_index.jsonl in CODEX_HOME (the case's folder): lines of every shape, each for a chat of
    /// its own that its SessionStart reads; when names are read; and the last 256 KB.
    static void CodexNames(Replay r, Replay reads, Replay window)
    {
        const string Index = "session_index.jsonl";
        const int Tail = 256 * 1024;
        var lines = new StringBuilder();
        var chats = new List<string>();
        void Chat(params Func<string, string>[] forChat)
        {
            string sid = CodexSid(300 + chats.Count);
            chats.Add(sid);
            foreach (var line in forChat) lines.Append(line(sid)).Append('\n');
        }
        string Named(string sid, string name) => IndexLine(sid, name).TrimEnd('\n');
        string Earlier(string sid) => Named(sid, "earlier");
        // arrays that nest the line to `depth` in all, with its object
        string Deep(int depth) => new string('[', depth - 1) + new string(']', depth - 1);
        // the id with its fifth character (a letter) escaped, so the raw line hasn't the id
        string Escaped(string sid) => sid[..4] + "\\u00" + ((int)sid[4]).ToString("x2") + sid[5..];

        Chat(s => Named(s, "plain name"));
        Chat(s => Named(s, "first"), s => Named(s, "second wins"));
        Chat(Earlier, s => Named(s, ""));
        Chat(Earlier, s => Named(s, " \t "));
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":42}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":null}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":[\"x\"]}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)}}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"cut");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"dup\",\"thread_name\":\"dup two\"}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"dup id\",\"i\\u0064\":{J(s)}}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"nested dup\",\"o\":{{\"a\":1,\"a\":2}}}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"deep 64\",\"x\":{Deep(64)}}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"deep 65\",\"x\":{Deep(65)}}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"lone \\ud800 surrogate\"}}");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"fine\",\"other\":\"\\udc00\"}}");
        Chat(Earlier, s => $"[{J(s)},{{\"thread_name\":\"in an array\"}}]");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"garbage\"}} x");
        Chat(Earlier, s => $"{{\"id\":{J(s)},\"thread_name\":\"comma\",}}");
        Chat(Earlier, s => $"{{\"id\":{J(s.ToUpperInvariant())},\"thread_name\":\"upper id\",\"x\":{J(s)}}}");
        Chat(Earlier, s => $"{{\"id\":42,\"thread_name\":\"number id\",\"x\":{J(s)}}}");
        Chat(Earlier, s => $"{{\"id\":null,\"thread_name\":\"null id\",\"x\":{J(s)}}}");
        Chat(Earlier, s => $"{{\"id\":\"{Escaped(s)}\",\"thread_name\":\"escaped id of {s}\"}}");
        Chat(Earlier, s => $"{{\"id\":\"{Escaped(s)}\",\"thread_name\":\"escaped id only\"}}");
        Chat(Earlier, s => $"{{\"id\":{J(s + " ")},\"thread_name\":\"a longer id\"}}");
        Chat(s => $"{{\"thread_name\":\"keys the other way round\",\"id\":{J(s)}}}");
        Chat(s => Named(s, "a rather long name for a chat that goes on and on and on"));
        Chat(s => Named(s, "two\nlines\tand  spaces"));
        Chat(s => $"{{\"id\":{J(s)},\"thread_name\":\"big number\",\"n\":1e400,\"m\":-0,\"k\":1.50}}");
        Chat(Earlier, s => "\ufeff" + Named(s, "inner bom"));
        Chat(s => Named(s, "crlf") + "\r");
        Chat(s => "  \t" + Named(s, "indented"));
        Chat(s => Named(s, new string('a', 42) + "😀 and more"));
        Chat(s => Named(CodexSid(999), "someone else's"));
        Chat();
        // the whole index, after a byte order mark (StreamReader drops it)
        r.Write(Index, Part.H(new byte[] { 0xEF, 0xBB, 0xBF }), Part.T(lines.ToString()));
        double t = r.T0 - 100;
        foreach (var sid in chats) r.Codex(sid, "SessionStart", null, t += 0.01);

        // read on SessionStart, UserPromptSubmit and Stop, and whenever the chat has none; never for an ignored event
        {
            string sid = CodexSid(400);
            double b = reads.T0 - 100;
            void Names(params string[] names) => reads.Write(Index, Part.T(string.Concat(names.Select(n => IndexLine(sid, n)))));
            string Ev(string ev, JsonObject extra = null) => reads.Codex(sid, ev, null, b += 0.01, extra);
            Ev("SessionStart");
            Names("first");
            Ev("PreToolUse", Bash("ls"));
            Names("first", "second");
            Ev("PostToolUse", Bash("ls"));
            Ev("PermissionRequest", Bash("make"));
            Ev("Stop");
            Names("first", "second", "third");
            Ev("UserPromptSubmit", Obj(("prompt", "go")));
            Names("fourth");
            Ev("SessionStart");
            Names("fifth");
            Ev("SomethingNew");
            Ev("Interrupt");
            reads.Delete(Index);
            Ev("Stop");
            Names("sixth");
            reads.Codex(sid, "Stop", null, b - 5);
            Ev("SessionEnd");
            Ev("PreCompact", Obj(("trigger", "auto")));
            // a helper's events are the chat's too
            Names("seventh");
            Ev("UserPromptSubmit", SubAgent("019a0000-0000-7000-8000-00000000a1a1", Obj(("prompt", "look"))));
        }

        // the last 256 KB
        {
            string early = CodexSid(500), late = CodexSid(501), edge = CodexSid(502), past = CodexSid(503), wide = CodexSid(504);
            double b = window.T0 - 100;
            string filler = IndexLine(CodexSid(599), new string('x', 1000));
            window.Write(Index, Part.T(IndexLine(early, "too early")), Part.T(filler, Tail / filler.Length + 2), Part.T(IndexLine(late, "late enough")));
            window.Codex(early, "SessionStart", null, b += 0.01);
            window.Codex(late, "SessionStart", null, b += 0.01);
            // a line whose last part begins the window exactly: that part is a line of its own there; a byte on, not
            string part = $"{{\"id\":{J(edge)},\"thread_name\":\"from the window's edge\"}}\n";
            int pad = Tail - Encoding.UTF8.GetByteCount(part) - 1;
            window.Write(Index, Part.T("{\"garbage\":\"xx "), Part.T(part), Part.T("y", pad), Part.T("\n"));
            window.Codex(edge, "SessionStart", null, b += 0.01);
            string pastPart = $"{{\"id\":{J(past)},\"thread_name\":\"a byte past the edge\"}}\n";
            window.Write(Index, Part.T("{\"garbage\":\"xx "), Part.T(pastPart), Part.T("y", Tail - Encoding.UTF8.GetByteCount(pastPart)), Part.T("\n"));
            window.Codex(past, "SessionStart", null, b += 0.01);
            // UTF-16, by its byte order mark
            window.Write(Index, Part.H(new byte[] { 0xFF, 0xFE }.Concat(Encoding.Unicode.GetBytes(IndexLine(wide, "utf-16 name"))).ToArray()));
            window.Codex(wide, "SessionStart", null, b += 0.01);
            window.Write(Index, Part.H(new byte[] { 0xFE, 0xFF }.Concat(Encoding.BigEndianUnicode.GetBytes(IndexLine(wide, "utf-16 be name"))).ToArray()));
            window.Codex(wide, "Stop", null, b += 0.01);
        }
    }

    // ------------------------------------------------------------------ Codex: cwd and threads
    /// cwd (the \\?\ prefix dropped) and has_transcript; the chat's own thread, another thread reporting under its
    /// session (another transcript), and sub-agents.
    static void CodexCwdAndThreads(Replay r)
    {
        string sid = CodexSid(800), own = Files + "/rollout-2026-09-29T10-00-00-" + sid.ToUpperInvariant() + ".jsonl";
        string other = Files + "/rollout-2026-09-29T10-00-01-helper.jsonl";
        double t = r.T0 - 100;
        string turn1 = V7(t - 50, 1), turn2 = V7(t - 40, 2), other1 = V7(t - 90, 3), other2 = V7(t - 30, 4), other3 = V7(t - 20, 5);
        string Ev(string ev, string turn, JsonObject extra = null) => r.Codex(sid, ev, turn, t += 0.01, extra);
        JsonObject Other(JsonObject extra = null) => (extra ?? new JsonObject()).Also(o => o["transcript_path"] = other);

        Ev("SessionStart", null, Obj(("cwd", @"\\?\C:\src\app"), ("transcript_path", own)));
        Ev("UserPromptSubmit", turn1, Obj(("cwd", ""), ("prompt", "go")));
        Ev("UserPromptSubmit", other1, Other(Obj(("prompt", "name the chat"))));
        Ev("PreToolUse", turn1, Bash("ls", "call_1").Also(o => o["cwd"] = 42));
        Ev("Stop", other1, Other());
        Ev("PostToolUse", turn1, Bash("ls", "call_1").Also(o => o["cwd"] = @"\\?\"));
        Ev("PreToolUse", other1, Other(Bash("pwd", "call_2")));
        Ev("Stop", turn1, Obj(("cwd", "/home/u/x"), ("transcript_path", "")));
        Ev("UserPromptSubmit", other2, Other(Obj(("prompt", "again"))));
        Ev("PreToolUse", turn1, Bash("ls", "call_3").Also(o => o["agent_id"] = ""));
        Ev("UserPromptSubmit", turn2, Obj(("cwd", @"\\?\UNC\server\share\p"), ("prompt", "next")));
        Ev("UserPromptSubmit", other2, Other(Obj(("prompt", "again"))));
        Ev("PreToolUse", other3, Other(Bash("date", "call_4")));
        Ev("PreToolUse", turn2, Bash("ls", "call_5").Also(o => { o["cwd"] = @"\\.\C:\x"; o["agent_id"] = 7; }));
        Ev("PreToolUse", turn2, Bash("ls", "call_6").Also(o => { o["cwd"] = @"\\?\\\?\x"; o["transcript_path"] = own.ToLowerInvariant(); }));
        // a sub-agent names its own thread, whatever transcript it reports
        Ev("PreToolUse", other1, SubAgent("019a0000-0000-7000-8000-00000000b2b2", Other(Bash("ls", "sub_1"))));
        Ev("SubagentStop", other1, SubAgent("019a0000-0000-7000-8000-00000000b2b2", Other()));
        Ev("PreToolUse", other1, SubAgent("019a0000-0000-7000-8000-00000000b2b2", Other(Bash("ls", "sub_2"))));
        Ev("Stop", turn2);
        // the chat's own transcript is one whose path has its id, ignoring case as .NET's ordinal casing does (simple
        // upper-case mappings, surrogate pairs too, but ı and ſ kept); else it's another thread's, stale while the
        // chat's turn has ended
        foreach (var (id, path) in new[]
        {
            ("chat-é-ω-𐐨-ǆ-a", "rollout-CHAT-É-Ω-𐐀-ǅ-A.jsonl"), ("chat-i-s", "rollout-CHAT-ı-ſ.jsonl"),
            ("chat-k-ß", "rollout-CHAT-K-ẞ.jsonl"),
        })
        {
            string turn = V7(t - 10, 20), next = V7(t - 5, 21);
            r.Codex(id, "UserPromptSubmit", turn, t += 0.01, Obj(("prompt", "go")));
            r.Codex(id, "Stop", turn, t += 0.01);
            r.Codex(id, "UserPromptSubmit", next, t += 0.01, Obj(("transcript_path", Files + "/" + path)));
        }
        // a chat without a transcript
        r.Codex(CodexSid(801), "SessionStart", null, t += 0.01, Obj(("transcript_path", ""), ("cwd", "\\\\?\\")));
        r.Codex(CodexSid(801), "UserPromptSubmit", null, t += 0.01, Obj(("transcript_path", 5)));
        r.Codex(CodexSid(801), "Stop", null, t += 0.01, Obj(("transcript_path", Files + "/x.jsonl")));
    }

    // ------------------------------------------------------------------ Codex: the log line's lag
    /// `lag=`: sent less at, in ms, as .NET's "0" format writes it (15 digits, then half away from zero; a negative
    /// zero keeps its sign), or "?" without a positive sent. Each a chat of its own.
    static void CodexLag(Replay r)
    {
        double b = r.T0 - 100;
        int n = 0;
        string B(double d) => J(d);
        var pairs = new (string At, string Sent)[]
        {
            (B(b), B(b + 0.01)), (B(b), null), (B(b), "0"), (B(b), "-5"), (B(b), "\"soon\""), (B(b), "true"), (B(b), "null"),
            (B(b), B(b - 0.0004)), (B(b), B(b - 0.0006)), (B(b), B(b + 2.5)), (B(b), B(b + 0.0005)), (B(b), B(b - 10)),
            ("0", "0.0125"), ("0", "0.0005"), ("0", "0.0015"), ("0", "0.0025"), ("0", "0.0004"), ("1", "1.0125"), ("1", "1e300"),
            ("0", "0.9995"), ("0", "9.9995"), ("-1.5", "2.5"), (B(b), "9007199254740993"), (B(b), "1e-300"), ("0", "123456789012.3456"),
            (null, B(b)), ("\"x\"", B(b)), ("null", B(b)),
        };
        foreach (var (at, sent) in pairs)
        {
            var line = new StringBuilder("{\"v\":1,\"type\":\"event\",\"agent\":\"codex\"");
            if (at != null) line.Append(",\"at\":").Append(at);
            line.Append(",\"pid\":4242,\"payload\":").Append(CodexPayload(CodexSid(900 + n++), "SessionStart").ToJsonString(Json));
            if (sent != null) line.Append(",\"sent\":").Append(sent);
            r.Apply(line.Append('}').ToString());
        }
    }

    // ------------------------------------------------------------------ Codex: malformed envelopes
    /// Envelopes and payloads of the wrong shape, each on a chat of its own where it makes one.
    static void CodexMalformed(Replay r)
    {
        double t = r.T0 - 100;
        int n = 0;
        JsonObject Env(string ev = "SessionStart") => CodexEnvelope(t += 0.01, CodexPayload(CodexSid(1000 + n++), ev));
        void Take(JsonObject envelope) => r.Apply(envelope);

        Take(Env().Also(o => o.Remove(Ipc.Payload)));
        Take(Env().Also(o => o[Ipc.Payload] = new JsonArray(1, 2)));
        Take(Env().Also(o => o[Ipc.Payload] = "SessionStart"));
        Take(Env().Also(o => ((JsonObject)o[Ipc.Payload]).Remove("session_id")));
        Take(Env().Also(o => o[Ipc.Payload]["session_id"] = ""));
        Take(Env("Stop").Also(o => o[Ipc.Payload]["session_id"] = 99));
        Take(Env().Also(o => o[Ipc.Payload]["session_id"] = null));
        Take(Env().Also(o => o[Ipc.Payload]["hook_event_name"] = 12));
        Take(Env().Also(o => ((JsonObject)o[Ipc.Payload]).Remove("hook_event_name")));
        foreach (var turn in new JsonNode[] { "", 42, null, true })
            Take(Env("UserPromptSubmit").Also(o => o[Ipc.Payload]["turn_id"] = turn?.DeepClone()));
        Take(Env("PreToolUse").Also(o =>
        {
            var p = o[Ipc.Payload];
            p["turn_id"] = V7(t - 60, 1);
            p["tool_name"] = "shell";
            p["tool_input"] = "ls";
            p["tool_use_id"] = 5;
        }));
        Take(Env("PreToolUse").Also(o =>
        {
            var p = o[Ipc.Payload];
            p["turn_id"] = V7(t - 60, 1);
            p["tool_name"] = "shell";
            p["tool_input"] = new JsonArray("ls");
            p["tool_use_id"] = "";
        }));
        Take(Env("PreToolUse").Also(o => o[Ipc.Payload]["tool_name"] = 3));
        Take(Env().Also(o => o.Remove(Ipc.At)));
        Take(Env().Also(o => o[Ipc.At] = "soon"));
        Take(Env().Also(o => o[Ipc.At] = null));
        Take(Env().Also(o => o[Ipc.At] = -1.5));
        foreach (var pid in new JsonNode[] { null, "4242", 42.9, -42.9, 1e20 })
            Take(Env().Also(o => { if (pid == null) o.Remove(Ipc.Pid); else o[Ipc.Pid] = pid.DeepClone(); }));
        Take(Env().Also(o => o[Ipc.V] = 2));
        Take(Env().Also(o => o[Ipc.Kind] = "ping"));
        Take(Env().Also(o => o[Ipc.Env] = Obj(("CLAUDE_CODE_ENTRYPOINT", "cli"))));
        Take(Env("Stop\n"));
        Take(Env("Stop "));
        Take(Env("stop"));
        Take(Env(new string('E', 59) + "😀x"));
        Take(Env(new string('E', 100)));
        Take(Env("SessionEnd"));
        foreach (var sid in new[] { "abcdefghijk😀tail", "abcdefghijkl😀tail", "tab\there\u0007bell", "é", new string('s', 13), new string('s', 14) })
            Take(Env().Also(o => o[Ipc.Payload]["session_id"] = sid));
        Take(Env().Also(o => o[Ipc.Payload]["transcript_path"] = 5));
        Take(Env().Also(o => o[Ipc.Payload]["agent_id"] = 7));
    }

    // ------------------------------------------------------------------ Codex: turn ids
    /// Which turn came first: v7 ids by their text, ignoring case, until the current one's time is well past now; other
    /// ids only by the last 16 seen, as written; the forms .NET's Guid parser takes, which Convert can then refuse.
    static void CodexTurnIds(Replay r)
    {
        double b = r.T0 - 100;
        string sid = null;
        string Ev(string ev, string turn, double at, JsonObject extra = null) => r.Codex(sid, ev, turn, at, extra);

        sid = CodexSid(1100);
        string t1 = V7(b - 60, 0xabc), t2 = V7(b - 50, 0xabd), t3 = V7(b - 40, 0xabe);
        Ev("UserPromptSubmit", t2, b);
        Ev("PreToolUse", t1.ToUpperInvariant(), b + 1, Bash("ls", "c1"));
        Ev("UserPromptSubmit", t3.ToUpperInvariant(), b + 2);
        Ev("PreToolUse", t2, b + 3, Bash("ls", "c2"));
        Ev("PreToolUse", t2.ToUpperInvariant(), b + 4, Bash("ls", "c3"));
        Ev("Stop", t3, b + 5);
        Ev("PreToolUse", t3.ToUpperInvariant(), b + 6, Bash("ls", "c4"));

        sid = CodexSid(1101);
        var ids = Enumerable.Range(0, 18).Select(i => $"turn-{i:D2}").ToArray();
        for (int i = 0; i < ids.Length; i++) Ev("UserPromptSubmit", ids[i], b + 10 + i);
        Ev("PreToolUse", ids[1], b + 30, Bash("ls", "c5"));
        Ev("PreToolUse", ids[0], b + 31, Bash("ls", "c6"));
        Ev("PreToolUse", ids[17], b + 32, Bash("ls", "c7"));
        Ev("PreToolUse", "TURN-02", b + 33, Bash("ls", "c8"));
        Ev("PreToolUse", "TURN-02", b + 34, Bash("ls", "c8"));

        sid = CodexSid(1102);
        Ev("UserPromptSubmit", V7(b - 30, 5), b + 40);
        Ev("UserPromptSubmit", "3f2504e0-4f89-41d3-9a0c-0305e82c3301", b + 41);
        Ev("UserPromptSubmit", V7(b - 60, 6), b + 42);
        Ev("UserPromptSubmit", V7(b - 61, 7), b + 43);
        Ev("UserPromptSubmit", "{" + V7(b - 20, 8) + "}", b + 44);
        Ev("UserPromptSubmit", V7(b - 70, 9), b + 45);
        Ev("UserPromptSubmit", V7(b - 19, 10)[..35] + "g", b + 46);
        Ev("UserPromptSubmit", V7(b - 90, 11), b + 47);
        Ev("UserPromptSubmit", V7(b - 18, 12)[..35] + "é", b + 48);
        Ev("UserPromptSubmit", V7(b - 95, 13), b + 49);
        Ev("UserPromptSubmit", V7(b - 17, 14).Replace('-', '_'), b + 50);

        // two of the same millisecond: their other bits decide
        sid = CodexSid(1103);
        Ev("UserPromptSubmit", V7(b - 10, 0x300), b + 60);
        Ev("UserPromptSubmit", V7(b - 10, 0x200), b + 61);
        Ev("UserPromptSubmit", V7(b - 10, 0x400), b + 62);

        // .NET's Guid parser takes a + and a 0x at a group's start: Convert reads the first group so, and throws on
        // the second
        sid = CodexSid(1104);
        Ev("UserPromptSubmit", "+0x19a2b-3c4d-7e6f-8000-000000000001", b + 70);
        Ev("UserPromptSubmit", V7(b - 10, 1), b + 71);
        Ev("UserPromptSubmit", "0X19a2b3-4d5e-7f60-8000-000000000001", b + 72);
        Ev("UserPromptSubmit", V7(b - 9, 2), b + 73);
        sid = CodexSid(1105);
        string second = "019a2b3c-0x4d-7e6f-8000-000000000001";
        Ev("UserPromptSubmit", second, b + 80);
        Ev("PreToolUse", second, b + 81, Bash("ls", "c9"));
        Ev("UserPromptSubmit", "3f2504e0-4f89-41d3-9a0c-0305e82c3301", b + 82);
        Ev("UserPromptSubmit", second, b + 83);
        sid = CodexSid(1106);
        Ev("UserPromptSubmit", second, b + 90);
        Ev("UserPromptSubmit", V7(b - 10, 3), b + 91);
        Ev("PreToolUse", V7(b - 10, 3), b + 92, Bash("ls", "c10"));
        Ev("PreToolUse", second, b + 93, Bash("ls", "c11"));
        Ev("Stop", second, b + 94);
        Ev("UserPromptSubmit", "019A2B3C-+4D5-7E6F-8000-000000000001", b + 95);
        Ev("UserPromptSubmit", V7(b - 8, 4), b + 96);
        // a new thread's first turn has nothing to be compared with
        Ev("PreToolUse", V7(b - 8, 5), b + 97, SubAgent("019a0000-0000-7000-8000-00000000c3c3", Bash("ls", "c12")));
    }

    // ------------------------------------------------------------------ Codex: tool calls
    /// The last 64 calls of a turn; calls by id, and without; a request waiting 5 s for its call's PreToolUse; the
    /// asking call's PostToolUse ending a rerun's wait; whole inputs as what a call runs; a sub-agent's calls.
    static void CodexCalls(Replay r)
    {
        string sid = CodexSid(1200);
        double b = r.T0 - 100;
        string turn = V7(b - 60, 1);
        string Ev(string ev, double at, JsonObject extra = null) => r.Codex(sid, ev, turn, at, extra);
        JsonObject Web(string input, string id = null) => Tool("web_search", input).Also(o => { if (id != null) o["tool_use_id"] = id; });

        Ev("UserPromptSubmit", b);
        for (int i = 0; i <= 64; i++) Ev("PreToolUse", b + 1 + i * 0.01, Bash($"cmd {i}", $"call_{i:D2}"));
        Ev("PostToolUse", b + 2, Bash("cmd 0", "call_00"));
        Ev("PreToolUse", b + 2.1, Bash("cmd 0", "call_00"));
        Ev("PostToolUse", b + 2.2, Bash("cmd 1", "call_01"));
        Ev("PreToolUse", b + 2.3, Bash("cmd 1", "call_01"));
        Ev("PermissionRequest", b + 2.4, Bash("cmd 2", "call_02"));
        Ev("PreToolUse", b + 2.5, Bash("cmd 2", "call_02"));
        // without ids
        Ev("PreToolUse", b + 3, Bash("make"));
        Ev("PermissionRequest", b + 3.1, Bash("make"));
        Ev("PostToolUse", b + 3.2, Bash("make"));
        Ev("PreToolUse", b + 3.3, Bash("make"));
        Ev("PreToolUse", b + 3.4, Bash("make"));
        // a request waits for its PreToolUse 5 s, and no more
        Ev("PermissionRequest", b + 10, Bash("deploy"));
        Ev("PreToolUse", b + 15, Bash("deploy", "call_d1"));
        Ev("PermissionRequest", b + 20, Bash("deploy 2"));
        Ev("PreToolUse", b + 25.001, Bash("deploy 2", "call_d2"));
        Ev("PreToolUse", b + 24, Bash("deploy 3", "call_d3"));
        Ev("PermissionRequest", b + 29, Bash("deploy 3"));
        Ev("PreToolUse", b + 30, Bash("deploy 4", "call_d4"));
        Ev("PermissionRequest", b + 35.001, Bash("deploy 4"));
        // the asking call's PostToolUse ends the rerun's wait; a PostToolUse without its id takes the wait instead
        Ev("PreToolUse", b + 40, Bash("npm test", "call_n1"));
        Ev("PermissionRequest", b + 40.5, Bash("npm test"));
        Ev("PostToolUse", b + 41, Bash("npm test", "call_n1"));
        Ev("PreToolUse", b + 42, Bash("npm test", "call_n2"));
        Ev("PreToolUse", b + 50, Bash("npm run", "call_r1"));
        Ev("PermissionRequest", b + 50.5, Bash("npm run"));
        Ev("PostToolUse", b + 51, Bash("npm run"));
        Ev("PreToolUse", b + 52, Bash("npm run", "call_r2"));
        // a request goes with the latest PreToolUse of its call, whichever is closer
        Ev("PreToolUse", b + 60, Bash("cargo test", "call_c1"));
        Ev("PreToolUse", b + 63, Bash("cargo test", "call_c2"));
        Ev("PermissionRequest", b + 60.5, Bash("cargo test"));
        Ev("PostToolUse", b + 64, Bash("cargo test", "call_c1"));
        Ev("PreToolUse", b + 64.5, Bash("cargo test", "call_c1"));
        Ev("PreToolUse", b + 64.6, Bash("cargo test", "call_c2"));
        // whole inputs, as System.Text.Json writes them
        Ev("PreToolUse", b + 70, Web("{\"query\":\"a\",\"n\":1.50}", "ws1"));
        Ev("PermissionRequest", b + 70.5, Web("{\"query\":\"a\",\"n\":1.5}"));
        Ev("PermissionRequest", b + 70.6, Web("{\"query\":\"a\",\"n\":1.50}"));
        Ev("PreToolUse", b + 71, Web("{\"n\":1.50,\"query\":\"a\"}", "ws2"));
        Ev("PreToolUse", b + 71.1, Web("{\"query\":\"a\",\"n\":1.50}", "ws1"));
        Ev("PreToolUse", b + 72, Tool("apply_patch", J(new { command = "*** Update File: a.rs" })).Also(o => o["tool_use_id"] = "p1"));
        Ev("PermissionRequest", b + 72.5, Tool("apply_patch", J(new { command = "*** Update File: a.rs", justification = "edit" })));
        Ev("PreToolUse", b + 73, Tool("apply_patch", J(new { command = "*** Update File: a.rs" })).Also(o => o["tool_use_id"] = "p1"));
        // two calls of one command whose hooks started in the same millisecond: a request goes with the later
        Ev("PreToolUse", b + 74, Bash("tie", "call_t1"));
        Ev("PreToolUse", b + 74, Bash("tie", "call_t2"));
        Ev("PermissionRequest", b + 74.5, Bash("tie"));
        Ev("PreToolUse", b + 74.6, Bash("tie", "call_t1"));
        Ev("PreToolUse", b + 74.7, Bash("tie", "call_t2"));
        // a sub-agent's calls, in its own thread: taken or not by at
        string agent = "019a0000-0000-7000-8000-00000000b2b2", sub = V7(b - 30, 9);
        r.Codex(sid, "PreToolUse", sub, b + 80, SubAgent(agent, Bash("ls", "s1")));
        r.Codex(sid, "PermissionRequest", sub, b + 79.5, SubAgent(agent, Bash("ls")));
        r.Codex(sid, "PreToolUse", sub, b + 81, SubAgent(agent, Bash("rm x", "s2")));
        r.Codex(sid, "PermissionRequest", sub, b + 80.5, SubAgent(agent, Bash("rm x")));
        r.Codex(sid, "PostToolUse", sub, b + 82, SubAgent(agent, Bash("rm x", "s2")));
        r.Codex(sid, "PreToolUse", sub, b + 83, SubAgent(agent, Bash("rm x", "s2")));
        Ev("Stop", b + 90);
        // the last 64 calls: the chat's latest call, 63 calls (stale by at, recorded all the same) and its request,
        // which goes with it; then 64 calls after it, and the request finds none and goes by at
        foreach (var (n, later) in new[] { (2, 63), (3, 64) })
        {
            string next = V7(b - 60 + n, n);
            // before now, which a time more than 5 s past would say the clock was set back since (the turn's start is
            // taken whatever its time)
            double s = b - 400 + 100 * n;
            r.Codex(sid, "UserPromptSubmit", next, s);
            r.Codex(sid, "PreToolUse", next, s + 50, Bash("asks " + n, "call_a" + n));
            for (int i = 0; i < later; i++) r.Codex(sid, "PreToolUse", next, s + 1 + i * 0.01, Bash($"cmd {n} {i}", $"call_{n}_{i:D2}"));
            r.Codex(sid, "PermissionRequest", next, s + 49.5, Bash("asks " + n));
        }
    }

    // ------------------------------------------------------------------ Codex: a clock set back
    /// Times and turn ids from while the clock was an hour ahead order nothing after it was set right; a tombstone from
    /// then doesn't hold either.
    static void CodexClock(Replay r)
    {
        string sid = CodexSid(1300);
        double t0 = r.T0;
        string ahead = V7(t0 + 3600, 1), now1 = V7(t0 - 50, 2), now2 = V7(t0 - 40, 3), now3 = V7(t0 - 10, 4);
        r.Codex(sid, "SessionStart", null, t0 + 3600);
        r.Codex(sid, "UserPromptSubmit", ahead, t0 + 3600.5, Obj(("prompt", "ahead")));
        r.Codex(sid, "PreToolUse", ahead, t0 + 3601, Bash("ls", "c1"));
        r.Codex(sid, "SessionStart", null, t0 - 60);
        r.Codex(sid, "UserPromptSubmit", now1, t0 - 50, Obj(("prompt", "set right")));
        r.Codex(sid, "PreToolUse", ahead, t0 + 3602, Bash("ls", "c2"));
        r.Codex(sid, "PreToolUse", now1, t0 - 49, Bash("ls", "c3"));
        r.Codex(sid, "UserPromptSubmit", now2, t0 - 40);
        r.Codex(sid, "PreToolUse", now1, t0 - 39, Bash("ls", "c4"));
        r.Codex(sid, "SessionEnd", null, t0 + 7200);
        r.Codex(sid, "Stop", now2, t0 - 30);
        r.Codex(sid, "SessionEnd", null, t0 - 20);
        r.Codex(sid, "PreToolUse", now2, t0 - 25, Bash("ls", "c5"));
        r.Codex(sid, "UserPromptSubmit", now3, t0 - 10);
        r.Codex(sid, "Stop", null, t0 - 11);
    }

    // ------------------------------------------------------------------ Codex: SessionEnd and pruning
    /// A tombstone holds off what started before the end, whatever its turn; a chat taken up again starts from its
    /// event's turn; chats and tombstones not heard of for a day go.
    static void CodexEnd(Replay r)
    {
        double t0 = r.T0;
        string a = CodexSid(1400), b = CodexSid(1401), c = CodexSid(1402), d = CodexSid(1403);
        string turn = V7(t0 - 200, 1), turn2 = V7(t0 - 100, 2);
        r.Codex(a, "SessionStart", null, t0 - 90000);
        r.Codex(b, "SessionEnd", null, t0 - 150);
        r.Codex(c, "UserPromptSubmit", turn, t0 - 150, Obj(("prompt", "one")));
        r.Codex(c, "SessionEnd", null, t0 - 140);
        r.Codex(c, "PreToolUse", turn, t0 - 145, Bash("ls", "c1"));
        r.Codex(c, "Stop", turn, t0 - 130);
        r.Codex(c, "UserPromptSubmit", turn, t0 - 129, Obj(("prompt", "late")));
        r.Codex(c, "PreToolUse", turn, t0 - 128, Bash("ls", "c2"));
        r.Codex(c, "UserPromptSubmit", turn2, t0 - 120, Obj(("prompt", "two")));
        r.Codex(b, "UserPromptSubmit", turn, t0 - 160);
        r.Codex(b, "UserPromptSubmit", turn, t0 - 110);
        r.Codex(d, "SessionStart", null, t0 - 100);
        r.Codex(d, "Stop", null, t0 - 86400 - 200);
        r.Codex(a, "SessionEnd", null, t0 - 90000);
        r.Codex(d, "SomethingNew", null, t0 - 90);
        r.Codex(d, "PreToolUse", null, t0 - 80, Bash("ls"));
    }

    // ------------------------------------------------------------------ Codex: random chats
    /// One to three Codex chats at once, as their hooks would deliver them.
    static void RandomCodexChats(Replay r, int seed)
    {
        var rng = new Random(seed);
        var steps = new List<(double Arrive, int Seq, Action Take)>();
        void Add(double arrive, Action take) => steps.Add((arrive, steps.Count, take));
        // session_index.jsonl's lines, in the order Codex appends them
        var index = new List<string>();
        int chats = 1 + rng.Next(3);
        for (int c = 0; c < chats; c++) RandomCodexChat(r, rng, index, Add);
        foreach (var s in steps.OrderBy(s => s.Arrive).ThenBy(s => s.Seq)) s.Take();
    }

    static void RandomCodexChat(Replay r, Random rng, List<string> index, Action<double, Action> add)
    {
        string sid = new Guid(Enumerable.Range(0, 16).Select(_ => (byte)rng.Next(256)).ToArray()).ToString();
        // on Windows Codex starts each hook through PowerShell, 1-4 s late, and has no PostToolUse hook
        bool windows = rng.Next(2) == 0;
        // well before now, so nothing is near the 5 s by which a time past now tells a clock set back; a chat whose
        // first turns came while the clock was an hour ahead has them, and their turn ids, 3600 s later
        double start = r.T0 - 400 + rng.NextDouble() * 100, t = start;
        double setBack = rng.Next(8) == 0 ? start + 5 + rng.NextDouble() * 20 : double.NegativeInfinity;
        string file = $"rollout-2026-09-29T10-00-00-{sid}.jsonl";
        string transcript = rng.Next(5) > 0 ? Files + "/" + file : null;
        string cwd = Pick(rng, @"\\?\C:\src\app", "/home/u/src/api", @"C:\tmp\scratch");
        int call = 0;

        void Emit(string ev, string turn, double when, JsonObject extra = null)
        {
            // the hook starts late, and `at` has ms precision
            double delay = windows ? 1 + rng.NextDouble() * 3 : rng.NextDouble() * 0.02;
            double at = Math.Floor((when + delay) * 1000) / 1000;
            if (when < setBack) at += 3600;
            // most hooks take tens of ms to deliver; a slow one, seconds
            double late = rng.Next(7) == 0 ? 0.2 + rng.NextDouble() * 3 : rng.NextDouble() * 0.08;
            var p = CodexPayload(sid, ev, turn, extra);
            if (transcript != null && !p.ContainsKey("transcript_path")) p["transcript_path"] = transcript;
            p["cwd"] = cwd;
            var envelope = CodexEnvelope(at, p, 1000 + rng.Next(60000));
            envelope[Ipc.Sent] = Math.Floor((at + late) * 1000) / 1000;
            add(when + delay + late, () => r.Apply(envelope));
            // delivered twice
            if (rng.Next(25) == 0)
            {
                var again = (JsonObject)envelope.DeepClone();
                add(when + delay + late + rng.NextDouble() * 2, () => r.Apply(again));
            }
        }
        string Turn(double when) => V7(when < setBack ? when + 3600 : when, rng.Next(0x1000));

        if (transcript != null)
        {
            var originator = Pick(rng, "Codex Desktop", "codex-tui", "codex_exec", "codex_vscode", "");
            // Codex writes the transcript's first line as the chat starts; now and then a hook is quicker
            add(t + (rng.Next(4) == 0 ? 3 : -1), () => r.Write(file, Part.T(MetaLine(originator) + "{\"type\":\"response_item\"}\n")));
        }
        if (rng.Next(3) > 0) Emit("SessionStart", null, t, Obj(("source", "startup")));
        int turns = 1 + rng.Next(3);
        for (int k = 0; k < turns; k++)
        {
            t += 0.5 + rng.NextDouble() * 4;
            string turn = Turn(t);
            Emit("UserPromptSubmit", turn, t, Obj(("prompt", Pick(rng, "fix the build", "/review", "", "explain\nthis", "Rename the helpers"))));
            int calls = rng.Next(5);
            for (int j = 0; j < calls; j++)
            {
                t += 0.05 + rng.NextDouble() * 2;
                var (tool, input) = rng.Next(7) switch
                {
                    0 or 1 or 2 => ("shell", Obj(("command", Pick(rng, "ls", "npm test", "cargo build", "git status", "rm -rf build")))),
                    3 => ("apply_patch", Obj(("command", "*** Begin Patch\n*** Update File: src/" + Pick(rng, "a.rs", "b.rs") + "\n*** End Patch"))),
                    4 => ("web_search", Obj(("query", Pick(rng, "rust hooks", "codex")))),
                    5 => ("view_image", Obj(("path", "/tmp/shot.png"))),
                    _ => ("mcp__atlassian__search", Obj(("query", "PROJ-" + rng.Next(3)))),
                };
                string id = $"call_{call++:D3}";
                JsonObject Call(bool withId = true, string description = null)
                {
                    var i = (JsonObject)input.DeepClone();
                    if (description != null) i["description"] = description;
                    var o = Obj(("tool_name", tool), ("tool_input", i));
                    if (withId) o["tool_use_id"] = id;
                    return o;
                }
                Emit("PreToolUse", turn, t, Call());
                switch (tool == "shell" ? rng.Next(4) : 3)
                {
                    case 0:
                        // it asks: the request's hook starts about when its PreToolUse's does
                        Emit("PermissionRequest", turn, t + rng.NextDouble() * 0.2, Call(false, "run it"));
                        t += 1 + rng.NextDouble() * 6;
                        break;
                    case 1:
                        // it failed in the sandbox: Codex asks to run it again with approval, a second or two apart
                        t += 5 + rng.NextDouble() * 20;
                        Emit("PermissionRequest", turn, t, Call(false, "rerun with approval"));
                        id = $"call_{call++:D3}";
                        t += 1 + rng.NextDouble() * 1.5;
                        Emit("PreToolUse", turn, t, Call());
                        break;
                }
                t += 0.05 + rng.NextDouble() * 3;
                if (!windows) Emit("PostToolUse", turn, t, Call());
            }
            if (rng.Next(4) == 0)
            {
                // a sub-agent, in a thread of its own, while the chat waits or goes on
                string agent = V7(t, rng.Next(0x1000));
                Emit("SubagentStart", turn, t += 0.1, Obj(("agent_type", Pick<JsonNode>(rng, "explorer", "default", null))));
                double s = t + 0.05;
                string subTurn = Turn(s);
                Emit("UserPromptSubmit", subTurn, s, SubAgent(agent, Obj(("prompt", "look around"))));
                for (int j = 1 + rng.Next(3); j > 0; j--)
                {
                    var o = Obj(("tool_name", "shell"), ("tool_input", Obj(("command", Pick(rng, "ls", "rg TODO")))), ("tool_use_id", $"sub_{call++:D3}"));
                    Emit("PreToolUse", subTurn, s += 0.1 + rng.NextDouble(), SubAgent(agent, o));
                    if (!windows) Emit("PostToolUse", subTurn, s += 0.1 + rng.NextDouble(), SubAgent(agent, (JsonObject)o.DeepClone()));
                }
                Emit("SubagentStop", subTurn, s += 0.2, SubAgent(agent));
                t = rng.Next(2) == 0 ? s + 0.1 : t + 0.5;
            }
            if (rng.Next(8) == 0)
            {
                Emit("PreCompact", turn, t += 0.2, Obj(("trigger", "auto")));
                Emit("PostCompact", turn, t += 2, Obj(("trigger", "auto")));
            }
            if (rng.Next(6) == 0)
            {
                // another thread under the session (Codex's own helper naming the chat), with a transcript of its own
                string other = Files + $"/rollout-2026-09-29T10-00-01-helper{rng.Next(1000)}.jsonl", otherTurn = Turn(t + 0.1);
                Emit("UserPromptSubmit", otherTurn, t + 0.1, Obj(("prompt", "name this chat"), ("transcript_path", other)));
                Emit("Stop", otherTurn, t + 0.8, Obj(("transcript_path", other)));
            }
            t += 0.1 + rng.NextDouble();
            Emit(rng.Next(8) == 0 ? "Interrupt" : "Stop", turn, t);
            // Codex names the chat after its first turn, and renames it now and then
            if (k == 0 && rng.Next(2) == 0 || rng.Next(6) == 0)
            {
                var name = Pick(rng, "Fix the build", "Explain the sessions", "", "a very long chat name that goes past the forty four characters");
                add(t + 0.5 + rng.NextDouble() * 5, () =>
                {
                    index.Add(IndexLine(sid, name));
                    r.Write("session_index.jsonl", Part.T(string.Concat(index)));
                });
            }
            t += 3;
        }
        if (rng.Next(6) == 0)
        {
            // /compact: a turn of its own, with no Stop
            string turn = Turn(t += 1);
            Emit("PreCompact", turn, t, Obj(("trigger", "manual")));
            Emit("PostCompact", turn, t += 3, Obj(("trigger", "manual")));
        }
        if (rng.Next(4) == 0)
        {
            Emit("SessionEnd", null, t += 1, Obj(("reason", "exit")));
            if (rng.Next(3) == 0)
            {
                Emit("SessionStart", null, t += 5, Obj(("source", "resume")));
                string turn = Turn(t += 1);
                Emit("UserPromptSubmit", turn, t, Obj(("prompt", "and again")));
                Emit("Stop", turn, t += 2);
            }
        }
    }


    static T Also<T>(this T value, Action<T> change)
    {
        change(value);
        return value;
    }
}

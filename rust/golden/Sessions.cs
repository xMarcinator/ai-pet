using System.Globalization;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AiPet.Golden;

/// Mode sessions: golden data for the Rust AgentSessions (rust/crates/aipet-core/src/sessions). Envelope sequences go
/// through the real AgentSessions.Apply, and what it gives goes to
/// rust/crates/aipet-core/tests/golden/sessions/claude.json, which tests/sessions.rs replays.
///
///   dotnet run --project rust/golden -c Release -- sessions
///
/// A case is a store of its own and a list of steps. A step is an envelope, as the hook writes it and HookServer parses
/// it, with the clock (`now`) it was applied at, and what Apply gave: the outcome, the hook-events.log line and, at
/// chosen steps, Snapshot(). Or it writes or deletes a transcript in the case's folder, which a transcript_path names
/// as "{files}". The cases:
///  - each Claude case of tests/AiPet.Tests/OrderingTests.cs;
///  - the event and tool tables, where and host ids, titles from transcripts (missing, oversized, odd encodings and
///    lines), when titles are read, pruning, pairing and ordering edges (tool inputs whose keys come in another order,
///    whose numbers are spelled otherwise, or whose lines space and escape them otherwise), and malformed envelopes;
///  - seeded random chats: late hooks, a PermissionRequest either side of its PreToolUse, duplicates, SessionEnd and
///    resumes, a clock set back, renamed transcripts. Chance is only in how the inputs were made: they're all written
///    out.
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
        Replay Case(string name, int every = 1, bool osSpecific = false)
        {
            var r = new Replay(root, name, every, osSpecific);
            cases.Add(r);
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

        var output = Path.Combine(repo, "rust", "crates", "aipet-core", "tests", "golden", "sessions", "claude.json");
        var sb = new StringBuilder();
        sb.Append("{\n");
        sb.Append("  \"about\": ").Append(J("Written by rust/golden (dotnet run --project rust/golden -c Release -- sessions) "
            + "with AiPet.Core's AgentSessions; replayed by rust/crates/aipet-core/tests/sessions.rs. Don't edit by hand.")).Append(",\n");
        sb.Append("  \"os\": ").Append(J(OperatingSystem.IsWindows() ? "windows" : OperatingSystem.IsMacOS() ? "macos" : "linux")).Append(",\n");
        sb.Append("  \"cases\": [\n    ").Append(string.Join(",\n    ", cases.Select(c => c.ToJson()))).Append("\n  ]\n");
        sb.Append("}\n");
        Directory.CreateDirectory(Path.GetDirectoryName(output));
        File.WriteAllText(output, sb.ToString(), new UTF8Encoding(false));

        // what the corpus holds, and a check that it holds every kind of outcome
        var outcomes = cases.SelectMany(c => c.Outcomes).GroupBy(o => o).OrderBy(g => g.Key, StringComparer.Ordinal)
            .ToDictionary(g => g.Key, g => g.Count());
        Console.WriteLine($"{cases.Count} cases, {outcomes.Values.Sum()} envelopes; outcomes: "
            + string.Join(" ", outcomes.Select(o => $"{o.Key} {o.Value}")));
        foreach (var o in new[] { "idle", "thinking", "working", "attention", "done", "removed", "ignored", "stale" })
            if (!outcomes.ContainsKey(o)) throw new InvalidOperationException($"no case gives the outcome {o}");
        var random = cases.Where(c => c.Name.StartsWith("random/", StringComparison.Ordinal)).SelectMany(c => c.Outcomes).ToList();
        if (random.Count(o => o == "stale") == 0) throw new InvalidOperationException("no random case has a stale event: pick other seeds");
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

        public string Apply(JsonObject envelope, bool? snapshot = null) => Apply(envelope.ToJsonString(Json), snapshot);

        /// A request line as it's written, which may space and escape its JSON as the hook wouldn't.
        public string Apply(string line, bool? snapshot = null)
        {
            // as HookServer.Answer parses it
            var req = (JsonObject)JsonNode.Parse(line, documentOptions: new JsonDocumentOptions { MaxDepth = Ipc.MaxDepth });
            if (req[Ipc.Payload] is JsonObject p && p["transcript_path"] is JsonValue v && v.TryGetValue(out string path)
                && path.StartsWith(Files, StringComparison.Ordinal))
                p["transcript_path"] = dir + path[Files.Length..];
            double now = FreshMillisecond();
            var (outcome, log) = sessions.Apply(req);
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
            ("nul", new[] { Part.T(TitleLine("a\u0000b")) }),
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

    static T Also<T>(this T value, Action<T> change)
    {
        change(value);
        return value;
    }
}

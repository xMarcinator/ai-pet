using System.Globalization;
using System.IO;
using System.Text;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet;

/// The agent chats as their hooks report them, kept by the running pet alone: every hook run forwards its event
/// (see Ipc, HookServer) and Apply places it. Events run in the background and can finish out of order, so each is
/// placed by when the agent started it: Claude's by `at` (see Stale) and a call's PreToolUse and PermissionRequest
/// by each other (ClaudeStale), Codex's by its own turn and tool ids first (CodexStale). A chat's ts only moves when
/// what it shows changes.
/// Thread-safe: HookServer applies events from its handler threads and Board takes a Snapshot on the UI thread.
/// Files (chat titles, Codex transcripts) are read on the caller's thread before the lock is taken.
public sealed class AgentSessions
{
    /// A chat, keyed "claude:<sid>" or "codex:<sid>". After SessionEnd only Ended and Ts are left, for a day.
    public sealed class Entry
    {
        /// Where: Claude's from the hook's environment, Codex's from the transcript's originator (null until it names one).
        public string Id, Agent, State, Detail, Prop, Title, ChatTitle, Cwd, Where, HostId;
        /// Ts: when the hook of the event behind the last visible change started. Dispatched: its latest event, for Stale.
        public double Ts, Dispatched;
        public int DispatchedRank;
        public double? Ended;
        /// Codex: the chat has a transcript (chats without one are exec runs or Codex's own helper threads).
        public bool HasTranscript;
        /// Codex: how far each of the chat's threads has come, by thread (see CodexStale). Only Apply uses it, under
        /// the lock; a Snapshot's copies share it.
        internal Dictionary<string, Turns> Threads;
        /// Codex: the chat's own current turn and whether its end is in, copied out of Threads for Board, which orders
        /// the hook's report against CodexWatcher's by turn.
        public string Turn;
        public bool TurnEnded;
        /// Claude: its latest PreToolUses and PermissionRequests, to pair a call's two (see ClaudePair). Like Threads,
        /// only Apply uses it and a Snapshot's copies share it.
        internal List<(int What, bool Request, double At, bool Paired)> Asks;
        public Entry Clone() => (Entry)MemberwiseClone();
    }

    readonly object gate = new();
    // in the order the chats first appeared: Board breaks exact ties by it
    readonly OrderedDictionary<string, Entry> chats = new();

    static readonly HashSet<string> ClaudeEvents = new()
    {
        "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest",
        "PermissionDenied", "Notification", "Elicitation", "ElicitationResult", "PreCompact", "PostCompact",
        "SubagentStart", "SubagentStop", "Stop", "StopFailure",
    };
    static readonly HashSet<string> CodexEvents = new()
    {
        "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse", "Stop", "Interrupt",
        "PreCompact", "PostCompact", "SubagentStart", "SubagentStop",
    };

    public List<Entry> Snapshot() { lock (gate) return chats.Values.Select(e => e.Clone()).ToList(); }

    /// Places one hook event (an Ipc envelope). Returns the outcome (the chat's state, "removed" after SessionEnd,
    /// "ignored" or "stale") and a line for hook-events.log, which never holds prompt text.
    public (string Outcome, string Log) Apply(JsonObject envelope)
    {
        var p = envelope[Ipc.Payload] as JsonObject ?? new JsonObject();
        var agent = StrOf(envelope, Ipc.Agent);
        var ev = StrOf(p, "hook_event_name") ?? "";
        double at = envelope[Ipc.At] is JsonValue a && a.TryGetValue(out double d) ? d : Board.Unix;
        var pid = $"pid={(long)Num(envelope, Ipc.Pid)}";
        switch (agent)
        {
            case "claude":
            {
                var sid = StrOf(p, "session_id") ?? "default";
                var key = "claude:" + sid;
                var env = envelope[Ipc.Env] as JsonObject;
                var where = ClaudeWhere(StrOf(env, "CLAUDE_CODE_ENTRYPOINT"));
                // read before the lock, and only when Apply will look at it (null: not read)
                string title = null;
                if (ClaudeEvents.Contains(ev) && (ev is "SessionStart" or "UserPromptSubmit" or "Stop" or "Notification" or "PermissionRequest"
                                                  || Peek(key) is not { ChatTitle.Length: > 0 }))
                    title = ChatTitle(StrOf(p, "transcript_path"));
                string outcome;
                lock (gate) outcome = Claude(key, p, ev, at, Board.Unix, where, env, title);
                return (outcome, $"claude {pid} {Clean(ev)} {Sid13(sid)} where={Clean(where ?? "?")} -> {outcome}");
            }
            case "codex":
            {
                // an older hook's "packaged" and "parent" (which process ran it) are ignored
                var sid = StrOf(p, "session_id");
                string outcome = "ignored", where = null;
                if (!string.IsNullOrEmpty(sid))
                {
                    var key = "codex:" + sid;
                    (bool Read, string Value) originator = default, name = default;
                    if (CodexEvents.Contains(ev))
                    {
                        var e = Peek(key);
                        if (e?.Where == null) originator = (true, Originator(StrOf(p, "transcript_path")));
                        if (ev is "SessionStart" or "UserPromptSubmit" or "Stop" || string.IsNullOrEmpty(e?.ChatTitle)) name = (true, ThreadName(sid));
                    }
                    lock (gate)
                    {
                        outcome = Codex(key, sid, p, ev, at, Board.Unix, originator, name);
                        where = chats.TryGetValue(key, out var s) ? s.Where : null;
                    }
                }
                // lag: from the hook's start to its sending the event (reading stdin, waiting for the pet)
                double sent = Num(envelope, Ipc.Sent);
                var lag = sent > 0 ? ((sent - at) * 1000).ToString("0", CultureInfo.InvariantCulture) + "ms" : "?";
                return (outcome, $"codex {pid} {Clean(ev)} {Sid13(sid)} lag={lag} where={Clean(where ?? "?")} -> {outcome}");
            }
            default:
                return ("ignored", $"{Clean(agent ?? "?")} {pid} {Clean(ev)} -> ignored");
        }
    }

    // ------------------------------------------------------------------ Claude
    string Claude(string key, JsonObject p, string ev, double at, double now, string where, JsonObject env, string title)
    {
        string Str(string k) => StrOf(p, k);
        Prune(now);
        if (ev == "SessionEnd") return End(key, ev, at, now);
        if (!ClaudeEvents.Contains(ev)) return "ignored";

        // an event Claude started before the last one recorded arrived late: it's out of date
        if (chats.TryGetValue(key, out var old) && ClaudeStale(old, p, ev, at, now)) return "stale";
        var (s, isNew) = Open(key);
        // a new chat (or one taken up again after it ended) starts pairing calls from this event
        if (isNew) ClaudePair(s, p, ev, at);
        Stamp(s, ev, at, now);
        var before = (s.State, s.Detail);
        s.Agent = "claude";
        if (Str("cwd") is { Length: > 0 } cwd) s.Cwd = cwd;
        // where the chat runs (for its border colour, and so Stop only presses Escape in the desktop app for its chats)
        s.Where = where;
        // the desktop app's own id for the chat (local_<uuid>): its deep link claude://claude.ai/epitaxy/<id> opens it
        if (StrOf(env, "CLAUDE_CODE_HOST_SESSION_ID") is { Length: > 0 } host && IsHostId(host)) s.HostId = host;
        if (ev is "SessionStart" or "UserPromptSubmit" or "Stop" or "Notification" or "PermissionRequest" || string.IsNullOrEmpty(s.ChatTitle))
            if (title is { Length: > 0 }) s.ChatTitle = title;

        void Set(string state, string detail, string prop = null)
        {
            s.State = state;
            s.Detail = detail;
            s.Prop = prop;
        }

        switch (ev)
        {
            case "SessionStart":
                // also fires after a compaction in the middle of a turn: don't reset a busy chat
                if (isNew || s.State is "idle" or "done") Set("idle", "Ready");
                break;
            case "UserPromptSubmit":
                var prompt = Short((Str("prompt") ?? "").Split('\n')[0], 38);
                if (prompt.Length > 0 && !prompt.StartsWith('/')) s.Title = prompt;
                Set("thinking", "Thinking");
                break;
            case "PreToolUse":
                var (st, detail, prop) = Describe(Str("tool_name") ?? "", p["tool_input"] as JsonObject);
                Set(st, detail, prop);
                break;
            case "PostToolUse":
            case "PostToolUseFailure":
                Set("thinking", "Thinking");
                break;
            case "PermissionRequest":
                Set("attention", "Needs your permission");
                break;
            case "PermissionDenied":
                // auto mode turned a tool call down; Claude carries on without it
                Set("thinking", "A tool call was denied");
                break;
            case "Notification":
                var msg = Str("message") ?? "";
                switch (Str("notification_type") ?? "")
                {
                    case "idle_prompt":
                        if (s.State != "done") Set("idle", "Waiting for you");
                        break;
                    case "permission_prompt":
                        Set("attention", "Needs your permission");
                        break;
                    case "agent_needs_input": case "elicitation_dialog": case "elicitation_url_dialog":
                        Set("attention", "Needs your input");
                        break;
                    case "agent_completed":
                        Set("done", "Done");
                        break;
                    case "auth_success": case "elicitation_complete": case "elicitation_response":
                    case "quota_auto_resume_fired": case "quota_auto_resume_stale": case "quota_auto_resume_disabled":
                        break;  // nothing to show
                    default:
                        // older Claude versions send no type: go by the message
                        if (msg.Contains("waiting for your input", StringComparison.OrdinalIgnoreCase))
                        {
                            if (s.State != "done") Set("idle", "Waiting for you");
                        }
                        else
                            Set("attention", msg.Contains("permission", StringComparison.OrdinalIgnoreCase)
                                ? "Needs your permission" : (Short(msg, 40) is { Length: > 0 } m ? m : "Needs your attention"));
                        break;
                }
                break;
            case "Elicitation":
                // an MCP server asks you to fill something in
                Set("attention", "Needs your input");
                break;
            case "ElicitationResult":
            case "SubagentStop":
                Set("thinking", "Thinking");
                break;
            case "PreCompact":
                Set("thinking", "Compacting the conversation");
                break;
            case "PostCompact":
                // /compact runs between turns (no Stop follows); an automatic one is part of a turn
                if (Str("trigger") == "manual") Set("done", "Compacted");
                else Set("thinking", "Thinking");
                break;
            case "SubagentStart":
                Set("working", "Delegating to " + Helper(Str("agent_type")), "laptop");
                break;
            case "Stop":
                Set("done", "Done");
                break;
            case "StopFailure":
                Set("attention", StopError(Str("error_type")));
                break;
        }
        // ts only moves when what the chat shows changes: an idle_prompt on a done chat mustn't show it as just done
        // again (or bring back a bubble the user closed). A ts well past now is from before the clock was set back.
        if (isNew || (s.State, s.Detail) != before || s.Ts > now + 5) s.Ts = at;
        return s.State;
    }

    /// What stopped the turn, from StopFailure's error_type.
    static string StopError(string type) => type switch
    {
        "rate_limit" => "Hit a rate limit",
        "overloaded" => "The service is overloaded",
        "server_error" => "Hit a server error",
        "authentication_failed" or "oauth_org_not_allowed" => "Needs you to sign in again",
        "billing_error" or "account_on_hold" => "Has a billing problem",
        "max_output_tokens" => "Hit the output limit",
        "model_not_found" => "Its model isn't available",
        "invalid_request" => "Hit a request error",
        "cloud_credential_error" => "Has a cloud credentials problem",
        _ => "Hit an error",
    };

    /// "desktop" (the Claude app), "terminal" (the CLI), or the client's own name (e.g. claude-vscode, sdk-ts), from
    /// the hook's CLAUDE_CODE_ENTRYPOINT.
    static string ClaudeWhere(string entrypoint) => entrypoint switch
    {
        "claude-desktop" => "desktop",
        "cli" => "terminal",
        { Length: > 0 } other => other,
        _ => null,
    };

    /// The Claude app's id for a chat, local_<uuid>, and nothing else: it goes into a deep link.
    public static bool IsHostId(string id) =>
        id is { Length: 42 } && id.StartsWith("local_", StringComparison.Ordinal) && Guid.TryParseExact(id[6..], "D", out _);

    /// The chat's name as the app shows it, from the last custom-title entry in the transcript.
    static string ChatTitle(string transcriptPath)
    {
        if (string.IsNullOrEmpty(transcriptPath)) return "";
        try
        {
            using var fs = new FileStream(transcriptPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            fs.Seek(Math.Max(0, fs.Length - 512 * 1024), SeekOrigin.Begin);
            using var reader = new StreamReader(fs, Encoding.UTF8);
            var lines = reader.ReadToEnd().Split('\n');
            for (int i = lines.Length - 1; i >= 0; i--)
            {
                if (!lines[i].Contains("\"custom-title\"")) continue;
                try
                {
                    if (JsonNode.Parse(lines[i])?["customTitle"] is JsonValue v && v.TryGetValue(out string t))
                        return Short(t, 44);
                }
                catch { }  // not a line of the expected shape
            }
        }
        catch { }
        return "";
    }

    // ------------------------------------------------------------------ Codex
    string Codex(string key, string sid, JsonObject p, string ev, double at, double now,
                 (bool Read, string Value) originator, (bool Read, string Value) name)
    {
        string Str(string k) => StrOf(p, k);
        Prune(now);
        if (ev == "SessionEnd") return End(key, ev, at, now);
        if (!CodexEvents.Contains(ev)) return "ignored";

        // an event from before what the chat already has arrived late: it's out of date. A turn's prompt that came
        // after the turn's other events (late) still names the chat, but changes nothing else.
        bool late = false;
        if (chats.TryGetValue(key, out var old) && CodexStale(old, p, ev, at, now, out late) && !late) return "stale";
        var (s, isNew) = Open(key);
        // a new chat (or one taken up again after it ended) starts from this event's turn
        if (isNew) CodexOrder(s, p, ev, at, now, out _);
        var chat = s.Threads?.GetValueOrDefault("");
        (s.Turn, s.TurnEnded) = (chat?.Current, chat?.Ended == true);
        if (!late) Stamp(s, ev, at, now);
        var before = (s.State, s.Detail);
        s.Agent = "codex";
        if (Str("cwd") is { Length: > 0 } cwd) s.Cwd = cwd.StartsWith(@"\\?\", StringComparison.Ordinal) ? cwd[4..] : cwd;
        // chats without a transcript are ephemeral runs or Codex's own helper threads (e.g. naming a chat)
        if (!string.IsNullOrEmpty(Str("transcript_path"))) s.HasTranscript = true;

        // where the chat runs, once its transcript names the app; until then it's plain "Codex"
        if (s.Where == null && originator.Read) s.Where = CodexWhere(originator.Value);
        if (ev is "SessionStart" or "UserPromptSubmit" or "Stop" || string.IsNullOrEmpty(s.ChatTitle))
            if (name.Value is { Length: > 0 } n) s.ChatTitle = Short(n, 44);

        void Set(string state, string detail, string prop = null)
        {
            s.State = state;
            s.Detail = detail;
            s.Prop = prop;
        }

        bool helper = !string.IsNullOrEmpty(Str("agent_id"));  // a sub-agent's event, reported under its parent chat
        switch (ev)
        {
            case "SessionStart":
                // also fires on resume/compact/fork in the middle of a chat: don't reset a busy one
                if (isNew || s.State is "idle" or "done") Set("idle", "Ready");
                break;
            case "UserPromptSubmit":
                if (!helper)
                {
                    var prompt = Short((Str("prompt") ?? "").Split('\n')[0], 38);
                    if (prompt.Length > 0 && !prompt.StartsWith('/')) s.Title = prompt;
                }
                if (late) return "stale";
                Set("thinking", helper ? "Delegating to a helper" : "Thinking");
                break;
            case "PreToolUse":
                var (st, detail, prop) = CodexDescribe(Str("tool_name") ?? "", p["tool_input"] as JsonObject);
                Set(st, detail, prop);
                break;
            case "PermissionRequest":
                Set("attention", "Needs your permission");
                break;
            case "PostToolUse":
                Set("thinking", "Thinking");
                break;
            case "Stop":
                Set("done", "Done");
                break;
            case "Interrupt":
                Set("idle", "Interrupted");
                break;
            // an automatic compaction happens inside a turn, whose Stop still follows; /compact is a turn of its own
            // with no Stop, so its PostCompact ends it (see CodexOrder)
            case "PreCompact":
                Set("thinking", "Compacting the conversation");
                break;
            case "PostCompact":
                if (ManualCompact(p)) Set("done", "Compacted");
                else Set("thinking", "Thinking");
                break;
            case "SubagentStop":
                Set("thinking", "Thinking");
                break;
            case "SubagentStart":
                Set("working", "Delegating to " + Helper(Str("agent_type")), "laptop");
                break;
        }
        // ts only moves when what the chat shows changes (so a closed bubble stays closed); one well past now is from
        // before the clock was set back
        if (isNew || (s.State, s.Detail) != before || s.Ts > now + 5) s.Ts = at;
        return s.State;
    }

    /// Codex's tool names on top of the shared ones (Bash, web_search, update_plan and mcp__* are already there).
    static (string, string, string) CodexDescribe(string tool, JsonObject input)
    {
        switch (tool)
        {
            case "apply_patch":
                var patch = input?["command"] is JsonValue v && v.TryGetValue(out string p) ? p : "";
                var files = Regex.Matches(patch, @"^\*\*\* (?:Update|Add|Delete) File: (.+)$", RegexOptions.Multiline)
                    .Select(m => m.Groups[1].Value.Trim()).Distinct().ToList();
                return ("working", files.Count == 1 ? "Editing " + BaseName(files[0]) : files.Count > 1 ? $"Editing {files.Count} files" : "Editing files", "laptop");
            case "spawn_agent": return ("working", "Delegating to a helper", "laptop");
            case "view_image": return ("working", "Looking at an image", "lens");
        }
        return Describe(tool, input);
    }

    /// "desktop" (ChatGPT/Codex desktop app), "terminal" (the Codex CLI), "exec" (codex exec), or another client, from
    /// the transcript's originator; null when there's none to read (yet).
    static string CodexWhere(string originator) => originator switch
    {
        "Codex Desktop" or "codex_work_desktop" => "desktop",
        "codex-tui" or "codex_cli_rs" => "terminal",
        "codex_exec" => "exec",
        { Length: > 0 } other => other.ToLowerInvariant(),
        _ => null,
    };

    /// session_meta.originator from the first line of the chat's transcript ("Codex Desktop", "codex-tui", "codex_exec").
    /// That line also holds the model instructions, so only the start of the file is read.
    static string Originator(string transcriptPath)
    {
        if (string.IsNullOrEmpty(transcriptPath)) return null;
        try
        {
            using var fs = new FileStream(transcriptPath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            var buf = new byte[Math.Min(fs.Length, 4 * 1024 * 1024)];
            int n = fs.Read(buf, 0, buf.Length);
            var text = Encoding.UTF8.GetString(buf, 0, n);
            int nl = text.IndexOf('\n');
            var first = nl >= 0 ? text[..nl] : text;
            if (!first.Contains("\"session_meta\"")) return null;
            var m = Regex.Match(first, "\"originator\"\\s*:\\s*\"([^\"]*)\"");
            return m.Success ? m.Groups[1].Value : null;
        }
        catch { return null; }
    }

    /// The chat's name as Codex shows it, from $CODEX_HOME/session_index.jsonl ({id, thread_name, updated_at} per line;
    /// Codex names a chat after its first turn). The pet's CODEX_HOME, which is the agent's unless one of them set it.
    static string ThreadName(string sid)
    {
        try
        {
            var path = Path.Combine(Paths.CodexHome, "session_index.jsonl");
            using var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            fs.Seek(Math.Max(0, fs.Length - 256 * 1024), SeekOrigin.Begin);
            using var reader = new StreamReader(fs, Encoding.UTF8);
            var lines = reader.ReadToEnd().Split('\n');
            for (int i = lines.Length - 1; i >= 0; i--)
            {
                if (!lines[i].Contains(sid)) continue;
                try
                {
                    if (JsonNode.Parse(lines[i]) is JsonObject o && (string)o["id"] == sid && o["thread_name"] is JsonValue v && v.TryGetValue(out string name))
                        return name;
                }
                catch { }
            }
        }
        catch { }
        return null;
    }

    // ------------------------------------------------------------------ both agents
    /// A chat's entry, or a fresh one for a new chat or one taken up again after it ended.
    (Entry, bool IsNew) Open(string key)
    {
        if (chats.TryGetValue(key, out var s) && s.Ended == null) return (s, false);
        chats[key] = s = new Entry { Id = key, State = "idle", Detail = "Ready", Title = "" };
        return (s, true);
    }

    /// SessionEnd: the chat goes, leaving {ended, ts} for a day, so an event from before the end that finishes late
    /// can't bring it back.
    string End(string key, string ev, double at, double now)
    {
        if (chats.TryGetValue(key, out var last) && Stale(last, ev, at, now)) return "stale";
        chats[key] = new Entry { Id = key, Ended = at, Ts = at };
        return "removed";
    }

    /// Chats not heard of for a day go.
    void Prune(double now)
    {
        foreach (var (key, e) in chats.ToList())
            if (now - e.Ts > 86400) chats.Remove(key);
    }

    /// A copy of the chat's live entry, to decide which files to read before taking the lock; null for none, one that
    /// ended, or one Prune is about to drop.
    Entry Peek(string key)
    {
        lock (gate) return chats.TryGetValue(key, out var e) && e.Ended == null && Board.Unix - e.Ts <= 86400 ? e.Clone() : null;
    }

    // ------------------------------------------------------------------ ordering events
    /// How far into a turn an event comes, to order events whose hooks started in the same millisecond. Only a
    /// rough guide: a plugin install starts Claude's hooks through bash, sh and a wrapper, a few ms late at random
    /// (tens on Git Bash), so the pair it matters most for, a call's PreToolUse and PermissionRequest, is paired by
    /// ClaudePair instead. A wider tie would make a quick next PreToolUse stale after a PostToolUse.
    static int Rank(string ev) => ev switch
    {
        "SessionStart" => 0,
        "UserPromptSubmit" => 1,
        "PreToolUse" or "PreCompact" or "SubagentStart" => 2,
        "PostToolUse" or "PostToolUseFailure" or "PostCompact" or "SubagentStop" => 3,
        _ => 4,  // PermissionRequest, PermissionDenied, Notification, Elicitation(Result), Stop, StopFailure, Interrupt, SessionEnd
    };

    /// True when an event the agent started at `at` is older than what the chat's entry already has, i.e. it finished
    /// late: started earlier, or in the same tick but earlier in a turn, or before the chat ended. A recorded time
    /// well past `now` means the clock was set back since: it can't order anything.
    static bool Stale(Entry s, string ev, double at, double now)
    {
        if (s.Ended is { } ended) return ended <= now + 5 && at <= ended + 0.0005;
        double last = s.Dispatched;
        if (last > now + 5) return false;
        return at < last - 0.0005 || (at <= last + 0.0005 && Rank(ev) < s.DispatchedRank);
    }

    /// Record the event as the chat's latest, for Stale. One taken although it started before the latest (a
    /// PermissionRequest paired with its PreToolUse, or one in the same tick) comes after it: the latest stays, so
    /// what started between the two is still stale. Not when the clock was set back since the latest.
    static void Stamp(Entry s, string ev, double at, double now)
    {
        if (at < s.Dispatched && s.Dispatched <= now + 5)
        {
            s.DispatchedRank = Math.Max(s.DispatchedRank, Rank(ev));
            return;
        }
        s.Dispatched = at;
        s.DispatchedRank = Rank(ev);
    }

    /// Claude's order: by `at` (Stale), except for a call's PreToolUse and PermissionRequest (ClaudePair).
    static bool ClaudeStale(Entry s, JsonObject p, string ev, double at, double now) =>
        (s.Ended == null ? ClaudePair(s, p, ev, at) : null) ?? Stale(s, ev, at, now);

    /// How far apart the hooks of one Claude call's PreToolUse and PermissionRequest can start, in seconds.
    const double PairWindow = 1;

    /// Claude dispatches a call's PreToolUse and then its PermissionRequest a few ms apart at most, and their hooks
    /// start a few ms late at random (see Rank), so `at` can put them either way round. PermissionRequest has no
    /// tool_use_id, so the two are paired by tool and command (CallKey), the closest within PairWindow:
    ///  - a PreToolUse whose PermissionRequest is in already is stale (true);
    ///  - a PermissionRequest is taken (false) when its own PreToolUse is the chat's latest event: nothing else is newer.
    /// Anything else goes by `at` (null). Each half pairs once, so the same command run again soon after isn't taken
    /// for the earlier call. Recorded even when its time then finds it stale, like CodexOrder.
    static bool? ClaudePair(Entry s, JsonObject p, string ev, double at)
    {
        bool request = ev == "PermissionRequest";
        if (!request && ev != "PreToolUse") return null;
        int what = CallKey(p);
        var asks = s.Asks ??= new();
        int i = -1;
        for (int j = 0; j < asks.Count; j++)
            if (asks[j].What == what && asks[j].Request != request && !asks[j].Paired && Math.Abs(asks[j].At - at) <= PairWindow
                && (i < 0 || Math.Abs(asks[j].At - at) < Math.Abs(asks[i].At - at))) i = j;
        if (i < 0)
        {
            asks.Add((what, request, at, false));
            if (asks.Count > 16) asks.RemoveAt(0);
            return null;
        }
        var other = asks[i];
        asks[i] = other with { Paired = true };
        if (!request) return true;
        return other.At == s.Dispatched && s.DispatchedRank == Rank("PreToolUse") ? false : null;
    }

    /// Codex's order. On Windows Codex runs each hook through PowerShell, which starts it 1-4 s late at random, so
    /// neither when an event arrives nor `at` (when its hook started) says which came first. Codex's own ids do, as
    /// far as they go. Turns are followed per thread (ThreadOf): the chat itself, each of its sub-agents, and any other
    /// thread that reports under its session.
    ///  1. An event of a turn before its thread's current one is stale. Turn ids are UUIDv7s, which begin with the
    ///     turn's start time, so they sort even when all of a turn's events come late (ids of another shape only tell
    ///     a turn seen before from a new one; so does a v7 id while the current one's time is well past now, after
    ///     the clock was set back). A turn's UserPromptSubmit is its first event, so once another event of the turn
    ///     is in it came late: it only names the chat.
    ///  2. Nothing of a turn is taken after its end (Stop; SubagentStop for a sub-agent; Interrupt; the PostCompact
    ///     of a /compact, which is a turn of its own with no Stop). Once the chat's own turn has ended, its other
    ///     threads' events are stale too, until its next turn.
    ///  3. Within a turn, a PreToolUse whose call has come further already (its PermissionRequest or PostToolUse is
    ///     in) is stale. Calls are told apart by tool_use_id. PermissionRequest has none, so it goes with the latest
    ///     call of the same tool and command that is still at its PreToolUse and started within HookSpread of it:
    ///     an identical call after the one that asked can only start once the ask is answered. It may still be for
    ///     a rerun whose PreToolUse hasn't come (Codex reruns a command that failed in the sandbox, with approval, a
    ///     second or two later), so it also waits for one on its own, until HookSpread has passed or its call's
    ///     PostToolUse is in. A PermissionRequest whose call's PreToolUse is the chat's latest event is taken
    ///     whatever its time. Left wrong: the same command run again within HookSpread of an approved request, and
    ///     before that call's PostToolUse (none on Windows), shows the chat asking until its next event.
    ///  4. The chat's own turn starting or ending is taken whatever its time: it comes after everything else of the
    ///     chat.
    ///  5. Anything else (SessionStart, which has no turn; one tool call against another; a sub-agent against the
    ///     chat; an automatic compaction) goes by `at` and Rank as for Claude (Stale).
    /// Wrong when another tool's Stop hook makes Codex go on, or a sub-agent works on past its chat's turn: until the
    /// next turn the chat shows done.
    static bool CodexStale(Entry s, JsonObject p, string ev, double at, double now, out bool late)
    {
        late = false;
        return (s.Ended == null ? CodexOrder(s, p, ev, at, now, out late) : null) ?? Stale(s, ev, at, now);
    }

    /// What Codex's ids say of an event (rules 1-4 above): stale (true), taken (false), or nothing (null). Late: a
    /// UserPromptSubmit after its turn's other events (stale, but its prompt still names the chat). What it tells
    /// of its thread is recorded even when its time then finds it stale: it holds either way.
    static bool? CodexOrder(Entry s, JsonObject p, string ev, double at, double now, out bool late)
    {
        late = false;
        var turn = StrOf(p, "turn_id");
        if (string.IsNullOrEmpty(turn)) return null;
        s.Threads ??= new();
        var thread = ThreadOf(p);
        bool own = thread.Length == 0;
        if (!own && s.Threads.TryGetValue("", out var chat) && chat.Ended) return true;
        // a thread's first turn is a new one too
        bool newTurn = !s.Threads.TryGetValue(thread, out var t);
        if (newTurn) s.Threads[thread] = t = new Turns { Current = turn };
        if (turn != t.Current)
        {
            if (t.Before(turn, now)) return true;
            t.Next(turn);
            newTurn = true;
        }
        // its turn's prompt, after another event of the turn (even its end)
        else if (t.Seen && ev == "UserPromptSubmit")
        {
            late = true;
            return true;
        }
        else if (t.Ended) return true;
        t.Seen = true;
        if (ev is "Stop" or "SubagentStop" or "Interrupt" || own && ManualCompact(p))
        {
            t.Ended = true;
            if (!own) return null;
        }
        else
        {
            var call = t.Call(ev, p, at, s.Dispatched);
            if (call == true || !own || !newTurn) return call;
        }
        // rule 4: the chat's own turn starting or ending is its latest event whatever its time (Stamp keeps a later one)
        s.Dispatched = at;
        return false;
    }

    /// A PostCompact of /compact, not of an automatic compaction in the middle of a turn.
    static bool ManualCompact(JsonObject p) =>
        StrOf(p, "hook_event_name") == "PostCompact" && StrOf(p, "trigger") == "manual" && ThreadOf(p).Length == 0;

    /// Which of a chat's threads an event is from: "" for the chat itself, "agent:<id>" for a sub-agent, and
    /// "transcript:<path>" for any other thread reporting under the chat's session (Codex names the chat's own
    /// transcript after the session id).
    static string ThreadOf(JsonObject p)
    {
        if (StrOf(p, "agent_id") is { Length: > 0 } agent) return "agent:" + agent;
        var transcript = StrOf(p, "transcript_path");
        return string.IsNullOrEmpty(transcript) || transcript.Contains(StrOf(p, "session_id") ?? "", StringComparison.OrdinalIgnoreCase)
            ? "" : "transcript:" + transcript;
    }

    /// How far apart, in seconds, the hooks of a Codex call's PreToolUse and PermissionRequest can start: PowerShell
    /// starts each 1-4 s late.
    const double HookSpread = 5;

    /// Codex: one thread of a chat, as far as its events have come (see CodexStale).
    internal sealed class Turns
    {
        public string Current;
        /// The current turn's end is in.
        public bool Ended;
        /// An event of the current turn is in.
        public bool Seen;
        /// The turns before, for ids that don't sort (the latest few).
        readonly List<string> past = new();
        /// The current turn's tool calls: tool_use_id (null for a PermissionRequest whose PreToolUse hasn't come), a
        /// hash of what it runs, how far it has come (1 PreToolUse, 2 PermissionRequest, 3 PostToolUse), and when its
        /// PreToolUse's hook started (else its first event's).
        readonly List<(string Id, int What, int Step, double At)> calls = new();

        /// Whether a turn other than the current one came before it.
        public bool Before(string turn, double now) => past.Contains(turn) || V7Before(turn, Current, now);

        /// A later turn starts.
        public void Next(string turn)
        {
            past.Add(Current);
            if (past.Count > 16) past.RemoveAt(0);
            Current = turn;
            Ended = false;
            Seen = false;
            calls.Clear();
        }

        /// Records a tool event of the current turn (rule 3): true for a PreToolUse whose call has come further
        /// already, false for a PermissionRequest whose call's PreToolUse is the chat's latest event, else null.
        public bool? Call(string ev, JsonObject p, double at, double latest)
        {
            int step = ev switch { "PreToolUse" => 1, "PermissionRequest" => 2, "PostToolUse" => 3, _ => 0 };
            if (step == 0) return null;
            var id = StrOf(p, "tool_use_id") is { Length: > 0 } x ? x : null;
            int what = CallKey(p);
            void Add(string callId, int callStep)
            {
                calls.Add((callId, what, callStep, at));
                if (calls.Count > 64) calls.RemoveAt(0);
            }
            // the call by its id; else one of the same tool and command within HookSpread: a PermissionRequest takes
            // the latest still at its PreToolUse, a PreToolUse or PostToolUse the closest PermissionRequest waiting
            // for its PreToolUse
            int i = id != null ? calls.FindLastIndex(c => c.Id == id) : -1;
            bool byId = i >= 0;
            if (!byId)
                for (int j = 0; j < calls.Count; j++)
                {
                    var c = calls[j];
                    if (c.What != what || Math.Abs(c.At - at) > HookSpread) continue;
                    if (step == 2 ? c.Step == 1 && (i < 0 || c.At >= calls[i].At)
                                  : c.Id == null && (i < 0 || Math.Abs(c.At - at) < Math.Abs(calls[i].At - at))) i = j;
                }
            if (i < 0)
            {
                Add(id, step);
                return null;
            }
            var call = calls[i];
            calls[i] = (call.Id ?? id, call.What, Math.Max(call.Step, step), step == 1 ? at : call.At);
            if (step == 1) return call.Step > 1 ? true : null;
            if (step == 3)
            {
                // the call that asked is done: no rerun waits on its request
                if (byId && call.Step == 2) calls.RemoveAll(c => c.Id == null && c.What == what && c.Step == 2);
                return null;
            }
            // it may be for a rerun whose PreToolUse is still to come: that one then comes late, like its own
            if (!byId) Add(null, 2);
            return call.Step == 1 && call.At == latest ? false : null;
        }
    }

    /// What a tool call runs, as a hash (a command can be 256 Ki characters): its tool and command (PermissionRequest
    /// adds a description to a shell command's input) or file, or else its whole input.
    static int CallKey(JsonObject p)
    {
        var input = p["tool_input"];
        var what = input is JsonObject o && (o["command"] ?? o["file_path"] ?? o["notebook_path"]) is JsonValue v && v.TryGetValue(out string command)
            ? command : input?.ToJsonString();
        return HashCode.Combine(StrOf(p, "tool_name"), what);
    }

    /// A UUIDv7 (xxxxxxxx-xxxx-7xxx-...), whose text sorts by when it was made.
    static bool IsV7(string id) => id is { Length: 36 } && id[14] == '7' && Guid.TryParseExact(id, "D", out _);

    /// Whether UUIDv7 a was made before b. Not when b's time is well past now: the clock was set back since, and ids
    /// made after that sort before it.
    internal static bool V7Before(string a, string b, double now) =>
        IsV7(a) && IsV7(b) && Convert.ToInt64(b[..8] + b[9..13], 16) / 1000.0 <= now + 5 && string.Compare(a, b, StringComparison.OrdinalIgnoreCase) < 0;

    // ------------------------------------------------------------------ describing tool calls
    static string BaseName(string path)
    {
        var b = Path.GetFileName((path ?? "").TrimEnd('/', '\\'));
        return string.IsNullOrEmpty(b) ? "a file" : b;
    }

    static (string State, string Detail, string Prop) Describe(string tool, JsonObject input)
    {
        string In(string k) => input?[k] is JsonValue v && v.TryGetValue(out string s) ? s : null;
        switch (tool)
        {
            case "Bash": case "PowerShell": case "shell": case "local_shell": case "exec_command":
                return ("working", "Running command", "laptop");
            case "Edit": case "MultiEdit": case "Write": case "NotebookEdit":
                return ("working", "Editing " + BaseName(In("file_path") ?? In("notebook_path")), "laptop");
            case "apply_patch": return ("working", "Editing files", "laptop");
            case "Read": return ("working", "Reading " + BaseName(In("file_path")), "lens");
            case "Grep": case "Glob": case "ToolSearch": return ("working", "Searching the code", "lens");
            case "WebFetch": case "WebSearch": case "web_search": return ("working", "Browsing the web", "lens");
            case "Agent": case "Task": case "Workflow": return ("working", "Delegating to a helper", "laptop");
            case "TodoWrite": case "EnterPlanMode": case "update_plan": return ("thinking", "Planning", null);
            case "AskUserQuestion": return ("attention", "Has a question for you", null);
            case "ExitPlanMode": return ("attention", "Plan ready for review", null);
            case "Skill": return ("working", "Using skill " + (In("skill") ?? "").Split(':')[^1], "laptop");
        }
        if (tool.StartsWith("mcp__", StringComparison.Ordinal))
        {
            var parts = tool.Split("__");
            var server = parts.Length > 1 ? parts[1] : "a tool";
            if (server.Contains("Browser") || server.Contains("chrome")) return ("working", "Using the browser", "lens");
            if (server.Length > 20) return ("working", "Using a connector", "laptop");
            return ("working", "Using " + server.Replace('_', ' '), "laptop");
        }
        return ("working", "Using " + tool, "laptop");
    }

    /// "a helper", or "a helper (Explore)" for a named kind of sub-agent.
    static string Helper(string agentType) =>
        string.IsNullOrWhiteSpace(agentType) || agentType is "general-purpose" or "default" ? "a helper" : $"a helper ({agentType})";

    static string Short(string text, int n)
    {
        text = string.Join(' ', (text ?? "").Split((char[])null, StringSplitOptions.RemoveEmptyEntries));
        if (text.Length == 0) return "";
        text = char.ToUpperInvariant(text[0]) + text[1..];
        return text.Length <= n ? text : text[..(n - 1)].TrimEnd() + "…";
    }

    // ------------------------------------------------------------------ JSON and log helpers
    static string StrOf(JsonObject o, string key) => o?[key] is JsonValue v && v.TryGetValue(out string s) ? s : null;

    static double Num(JsonObject o, string key) => o?[key] is JsonValue v && v.TryGetValue(out double d) ? d : 0;

    static string Sid13(string sid) => string.IsNullOrEmpty(sid) ? "-" : Clean(sid.Length > 13 ? sid[..13] : sid);

    /// A value from an event as the log shows it: on one line, and not too long.
    static string Clean(string s) => new(s.Take(60).Select(c => char.IsControl(c) ? '?' : c).ToArray());
}

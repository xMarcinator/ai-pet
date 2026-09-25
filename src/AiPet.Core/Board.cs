namespace AiPet;

/// One bubble: an agent chat, a Jira/GitHub review, the music player, or an error.
public sealed class Session
{
    public string Id, Eff, Name, Detail, Prop, Cwd, Agent = "claude", Kind = "chat", Url, TicketUrl, PrUrl;
    /// Where a chat runs, as the hook saw it: "desktop" (the agent's desktop app), "terminal", another client, or null.
    public string Where;
    /// A chat's deep link into its desktop app (see Board.LinkFor), or null.
    public string Link;
    /// Who reported a chat: "hook", or "log" (Codex's session files, via CodexWatcher).
    public string Source;
    public double Ts;
    public string Section => Board.SectionOf(Kind);
    public Session Clone() => (Session)MemberwiseClone();
}

/// Everything the pet shows, rebuilt a few times a second from the chats the hooks reported plus the watchers.
public sealed class Board
{
    /// How many bubbles each stack shows; the rest are counted in Extra. There is only ever one music bubble.
    public static readonly Dictionary<string, int> MaxCards = new() { ["chats"] = 4, ["reviews"] = 4, ["music"] = 1 };

    public static readonly Dictionary<string, int> Priority = new()
    { ["attention"] = 5, ["review"] = 4, ["working"] = 3, ["thinking"] = 3, ["done"] = 2, ["error"] = 2, ["idle"] = 1, ["music"] = 0, ["paused"] = 0 };

    /// Dot colours (0xAARRGGBB).
    public static readonly Dictionary<string, uint> StatusColor = new()
    {
        ["thinking"] = 0xFF7AA7FF, ["working"] = 0xFFE27A52, ["attention"] = 0xFFFFCF3F, ["done"] = 0xFF5FD38D,
        ["idle"] = 0xFF8A8A90, ["review"] = 0xFF4C9AFF, ["music"] = 0xFF1DB954, ["paused"] = 0xFF5E8F6E,
        ["error"] = 0xFFE5484D, ["sleep"] = 0xFF8A8A90,
    };
    public const uint GitHubPurple = 0xFFA371F7;

    /// Bubbles live in three stacks: the music player (right above the pet, so no chat ever hides it), agent chats
    /// (above it) and reviews (on top).
    public static string SectionOf(string kind) => kind switch
    {
        "jira" or "jira-error" or "github" or "github-error" => "reviews",
        "music" => "music",
        _ => "chats",
    };

    public List<Session> All { get; private set; } = new();
    public List<Session> Cards { get; private set; } = new();
    public Dictionary<string, int> Extra { get; } = new() { ["chats"] = 0, ["reviews"] = 0, ["music"] = 0 };
    /// The pet's mood: sleep, idle, thinking, working, attention, done.
    public string State { get; private set; } = "sleep";
    public string Prop { get; private set; }
    /// Bubbles the user closed, with what they showed then (see Dismiss).
    public readonly Dictionary<string, (double Ts, string Eff, string Detail)> Dismissed = new();

    public static double Unix => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() / 1000.0;

    static string Effective(string st, double ts, double now)
    {
        double age = now - ts;
        if ((st == "thinking" || st == "working") && age > 900) return "idle";
        if (st == "done" && age > 20) return "idle";
        if (st == "attention" && age > 3600) return "idle";
        return st;
    }

    /// The app a chat runs in, for its bubble's border colour and label (0xAARRGGBB).
    public static (string Label, uint Color) AppOf(Session s) => (s.Agent, s.Where) switch
    {
        (_, "terminal") => (s.Agent == "codex" ? "Codex CLI" : "Claude Code CLI", 0xFFA3A3AD),
        ("codex", "exec") => ("codex exec", 0xFFA3A3AD),
        ("codex", "desktop") => ("ChatGPT app", 0xFF10A37F),
        // other clients keep their originator's name, e.g. codex_vscode (the VS Code extension) or codex_sdk_ts
        ("codex", { } other) when other.Contains("vscode") => ("Codex in VS Code", 0xFF3794FF),
        ("codex", _) => ("Codex", 0xFF10A37F),
        (_, "desktop") => ("Claude app", 0xFFD97757),
        (_, "claude-vscode") => ("Claude in VS Code", 0xFF3794FF),
        (_, { } other) when other.StartsWith("sdk") => ("Claude Agent SDK", 0xFFB48CFF),
        _ => ("Claude", 0xFFD97757),
    };

    public static string AgentLabel(string agent) => agent switch { "codex" => "ChatGPT", "jira" => "Jira", "github" => "GitHub", _ => "Claude" };

    public static string PrLabel(string url)
    {
        // https://github.com/Owner/repo/pull/123 -> repo#123
        var parts = url.TrimEnd('/').Split('/');
        return parts.Length >= 4 ? $"{parts[^3]}#{parts[^1]}" : "linked";
    }

    double musicLastPlaying;

    /// Deep link that opens the chat itself in its desktop app, or null (then the app is just brought forward).
    ///   Claude app:   claude://claude.ai/epitaxy/<the app's id for the chat, local_<uuid>>
    ///   ChatGPT app:  codex://threads/<thread id>  (the hook's session id)
    /// Both ids come from the agents' events, so anything but exactly those shapes gets no link.
    static string LinkFor(string agent, string where, string sid, string hostId) => agent switch
    {
        "claude" when where == "desktop" && AgentSessions.IsHostId(hostId) => "claude://claude.ai/epitaxy/" + Uri.EscapeDataString(hostId),
        "codex" when where == "desktop" && sid is { Length: 36 } && Guid.TryParseExact(sid, "D", out _) => "codex://threads/" + sid,
        _ => null,
    };

    /// Chats the hooks reported. Name quality: 2 = the app's own title, 1 = first prompt, 0 = folder.
    static List<(Session S, int NameRank)> Recorded(AgentSessions hooks)
    {
        var list = new List<(Session, int)>();
        foreach (var v in hooks.Snapshot())
        {
            // what's left when a chat ends (so a late event can't bring it back) isn't a chat
            if (v.Ended != null) continue;
            var agent = v.Agent ?? "claude";
            // Codex chats without a transcript are one-off exec runs or Codex's own helper threads
            if (agent == "codex" && !v.HasTranscript) continue;
            string cwd = v.Cwd ?? "";
            var sid = v.Id.Contains(':') ? v.Id[(v.Id.IndexOf(':') + 1)..] : v.Id;
            int rank = !string.IsNullOrEmpty(v.ChatTitle) ? 2 : !string.IsNullOrEmpty(v.Title) ? 1 : 0;
            list.Add((new Session
            {
                Id = v.Id, Ts = v.Ts, Detail = v.Detail ?? "", Prop = v.Prop, Cwd = cwd, Agent = agent,
                Where = v.Where, Eff = v.State ?? "idle", Source = "hook",
                Link = LinkFor(agent, v.Where, sid, v.HostId),
                Name = rank == 2 ? v.ChatTitle : rank == 1 ? v.Title
                    : (Path.GetFileName(cwd.TrimEnd('/', '\\')) is { Length: > 0 } f ? f : AgentLabel(agent)),
            }, rank));
        }
        return list;
    }

    /// Rebuild from the hooks' chats and the watchers.
    public void Refresh(AgentSessions hooks, JiraWatcher jira, GitHubWatcher github, IMediaPlayer media, bool musicOn, CodexWatcher codex = null)
    {
        var recorded = Recorded(hooks);

        // one bubble per chat: the newest report wins (a hook's on a tie), keeping the best name any source has
        var merged = new Dictionary<string, (Session S, int NameRank)>();
        void Offer(Session s, int rank)
        {
            if (!merged.TryGetValue(s.Id, out var cur)) { merged[s.Id] = (s, rank); return; }
            var (win, lose) = s.Ts > cur.S.Ts || (s.Ts == cur.S.Ts && s.Source == "hook" && cur.S.Source != "hook") ? (s, cur.S) : (cur.S, s);
            win.Where ??= lose.Where;
            win.Link ??= lose.Link ?? LinkFor(win.Agent, win.Where, win.Id[(win.Id.IndexOf(':') + 1)..], null);
            if (string.IsNullOrEmpty(win.Cwd)) win.Cwd = lose.Cwd;
            int bestRank = Math.Max(rank, cur.NameRank);
            win.Name = rank >= cur.NameRank ? s.Name : cur.S.Name;
            merged[s.Id] = (win, bestRank);
        }
        foreach (var (s, rank) in recorded) Offer(s, rank);
        foreach (var s in codex?.Sessions ?? Array.Empty<Session>())
        {
            var copy = s.Clone();
            copy.Link = LinkFor(copy.Agent, copy.Where, copy.Id[(copy.Id.IndexOf(':') + 1)..], null);
            // the watcher's name is the app's own title (Codex's index), once the chat has one; else just the folder
            bool titled = !string.IsNullOrEmpty(copy.Name);
            if (!titled) copy.Name = Path.GetFileName((copy.Cwd ?? "").TrimEnd('/', '\\')) is { Length: > 0 } f ? f : "Codex";
            Offer(copy, titled ? 2 : 0);
        }

        double now = Unix;
        var sessions = merged.Values.Select(m => { m.S.Eff = Effective(m.S.Eff, m.S.Ts, now); return m.S; }).ToList();

        // reviews: Jira issues get their PR; PRs that match a Jira review are merged into it
        var jiraKeys = jira.Issues.Select(i => i.Key).ToHashSet(StringComparer.OrdinalIgnoreCase);
        foreach (var i in jira.Issues)
        {
            var pr = github.PrForKey.GetValueOrDefault(i.Key);
            sessions.Add(new Session
            {
                Id = "jira:" + i.Key, Kind = "jira", Agent = "jira", Eff = "review", Ts = i.Updated,
                Url = i.Url, TicketUrl = i.Url, PrUrl = pr,
                Name = $"{i.Key} · {i.Summary}",
                Detail = $"Review requested · {i.Status}" + (pr != null ? " · PR " + PrLabel(pr) : ""),
            });
        }
        foreach (var pr in github.ReviewRequests)
        {
            if (pr.Keys.Any(jiraKeys.Contains)) continue;
            var key = pr.Keys.FirstOrDefault();
            sessions.Add(new Session
            {
                Id = "gh:" + pr.Url, Kind = "github", Agent = "github", Eff = "review", Ts = pr.Updated,
                Url = pr.Url, PrUrl = pr.Url,
                TicketUrl = key != null && !string.IsNullOrWhiteSpace(jira.Config.Site) ? $"https://{jira.Config.Site.Trim()}/browse/{key}" : null,
                Name = $"{pr.Repo.Split('/')[^1]}#{pr.Number} · {pr.Title}",
                Detail = "PR review requested" + (key != null ? " · " + key : ""),
            });
        }

        // music
        if (media?.Playing == true) musicLastPlaying = now;
        if (musicOn && media?.Song != null && (media.Playing || now - musicLastPlaying < 600))
            sessions.Add(new Session
            {
                Id = "music", Kind = "music", Agent = "music", Eff = media.Playing ? "music" : "paused",
                Ts = media.TrackSince, Name = media.Song,
                Detail = (media.Playing ? "♫ " : "Paused · ") + (string.IsNullOrEmpty(media.Artist) ? media.Name : media.Artist),
            });

        if (github.Config.Enabled && github.LastError != null)
            sessions.Add(new Session { Id = "gh:_error", Kind = "github-error", Agent = "github", Eff = "error", Ts = now, Name = "GitHub reviews", Detail = github.LastError + ". Click to fix" });
        if (jira.Config.Enabled && jira.LastError != null)
            sessions.Add(new Session { Id = "jira:_error", Kind = "jira-error", Agent = "jira", Eff = "error", Ts = now, Name = "Jira reviews", Detail = jira.LastError + ". Click to fix" });

        sessions = sessions.OrderByDescending(s => Priority.GetValueOrDefault(s.Eff, 1)).ThenByDescending(s => s.Ts).ToList();
        All = sessions;

        // a dismissed bubble comes back when it does something new: a chat's state or detail changes, or a report comes
        // in well after it (the hook and Codex's log often report the same event a moment apart)
        foreach (var (id, d) in Dismissed.ToList())
            if (sessions.FirstOrDefault(s => s.Id == id) is not { } s0 || s0.Ts > d.Ts + 2
                || s0.Kind == "chat" && (s0.Eff != d.Eff || s0.Detail != d.Detail)) Dismissed.Remove(id);

        var live = sessions.Where(s => s.Eff != "idle" && !Dismissed.ContainsKey(s.Id)).ToList();
        var cards = new List<Session>();
        foreach (var (sec, max) in MaxCards)
        {
            var inSection = live.Where(x => x.Section == sec).ToList();
            cards.AddRange(inSection.Take(max));
            Extra[sec] = Math.Max(0, inSection.Count - max);
        }
        Cards = cards;

        var chats = sessions.Where(s => s.Kind == "chat").ToList();
        double latest = chats.Count > 0 ? chats.Max(s => s.Ts) : 0;
        var primary = chats.FirstOrDefault();
        var st = primary?.Eff ?? "sleep";
        if (st == "idle" && now - latest > 300) st = "sleep";
        State = st;
        Prop = primary?.Prop;
    }

    public Session Find(string id) => All.FirstOrDefault(s => s.Id == id);

    /// Hide a bubble until it does something new (see Refresh).
    public void Dismiss(string id)
    {
        var s = Find(id);
        Dismissed[id] = (s?.Ts ?? Unix, s?.Eff, s?.Detail);
    }
}

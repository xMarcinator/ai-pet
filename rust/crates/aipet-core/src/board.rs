//! The Board: the hooks' chats, Codex's logs, Jira, GitHub and media merged into the pet's bubbles
//! (`src/AiPet.Core/Board.cs`).
//!
//! [`Board::refresh`] rebuilds it a few times a second from [`Sources`], each source as it was at one moment
//! ([`Sources::read`] reads the live ones):
//! - one bubble per chat. A Codex chat that both its hook and Codex's own log report is the newer report, by the
//!   chat's turns first, else by time (the hook's on a tie), with the winner's own time and the best name either has;
//! - a bubble per Jira issue, with its pull request, and per review request that no Jira issue has;
//! - the music player's, and an error bubble for a watcher whose last poll failed;
//! - sorted by priority, then time. Each stack shows so many, a watcher's error first. A bubble the user dismissed
//!   stays hidden until it does something new.
//!
//! Where parity bends:
//! - a turn id whose time the C#'s `Convert.ToInt64` can't read (a form with `+` or `0x` inside, which .NET's Guid
//!   parser takes and only a hand-made id has) makes the C#'s Refresh throw; here time decides;
//! - `AppOf`'s `StartsWith("sdk")` compares by the current culture in the C#, and ordinally here: they differ only for
//!   text with characters a culture ignores or combines (Claude's entry points are ASCII).

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::codex_watcher::CodexWatcher;
use crate::github::{GitHubSettings, GitHubWatcher, PullRequest};
use crate::http::{equals_ignoring_case, escape_data_string};
use crate::jira::{Issue, JiraSettings, JiraWatcher};
use crate::sessions::{AgentSessions, Entry};

/// How many bubbles each stack shows, in the order the stacks are laid out; the rest are counted in
/// [`Board::extra`]. There is only ever one music bubble.
pub const MAX_CARDS: [(&str, usize); 3] = [("chats", 4), ("reviews", 4), ("music", 1)];

/// A GitHub review's dot colour (0xAARRGGBB).
pub const GITHUB_PURPLE: u32 = 0xFFA371F7;

/// How much a bubble's state counts in the order: more first; a state not listed counts 1.
pub fn priority(eff: &str) -> i32 {
    match eff {
        "attention" => 5,
        "review" => 4,
        "working" | "thinking" => 3,
        "done" | "error" => 2,
        "music" | "paused" => 0,
        _ => 1,
    }
}

/// A state's dot colour (0xAARRGGBB), if it has one.
pub fn status_color(state: &str) -> Option<u32> {
    Some(match state {
        "thinking" => 0xFF7AA7FF,
        "working" => 0xFFE27A52,
        "attention" => 0xFFFFCF3F,
        "done" => 0xFF5FD38D,
        "idle" | "sleep" => 0xFF8A8A90,
        "review" => 0xFF4C9AFF,
        "music" => 0xFF1DB954,
        "paused" => 0xFF5E8F6E,
        "error" => 0xFFE5484D,
        _ => return None,
    })
}

/// The stack a kind of bubble lives in: the music player (right above the pet, so no chat ever hides it), agent chats
/// (above it) and reviews (on top).
pub fn section_of(kind: &str) -> &'static str {
    match kind {
        "jira" | "jira-error" | "github" | "github-error" => "reviews",
        "music" => "music",
        _ => "chats",
    }
}

/// One bubble: an agent chat, a Jira or GitHub review, the music player, or a watcher's error (`Session`).
///
/// The C#'s null `Name` (a Codex chat its index doesn't name yet) and `Cwd` (on every bubble but a chat) are empty
/// here: nothing tells them from empty text.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    /// `claude:<sid>` or `codex:<sid>` for a chat, `jira:<key>`, `gh:<url>`, `music`, `jira:_error` or `gh:_error`.
    pub id: String,
    /// Its state as shown: idle, thinking, working, attention, done, review, music, paused or error.
    pub eff: &'static str,
    pub name: String,
    pub detail: String,
    /// `laptop`, `lens` or none.
    pub prop: Option<&'static str>,
    pub cwd: String,
    /// `claude`, `codex`, `jira`, `github` or `music`.
    pub agent: &'static str,
    /// `chat`, `jira`, `jira-error`, `github`, `github-error` or `music`.
    pub kind: &'static str,
    pub url: Option<String>,
    pub ticket_url: Option<String>,
    pub pr_url: Option<String>,
    /// Where a chat runs, as its source saw it: `desktop` (the agent's desktop app), `terminal`, another client, or
    /// none.
    pub r#where: Option<String>,
    /// A chat's deep link into its desktop app (see [`Board`]), or none.
    pub link: Option<String>,
    /// Who reported a chat: `hook`, or `log` (Codex's session files, through [`CodexWatcher`]).
    pub source: Option<&'static str>,
    /// Codex: the chat's turn the report is of (none when it names none), and whether that turn has ended, so the
    /// hook's report and the log's are ordered by turn.
    pub turn: Option<String>,
    pub turn_ended: bool,
    /// Unix seconds.
    pub ts: f64,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            id: String::new(),
            eff: "",
            name: String::new(),
            detail: String::new(),
            prop: None,
            cwd: String::new(),
            agent: "claude",
            kind: "chat",
            url: None,
            ticket_url: None,
            pr_url: None,
            r#where: None,
            link: None,
            source: None,
            turn: None,
            turn_ended: false,
            ts: 0.0,
        }
    }
}

impl Session {
    /// The stack it lives in: `chats`, `reviews` or `music`.
    pub fn section(&self) -> &'static str {
        section_of(self.kind)
    }
}

/// The app a chat runs in, for its bubble's label and border colour (0xAARRGGBB).
pub fn app_of(s: &Session) -> (&'static str, u32) {
    match (s.agent, s.r#where.as_deref()) {
        (_, Some("terminal")) => (
            if s.agent == "codex" {
                "Codex CLI"
            } else {
                "Claude Code CLI"
            },
            0xFFA3A3AD,
        ),
        ("codex", Some("exec")) => ("codex exec", 0xFFA3A3AD),
        ("codex", Some("desktop")) => ("ChatGPT app", 0xFF10A37F),
        // other clients keep their originator's name, e.g. codex_vscode (the VS Code extension) or codex_sdk_ts
        ("codex", Some(other)) if other.contains("vscode") => ("Codex in VS Code", 0xFF3794FF),
        ("codex", _) => ("Codex", 0xFF10A37F),
        (_, Some("desktop")) => ("Claude app", 0xFFD97757),
        (_, Some("claude-vscode")) => ("Claude in VS Code", 0xFF3794FF),
        (_, Some(other)) if other.starts_with("sdk") => ("Claude Agent SDK", 0xFFB48CFF),
        _ => ("Claude", 0xFFD97757),
    }
}

/// The name an agent's bubbles go by.
pub fn agent_label(agent: &str) -> &'static str {
    match agent {
        "codex" => "ChatGPT",
        "jira" => "Jira",
        "github" => "GitHub",
        _ => "Claude",
    }
}

/// A pull request's short name: `https://github.com/Owner/repo/pull/123` is `repo#123`.
pub fn pr_label(url: &str) -> String {
    let parts: Vec<&str> = url.trim_end_matches('/').split('/').collect();
    match parts.as_slice() {
        [.., repo, _, number] if parts.len() >= 4 => format!("{repo}#{number}"),
        _ => "linked".into(),
    }
}

/// The music player as the Board reads it (`IMediaPlayer`'s Name, Song, Artist, Playing and TrackSince).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Media {
    /// The player, e.g. "Spotify".
    pub name: String,
    /// The song playing or paused; none when there's none.
    pub song: Option<String>,
    pub artist: String,
    pub playing: bool,
    /// When the current track started showing (Unix seconds): a new song brings a dismissed bubble back.
    pub track_since: f64,
}

/// What a refresh is made from: each source as it was at one moment.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    /// The hooks' chats ([`AgentSessions::snapshot`]).
    pub chats: Vec<Entry>,
    /// The chats in Codex's own logs ([`CodexWatcher::sessions`]).
    pub codex: Vec<Session>,
    /// The Jira watcher's settings, the issues its last search found, and its last poll's error.
    pub jira: JiraSettings,
    pub issues: Vec<Issue>,
    pub jira_error: Option<String>,
    /// The GitHub watcher's settings, review requests, pull requests by Jira key, and last poll's error.
    pub github: GitHubSettings,
    pub review_requests: Vec<PullRequest>,
    pub pr_for_key: HashMap<String, String>,
    pub github_error: Option<String>,
    /// The music player, on a platform that has one.
    pub media: Option<Media>,
    /// Whether the user wants the music bubble (`config.json`'s `Music`).
    pub music_on: bool,
}

impl Sources {
    /// Reads the live sources, as the C#'s Refresh does.
    pub fn read(
        hooks: &AgentSessions,
        jira: &JiraWatcher,
        github: &GitHubWatcher,
        codex: Option<&CodexWatcher>,
        media: Option<Media>,
        music_on: bool,
    ) -> Sources {
        Sources {
            chats: hooks.snapshot(),
            codex: codex.map(CodexWatcher::sessions).unwrap_or_default(),
            jira: jira.config(),
            issues: jira.issues(),
            jira_error: jira.last_error(),
            github: github.config(),
            review_requests: github.review_requests(),
            pr_for_key: github.pr_for_key(),
            github_error: github.last_error(),
            media,
            music_on,
        }
    }
}

/// A bubble the user closed, with what it showed then.
#[derive(Clone, Debug, PartialEq)]
pub struct Dismissal {
    pub ts: f64,
    /// None when the bubble wasn't there to close.
    pub eff: Option<&'static str>,
    pub detail: Option<String>,
}

/// Everything the pet shows, rebuilt from the chats the hooks reported plus the watchers (`Board`).
///
/// A chat's deep link opens the chat itself in its desktop app; without one, the app is just brought forward:
/// - Claude app: `claude://claude.ai/epitaxy/<the app's id for the chat, local_<uuid>>`;
/// - ChatGPT app: `codex://threads/<thread id>` (the hook's session id).
///
/// Both ids come from the agents' events, so anything but exactly those shapes gets no link.
#[derive(Clone, Debug)]
pub struct Board {
    all: Vec<Session>,
    cards: Vec<Session>,
    /// How many bubbles each stack of [`MAX_CARDS`] leaves out.
    extra: [usize; 3],
    state: &'static str,
    prop: Option<&'static str>,
    dismissed: HashMap<String, Dismissal>,
    music_last_playing: f64,
}

impl Default for Board {
    fn default() -> Self {
        Board {
            all: Vec::new(),
            cards: Vec::new(),
            extra: [0; 3],
            state: "sleep",
            prop: None,
            dismissed: HashMap::new(),
            music_last_playing: 0.0,
        }
    }
}

impl Board {
    pub fn new() -> Board {
        Board::default()
    }

    /// Every bubble, in order: by priority, then newest first.
    pub fn all(&self) -> &[Session] {
        &self.all
    }

    /// The bubbles shown: each stack's (see [`MAX_CARDS`]), in the order of the stacks.
    pub fn cards(&self) -> &[Session] {
        &self.cards
    }

    /// How many bubbles a stack (`chats`, `reviews` or `music`) leaves out.
    pub fn extra(&self, section: &str) -> usize {
        MAX_CARDS
            .iter()
            .position(|(s, _)| *s == section)
            .map_or(0, |i| self.extra[i])
    }

    /// The pet's mood: sleep, idle, thinking, working, attention or done.
    pub fn state(&self) -> &'static str {
        self.state
    }

    /// What the pet holds for the chat in front: `laptop`, `lens` or nothing.
    pub fn prop(&self) -> Option<&'static str> {
        self.prop
    }

    /// Bubbles the user closed, with what they showed then (see [`Board::dismiss`]).
    pub fn dismissed(&self) -> &HashMap<String, Dismissal> {
        &self.dismissed
    }

    pub fn find(&self, id: &str) -> Option<&Session> {
        self.all.iter().find(|s| s.id == id)
    }

    /// Hides a bubble until it does something new (see [`Board::refresh`]); `now` is the time kept for one that isn't
    /// there.
    pub fn dismiss(&mut self, id: &str, now: f64) {
        let s = self.find(id);
        let dismissal = Dismissal {
            ts: s.map_or(now, |s| s.ts),
            eff: s.map(|s| s.eff),
            detail: s.map(|s| s.detail.clone()),
        };
        self.dismissed.insert(id.into(), dismissal);
    }

    /// Rebuilds the bubbles from the sources, at `now` (Unix seconds, as `aipet_ipc::protocol::unix_time` gives
    /// them).
    pub fn refresh(&mut self, sources: &Sources, now: f64) {
        // one bubble per chat: the newest report wins, by Codex's turns (by_turn), else by time (a hook's on a tie),
        // keeping the winner's own time and the best name any source has
        let mut merged = Merged::default();
        for (s, rank) in recorded(&sources.chats) {
            merged.offer(s, rank, now);
        }
        for s in &sources.codex {
            let mut copy = s.clone();
            copy.link = link_for(copy.agent, copy.r#where.as_deref(), sid_of(&copy.id), None);
            // the watcher's name is the app's own title (Codex's index), once the chat has one; else just the folder
            let titled = !copy.name.is_empty();
            if !titled {
                copy.name = folder(&copy.cwd).unwrap_or("Codex").into();
            }
            merged.offer(copy, if titled { 2 } else { 0 }, now);
        }
        let mut sessions: Vec<Session> = merged
            .chats
            .into_iter()
            .map(|(mut s, _)| {
                s.eff = effective(s.eff, s.ts, now);
                s
            })
            .collect();

        reviews(sources, &mut sessions);

        // music
        let media = sources.media.as_ref();
        if media.is_some_and(|m| m.playing) {
            self.music_last_playing = now;
        }
        if let Some(m) = media
            && sources.music_on
            && let Some(song) = &m.song
            && (m.playing || now - self.music_last_playing < 600.0)
        {
            let by = if m.artist.is_empty() { &m.name } else { &m.artist };
            sessions.push(Session {
                id: "music".into(),
                kind: "music",
                agent: "music",
                eff: if m.playing { "music" } else { "paused" },
                ts: m.track_since,
                name: song.clone(),
                detail: format!("{}{by}", if m.playing { "♫ " } else { "Paused · " }),
                ..Session::default()
            });
        }

        if sources.github.enabled
            && let Some(error) = &sources.github_error
        {
            sessions.push(error_bubble(
                "gh:_error",
                "github-error",
                "github",
                "GitHub reviews",
                error,
                now,
            ));
        }
        if sources.jira.enabled
            && let Some(error) = &sources.jira_error
        {
            sessions.push(error_bubble("jira:_error", "jira-error", "jira", "Jira", error, now));
        }

        // stable, as LINQ's OrderByDescending(...).ThenByDescending(...) is
        sessions.sort_by(|a, b| priority(b.eff).cmp(&priority(a.eff)).then_with(|| compare(b.ts, a.ts)));

        // a dismissed bubble comes back when it does something new: a chat's state or detail changes, or a report
        // comes in well after it (the hook and Codex's log often report the same event a moment apart). An error
        // bubble is made anew on every refresh, so only another error brings it back.
        self.dismissed.retain(|id, d| {
            sessions.iter().find(|s| s.id == *id).is_some_and(|s0| {
                let detail_changed = d.detail.as_deref() != Some(s0.detail.as_str());
                let changed = if matches!(s0.kind, "jira-error" | "github-error") {
                    detail_changed
                } else {
                    s0.ts > d.ts + 2.0 || s0.kind == "chat" && (d.eff != Some(s0.eff) || detail_changed)
                };
                !changed
            })
        });

        let live: Vec<&Session> = sessions
            .iter()
            .filter(|s| s.eff != "idle" && !self.dismissed.contains_key(&s.id))
            .collect();
        let mut cards = Vec::new();
        for (i, (section, max)) in MAX_CARDS.iter().enumerate() {
            // a watcher's error always shows, first (it's the only sign its reviews are out of date): the cap only
            // leaves out reviews, and only they count as more
            let (errors, rest): (Vec<&Session>, Vec<&Session>) = live
                .iter()
                .copied()
                .filter(|x| x.section() == *section)
                .partition(|x| matches!(x.kind, "jira-error" | "github-error"));
            let room = max.saturating_sub(errors.len());
            cards.extend(errors.into_iter().cloned());
            cards.extend(rest.iter().take(room).copied().cloned());
            self.extra[i] = rest.len().saturating_sub(room);
        }
        self.cards = cards;

        let chats: Vec<&Session> = sessions.iter().filter(|s| s.kind == "chat").collect();
        let latest = chats.iter().map(|s| s.ts).reduce(linq_max).unwrap_or(0.0);
        let primary = chats.first();
        let mut state = primary.map_or("sleep", |p| p.eff);
        if state == "idle" && now - latest > 300.0 {
            state = "sleep";
        }
        self.state = state;
        self.prop = primary.and_then(|p| p.prop);
        self.all = sessions;
    }
}

/// The chats as one bubble each, in the order they were first offered.
#[derive(Default)]
struct Merged {
    /// Each chat's bubble, and the rank of its best name: 2 = the app's own title, 1 = first prompt, 0 = folder.
    chats: Vec<(Session, i32)>,
    index: HashMap<String, usize>,
}

impl Merged {
    fn offer(&mut self, s: Session, rank: i32, now: f64) {
        let Some(&i) = self.index.get(&s.id) else {
            self.index.insert(s.id.clone(), self.chats.len());
            self.chats.push((s, rank));
            return;
        };
        let (cur, cur_rank) = std::mem::take(&mut self.chats[i]);
        let newer = by_turn(&s, &cur, now)
            .unwrap_or(s.ts > cur.ts || (s.ts == cur.ts && s.source == Some("hook") && cur.source != Some("hook")));
        let name = if rank >= cur_rank {
            s.name.clone()
        } else {
            cur.name.clone()
        };
        let (mut win, lose) = if newer { (s, cur) } else { (cur, s) };
        if win.r#where.is_none() {
            win.r#where = lose.r#where;
        }
        if win.link.is_none() {
            win.link = lose
                .link
                .or_else(|| link_for(win.agent, win.r#where.as_deref(), sid_of(&win.id), None));
        }
        if win.cwd.is_empty() {
            win.cwd = lose.cwd;
        }
        win.name = name;
        self.chats[i] = (win, rank.max(cur_rank));
    }
}

/// Codex: whether report `a` of a chat is newer than `b` by the chat's turn ids, when one is the hook's and the other
/// the log's; none when the ids can't tell (then time decides). Their times don't compare: the hook's is when its
/// hook started (on Windows 1-4 s late, through PowerShell), the log's when Codex wrote the line. So an interrupt or
/// an error, which only the log reports, would lose to a hook event Codex sent just before it.
/// - The log's end of a turn beats the hook's news of that turn (unless the hook has its end too) or of an older one.
/// - The hook's news of a later turn beats the log.
///
/// Turns order as in AgentSessions: UUIDv7s by their time, ids of another shape only equal or not.
fn by_turn(a: &Session, b: &Session, now: f64) -> Option<bool> {
    if a.source == b.source {
        return None;
    }
    let a_is_hook = a.source == Some("hook");
    let (hook, log) = if a_is_hook { (a, b) } else { (b, a) };
    let (hook_turn, log_turn) = (hook.turn.as_deref()?, log.turn.as_deref()?);
    let hook_newer = if hook_turn == log_turn {
        (log.turn_ended && !hook.turn_ended).then_some(false)
    } else if AgentSessions::v7_before(log_turn, hook_turn, now)? {
        Some(true)
    } else {
        (AgentSessions::v7_before(hook_turn, log_turn, now)? && log.turn_ended).then_some(false)
    };
    hook_newer.map(|hook_newer| hook_newer == a_is_hook)
}

/// Chats the hooks reported, each with the rank of its name.
fn recorded(chats: &[Entry]) -> Vec<(Session, i32)> {
    let text = |s: &Option<String>| s.as_deref().filter(|s| !s.is_empty()).map(str::to_owned);
    chats
        .iter()
        // what's left when a chat ends (so a late event can't bring it back) isn't a chat
        .filter(|v| v.ended.is_none())
        .filter_map(|v| {
            let agent = v.agent.unwrap_or("claude");
            // Codex chats without a transcript are one-off exec runs or Codex's own helper threads
            if agent == "codex" && !v.has_transcript {
                return None;
            }
            let cwd = v.cwd.clone().unwrap_or_default();
            let (name, rank) = match (text(&v.chat_title), text(&v.title)) {
                (Some(title), _) => (title, 2),
                (None, Some(prompt)) => (prompt, 1),
                (None, None) => (folder(&cwd).unwrap_or(agent_label(agent)).into(), 0),
            };
            let session = Session {
                id: v.id.clone(),
                ts: v.ts,
                detail: v.detail.clone().unwrap_or_default(),
                prop: v.prop,
                agent,
                r#where: v.r#where.clone(),
                eff: v.state.unwrap_or("idle"),
                source: Some("hook"),
                turn: v.turn.clone(),
                turn_ended: v.turn_ended,
                link: link_for(agent, v.r#where.as_deref(), sid_of(&v.id), v.host_id.as_deref()),
                name,
                cwd,
                ..Session::default()
            };
            Some((session, rank))
        })
        .collect()
}

/// Jira issues, each with its pull request; and review requests, but those that match a Jira issue (merged into it).
fn reviews(sources: &Sources, sessions: &mut Vec<Session>) {
    for i in &sources.issues {
        let pr = sources.pr_for_key.get(&i.key);
        let pr_part = pr.map(|pr| format!("PR {}", pr_label(pr)));
        // what the issue is at, whatever the search (the default finds your own open issues, not reviews)
        let detail: Vec<&str> = [Some(i.status.as_str()), pr_part.as_deref()]
            .into_iter()
            .flatten()
            .filter(|x| !x.is_empty())
            .collect();
        sessions.push(Session {
            id: format!("jira:{}", i.key),
            kind: "jira",
            agent: "jira",
            eff: "review",
            ts: i.updated,
            url: Some(i.url.clone()),
            ticket_url: Some(i.url.clone()),
            pr_url: pr.cloned(),
            name: format!("{} · {}", i.key, i.summary),
            detail: detail.join(" · "),
            ..Session::default()
        });
    }
    let site = sources.jira.site.as_deref().unwrap_or("").trim();
    for pr in &sources.review_requests {
        let in_jira = |key: &String| sources.issues.iter().any(|i| equals_ignoring_case(&i.key, key));
        if pr.keys.iter().any(in_jira) {
            continue;
        }
        let key = pr.keys.first();
        sessions.push(Session {
            id: format!("gh:{}", pr.url),
            kind: "github",
            agent: "github",
            eff: "review",
            ts: pr.updated,
            url: Some(pr.url.clone()),
            pr_url: Some(pr.url.clone()),
            ticket_url: key
                .filter(|_| !site.is_empty())
                .map(|key| format!("https://{site}/browse/{key}")),
            name: format!(
                "{}#{} · {}",
                pr.repo.rsplit('/').next().unwrap_or(""),
                pr.number,
                pr.title
            ),
            detail: match key {
                Some(key) => format!("PR review requested · {key}"),
                None => "PR review requested".into(),
            },
            ..Session::default()
        });
    }
}

fn error_bubble(id: &str, kind: &'static str, agent: &'static str, name: &str, error: &str, now: f64) -> Session {
    Session {
        id: id.into(),
        kind,
        agent,
        eff: "error",
        ts: now,
        name: name.into(),
        detail: format!("{error}. Click to fix"),
        ..Session::default()
    }
}

/// The state a chat shows: a live state goes idle once it's this old.
fn effective(state: &'static str, ts: f64, now: f64) -> &'static str {
    let age = now - ts;
    match state {
        "thinking" | "working" if age > 900.0 => "idle",
        "done" if age > 20.0 => "idle",
        "attention" if age > 3600.0 => "idle",
        _ => state,
    }
}

/// The chat's deep link into its desktop app, or none.
fn link_for(agent: &str, place: Option<&str>, sid: &str, host_id: Option<&str>) -> Option<String> {
    match (agent, place, host_id) {
        ("claude", Some("desktop"), Some(host)) if AgentSessions::is_host_id(host) => {
            Some(format!("claude://claude.ai/epitaxy/{}", escape_data_string(host)))
        }
        ("codex", Some("desktop"), _) if AgentSessions::is_guid(sid) => Some(format!("codex://threads/{sid}")),
        _ => None,
    }
}

/// A chat's session id: its key past the agent's `name:`.
fn sid_of(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, sid)| sid)
}

/// The name of the folder a chat runs in; none for none.
fn folder(cwd: &str) -> Option<&str> {
    Some(AgentSessions::file_name(cwd.trim_end_matches(['/', '\\']))).filter(|f| !f.is_empty())
}

/// `double.CompareTo`: NaN is less than every number, and equal to itself; -0 equals 0.
fn compare(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b)
        .unwrap_or_else(|| a.is_nan().cmp(&b.is_nan()).reverse())
}

/// LINQ's `Max` of doubles, as a fold: NaN only while every number so far is NaN.
fn linq_max(max: f64, x: f64) -> f64 {
    if x > max || max.is_nan() { x } else { max }
}

#[cfg(test)]
mod tests {
    use super::*;

    // What the golden data can't reach: the C#'s real clock can't move 600 s between two refreshes, and a turn id
    // with a `+` inside makes the C#'s Refresh throw.

    #[test]
    fn music_stays_ten_minutes_after_it_last_played() {
        let song = |playing| Sources {
            media: Some(Media {
                name: "Player".into(),
                song: Some("Song".into()),
                artist: String::new(),
                playing,
                track_since: 1.0,
            }),
            music_on: true,
            ..Sources::default()
        };
        let shows = |board: &Board| board.find("music").map(|s| (s.eff, s.detail.clone()));
        let mut board = Board::new();
        let t = 1_790_000_000.0;
        board.refresh(&song(true), t);
        assert_eq!(shows(&board), Some(("music", "♫ Player".into())));
        board.refresh(&song(false), t + 599.5);
        assert_eq!(shows(&board), Some(("paused", "Paused · Player".into())));
        board.refresh(&song(false), t + 600.0);
        assert_eq!(shows(&board), None);
        // a board that never heard it play shows no paused song
        let mut fresh = Board::new();
        fresh.refresh(&song(false), t);
        assert_eq!(shows(&fresh), None);
    }

    #[test]
    fn a_turn_id_the_csharp_throws_on_leaves_it_to_time() {
        let now = 1_790_000_000.0;
        let turn = |ms: u64| format!("{:08x}-{:04x}-7000-8000-000000000001", ms >> 16, ms & 0xFFFF);
        let hook = Session {
            id: "codex:x".into(),
            source: Some("hook"),
            turn: Some(turn(1_789_999_990_000)),
            ts: now - 5.0,
            ..Session::default()
        };
        // a Guid form .NET takes, which sorts after the hook's turn and whose time Convert.ToInt64 can't read
        let log = Session {
            source: Some("log"),
            turn: Some("fffffff0-+0x1-7000-8000-000000000001".into()),
            turn_ended: true,
            ts: now - 1.0,
            ..hook.clone()
        };
        assert!(AgentSessions::is_guid(log.turn.as_deref().unwrap()));
        assert_eq!(by_turn(&hook, &log, now), None);
        assert_eq!(by_turn(&log, &hook, now), None);
        // a well-formed turn before the hook's: the hook's news of its later turn beats the log's end of that one
        let older = Session {
            turn: Some(turn(1_789_999_980_000)),
            ..log.clone()
        };
        assert_eq!(by_turn(&hook, &older, now), Some(true));
        assert_eq!(by_turn(&older, &hook, now), Some(false));
    }
}

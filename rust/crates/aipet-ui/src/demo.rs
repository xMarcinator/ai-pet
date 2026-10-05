//! A scripted day in the pet's life, so the pet shows every state and animation without real chats: a Claude chat
//! that thinks, works, needs you and finishes, a Codex chat, a Jira issue to review, and late in each pass a crowd of
//! chats that fills their stack ([`CROWD_FROM`]). It loops every [`PERIOD`] s.
//!
//! The script goes through the core's own [`Board`], as the hooks and the Jira watcher would report it, so the demo
//! shows the bubbles, labels and moods the pet shows for real data. It is for development, screenshots and tests:
//! compiled with the `demo` feature (`cargo run -p aipet --features demo`) and in this crate's tests, never in a
//! release build.

use std::sync::Arc;

use aipet_core::board::{Board, Sources};
use aipet_core::jira::{Issue, JiraSettings};
use aipet_core::sessions::Entry;

/// The script's length; it starts over after that.
pub const PERIOD: f64 = 40.0;
/// The bubbles' ids: the Claude and Codex chats, and the Jira issue.
pub const CLAUDE: &str = "claude:demo";
pub const CODEX: &str = "codex:demo";
pub const REVIEW: &str = "jira:AIPET-42";
/// The Unix time the pet clock's start stands for, so the Board's times are a day's.
const EPOCH: f64 = 1_790_000_000.0;

/// A chat in one state for a while.
struct Step {
    from: f64,
    until: f64,
    id: &'static str,
    state: &'static str,
    detail: &'static str,
}

/// The chats: each one's states, a line each.
#[rustfmt::skip]
const CHATS: &[Step] = &[
    Step { from: 2.0, until: 6.0, id: CLAUDE, state: "thinking", detail: "Reading MainWindow.axaml" },
    Step { from: 6.0, until: 16.0, id: CLAUDE, state: "working", detail: "Editing crates/aipet-ui/src/cards.rs" },
    Step { from: 16.0, until: 22.0, id: CLAUDE, state: "attention", detail: "Allow cargo build?" },
    Step { from: 22.0, until: 34.0, id: CLAUDE, state: "done", detail: "Finished · 5 files changed" },
    Step { from: 9.0, until: 25.0, id: CODEX, state: "working", detail: "Running cargo test" },
    Step { from: 25.0, until: 29.0, id: CODEX, state: "done", detail: "All 42 tests passed" },
    // a crowd: from CROWD_FROM the chats' stack is full (four bubbles, and "+N more" while Codex is there too), to
    // spread out and fold again
    Step { from: CROWD_FROM, until: 37.0, id: "claude:demo-2", state: "done", detail: "Finished · the release workflow" },
    Step { from: CROWD_FROM, until: 37.0, id: "claude:demo-3", state: "done", detail: "Finished · the Windows proof" },
    Step { from: CROWD_FROM, until: 37.0, id: "claude:demo-4", state: "done", detail: "Finished · the hook server" },
    Step { from: CROWD_FROM, until: 37.0, id: "claude:demo-5", state: "done", detail: "Finished · the placement" },
];

/// When the chats' stack fills up (seconds into each pass).
pub const CROWD_FROM: f64 = 27.0;

/// When the issue waits on you.
const REVIEW_FROM: f64 = 13.0;
const REVIEW_UNTIL: f64 = 31.0;

/// The script's Board, refreshed as the pet's clock goes on.
pub struct Demo {
    board: Board,
}

impl Default for Demo {
    fn default() -> Self {
        Demo::new()
    }
}

impl Demo {
    pub fn new() -> Demo {
        Demo { board: Board::new() }
    }

    /// The Board at `t` seconds since the start.
    pub fn board(&mut self, t: f64) -> Arc<Board> {
        self.board.refresh(&sources(t), EPOCH + t);
        Arc::new(self.board.clone())
    }

    /// Hides a bubble until it does something new, as the core's Board does.
    pub fn dismiss(&mut self, id: &str, t: f64) {
        self.board.dismiss(id, EPOCH + t);
    }
}

/// What the hooks and the Jira watcher report at `t` seconds since the start.
pub fn sources(t: f64) -> Sources {
    let at = t.rem_euclid(PERIOD);
    // the Unix time of this pass's start
    let pass = EPOCH + t - at;
    let chats = CHATS
        .iter()
        .filter(|s| (s.from..s.until).contains(&at))
        .map(|s| {
            let mut chat = Entry::default();
            chat.id = s.id.into();
            chat.state = Some(s.state);
            chat.detail = Some(s.detail.into());
            chat.r#where = Some("desktop".into());
            chat.ts = pass + s.from;
            if s.id == CLAUDE {
                chat.agent = Some("claude");
                chat.title = Some("Make the spike a real pet".into());
                chat.cwd = Some("/home/me/ai-pet".into());
                chat.prop = Some("laptop");
            } else if s.id.starts_with("claude:") {
                // the crowd: Claude Code chats in a terminal
                chat.agent = Some("claude");
                chat.title = Some(format!("Chat {}", &s.id["claude:demo-".len()..]));
                chat.cwd = Some("/home/me/ai-pet".into());
                chat.r#where = Some("terminal".into());
            } else {
                chat.agent = Some("codex");
                chat.chat_title = Some("Spike notes".into());
                chat.cwd = Some("/home/me/spike-notes".into());
                chat.has_transcript = true;
            }
            chat
        })
        .collect();
    let issues = (REVIEW_FROM..REVIEW_UNTIL)
        .contains(&at)
        .then(|| Issue {
            key: "AIPET-42".into(),
            summary: "Port the pet to Rust".into(),
            status: "In review".into(),
            updated: pass + REVIEW_FROM,
            url: "https://example.atlassian.net/browse/AIPET-42".into(),
        })
        .into_iter()
        .collect();
    Sources {
        chats,
        jira: JiraSettings {
            enabled: true,
            site: Some("example.atlassian.net".into()),
            ..JiraSettings::default()
        },
        issues,
        ..Sources::default()
    }
}

/// Whether the issue turns up after `from` and by `to` (seconds since the start), which the C#'s watchers report as
/// NewReviews.
pub fn review_arrived(from: f64, to: f64) -> bool {
    let next = REVIEW_FROM + (((from - REVIEW_FROM) / PERIOD).floor() + 1.0) * PERIOD;
    next <= to
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(t: f64) -> Arc<Board> {
        Demo::new().board(t)
    }

    fn ids(t: f64) -> Vec<String> {
        board(t).cards().iter().map(|s| s.id.clone()).collect()
    }

    #[test]
    fn the_script_walks_claude_through_its_states() {
        let claude = |t: f64| board(t).find(CLAUDE).map(|s| s.eff);
        assert_eq!(claude(1.0), None);
        assert_eq!(claude(3.0), Some("thinking"));
        assert_eq!(claude(10.0), Some("working"));
        assert_eq!(claude(18.0), Some("attention"));
        assert_eq!(claude(30.0), Some("done"));
        assert_eq!(claude(35.0), None);
    }

    #[test]
    fn the_mood_follows_the_loudest_chat() {
        let mood = |t: f64| board(t).state();
        assert_eq!(mood(0.5), "sleep");
        assert_eq!(mood(3.0), "thinking");
        assert_eq!(mood(12.0), "working");
        assert_eq!(mood(17.0), "attention");
        // Claude is done but Codex still works
        assert_eq!(mood(23.0), "working");
        assert_eq!(mood(26.0), "done");
        assert_eq!(mood(38.0), "sleep");
        // the chat in front holds its prop: Claude's laptop, while Codex's newer chat (no prop) isn't in front
        assert_eq!(board(10.0).prop(), None);
        assert_eq!(board(17.0).prop(), Some("laptop"));
    }

    #[test]
    fn bubbles_come_and_go_and_the_script_loops() {
        assert_eq!(ids(14.0), [CODEX, CLAUDE, REVIEW]);
        assert_eq!(ids(26.0), [CODEX, CLAUDE, REVIEW]);
        // the crowd fills the chats' stack: four bubbles, and Claude's chat left out as "+1 more"
        let crowd = [
            "claude:demo-2",
            "claude:demo-3",
            "claude:demo-4",
            "claude:demo-5",
            REVIEW,
        ];
        assert_eq!(ids(CROWD_FROM + 3.0), crowd);
        assert_eq!(board(CROWD_FROM + 3.0).extra("chats"), 1);
        assert_eq!(ids(36.0), &crowd[..4]);
        assert!(ids(38.0).is_empty());
        assert_eq!(ids(14.0 + 3.0 * PERIOD), ids(14.0));
        let board = board(14.0);
        let review = board.find(REVIEW).unwrap();
        assert_eq!((review.kind, review.eff), ("jira", "review"));
    }

    #[test]
    fn the_review_arrives_at_13_s_in_every_pass() {
        assert!(review_arrived(12.99, 13.0));
        assert!(!review_arrived(13.0, 13.02), "only once");
        assert!(!review_arrived(0.0, 12.9));
        assert!(review_arrived(PERIOD + 12.9, PERIOD + 13.1));
        assert!(review_arrived(30.0, PERIOD + 20.0), "across the loop");
        assert!(!review_arrived(0.0, 0.0));
    }
}

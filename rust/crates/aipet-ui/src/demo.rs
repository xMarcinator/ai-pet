//! A scripted day in the pet's life, so the spike shows every state and animation without real chats: a Claude
//! chat that thinks, works, needs you and finishes, a Codex chat, and a Jira review. It loops every [`PERIOD`] s.

use crate::cards::{Bubble, Section};

/// The script's length; it starts over after that.
pub const PERIOD: f64 = 40.0;

/// One bubble in one state for a while.
struct Step {
    from: f64,
    until: f64,
    id: &'static str,
    section: Section,
    state: &'static str,
    title: &'static str,
    detail: &'static str,
    app: Option<u32>,
}

/// The apps' border colours (Board.AppOf).
const CLAUDE_APP: u32 = 0xFFD97757;
const CHATGPT_APP: u32 = 0xFF10A37F;

const fn chat(
    from: f64,
    until: f64,
    id: &'static str,
    state: &'static str,
    title: &'static str,
    detail: &'static str,
    app: u32,
) -> Step {
    Step {
        from,
        until,
        id,
        section: Section::Chats,
        state,
        title,
        detail,
        app: Some(app),
    }
}

/// The script: each bubble's states, one line each.
#[rustfmt::skip]
const SCRIPT: &[Step] = &[
    chat(2.0, 6.0, "claude", "thinking", "Claude · ai-pet", "Reading MainWindow.axaml", CLAUDE_APP),
    chat(6.0, 16.0, "claude", "working", "Claude · ai-pet", "Editing crates/aipet-ui/src/cards.rs", CLAUDE_APP),
    chat(16.0, 22.0, "claude", "attention", "Claude · ai-pet", "Needs you · Allow cargo build?", CLAUDE_APP),
    chat(22.0, 34.0, "claude", "done", "Claude · ai-pet", "Finished · 5 files changed", CLAUDE_APP),
    chat(9.0, 25.0, "codex", "working", "Codex · spike-notes", "Running cargo test", CHATGPT_APP),
    chat(25.0, 29.0, "codex", "done", "Codex · spike-notes", "All 42 tests passed", CHATGPT_APP),
    Step {
        from: 13.0, until: 31.0, id: "jira", section: Section::Reviews, state: "review",
        title: "AIPET-42 · Port the pet to Rust", detail: "In review · waiting on you", app: None,
    },
];

/// The scene at `t` seconds since the start: sets `bubbles` to its bubbles, in stack order (the first of a section
/// is at the front), and gives the pet's mood (a `PetInput::state`).
pub fn scene(t: f64, bubbles: &mut Vec<Bubble>) -> &'static str {
    let at = t.rem_euclid(PERIOD);
    bubbles.clear();
    bubbles.extend(
        SCRIPT
            .iter()
            .filter(|s| (s.from..s.until).contains(&at))
            .map(|s| Bubble {
                id: s.id,
                section: s.section,
                state: s.state,
                title: s.title.into(),
                detail: s.detail.into(),
                app_colour: s.app,
            }),
    );
    // the loudest chat sets the mood; with nothing going on the pet naps, until the script starts over
    let chats = |state: &str| bubbles.iter().any(|b| b.section == Section::Chats && b.state == state);
    ["attention", "working", "thinking", "done"]
        .into_iter()
        .find(|&m| chats(m))
        .unwrap_or(if (2.0..36.0).contains(&at) { "idle" } else { "sleep" })
}

/// Whether a review turns up after `from` and by `to` (seconds since the start), which the C#'s watchers report as
/// NewReviews.
pub fn review_arrived(from: f64, to: f64) -> bool {
    SCRIPT.iter().filter(|s| s.section == Section::Reviews).any(|s| {
        let next = s.from + (((from - s.from) / PERIOD).floor() + 1.0) * PERIOD;
        next <= to
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bubbles(t: f64) -> Vec<Bubble> {
        let mut bubbles = Vec::new();
        scene(t, &mut bubbles);
        bubbles
    }

    fn mood(t: f64) -> &'static str {
        scene(t, &mut Vec::new())
    }

    fn ids(t: f64) -> Vec<&'static str> {
        bubbles(t).iter().map(|b| b.id).collect()
    }

    #[test]
    fn the_script_walks_claude_through_its_states() {
        let claude = |t: f64| bubbles(t).into_iter().find(|b| b.id == "claude").map(|b| b.state);
        assert_eq!(claude(1.0), None);
        assert_eq!(claude(3.0), Some("thinking"));
        assert_eq!(claude(10.0), Some("working"));
        assert_eq!(claude(18.0), Some("attention"));
        assert_eq!(claude(30.0), Some("done"));
        assert_eq!(claude(35.0), None);
    }

    #[test]
    fn the_mood_follows_the_loudest_chat() {
        assert_eq!(mood(0.5), "sleep");
        assert_eq!(mood(3.0), "thinking");
        assert_eq!(mood(12.0), "working");
        assert_eq!(mood(17.0), "attention");
        // Claude is done but Codex still works
        assert_eq!(mood(23.0), "working");
        assert_eq!(mood(26.0), "done");
        assert_eq!(mood(35.0), "idle");
        assert_eq!(mood(38.0), "sleep");
    }

    #[test]
    fn bubbles_come_and_go_and_the_script_loops() {
        assert_eq!(ids(14.0), vec!["claude", "codex", "jira"]);
        assert_eq!(ids(30.0), vec!["claude", "jira"]);
        assert!(ids(38.0).is_empty());
        assert_eq!(bubbles(14.0 + 3.0 * PERIOD), bubbles(14.0));
        let jira = bubbles(14.0).into_iter().find(|b| b.id == "jira").unwrap();
        assert_eq!((jira.section, jira.app_colour), (Section::Reviews, None));
        // the buffer is refilled, not added to
        let mut reused = bubbles(14.0);
        scene(30.0, &mut reused);
        assert_eq!(reused, bubbles(30.0));
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

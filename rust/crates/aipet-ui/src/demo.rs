//! A scripted day in the pet's life, so the spike shows every state and animation without real chats: a Claude
//! chat that thinks, works, needs you and finishes, a Codex chat, and a Jira review. It loops every [`PERIOD`] s.

use crate::cards::{Bubble, Section};

/// The script's length; it starts over after that.
pub const PERIOD: f64 = 40.0;

/// What the pet shows at a moment of the script.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// The pet's mood (a `PetInput::state`).
    pub mood: &'static str,
    /// The bubbles, in stack order (the first of a section is at the front).
    pub bubbles: Vec<Bubble>,
}

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

/// Which pass through the script `t` (seconds since the start) is in.
pub fn cycle(t: f64) -> u64 {
    (t / PERIOD).floor().max(0.0) as u64
}

/// The scene at `t` seconds since the start.
pub fn scene(t: f64) -> Scene {
    let at = t.rem_euclid(PERIOD);
    let bubbles: Vec<Bubble> = SCRIPT
        .iter()
        .filter(|s| (s.from..s.until).contains(&at))
        .map(|s| Bubble {
            id: s.id,
            section: s.section,
            state: s.state,
            title: s.title.to_owned(),
            detail: s.detail.to_owned(),
            app_colour: s.app,
        })
        .collect();
    // the loudest chat sets the mood; with nothing going on the pet naps, until the script starts over
    let chats = |state: &str| bubbles.iter().any(|b| b.section == Section::Chats && b.state == state);
    let mood = ["attention", "working", "thinking", "done"]
        .into_iter()
        .find(|&m| chats(m))
        .unwrap_or(if (2.0..36.0).contains(&at) { "idle" } else { "sleep" });
    Scene { mood, bubbles }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(t: f64) -> Vec<&'static str> {
        scene(t).bubbles.iter().map(|b| b.id).collect()
    }

    #[test]
    fn the_script_walks_claude_through_its_states() {
        let claude = |t: f64| scene(t).bubbles.into_iter().find(|b| b.id == "claude").map(|b| b.state);
        assert_eq!(claude(1.0), None);
        assert_eq!(claude(3.0), Some("thinking"));
        assert_eq!(claude(10.0), Some("working"));
        assert_eq!(claude(18.0), Some("attention"));
        assert_eq!(claude(30.0), Some("done"));
        assert_eq!(claude(35.0), None);
    }

    #[test]
    fn the_mood_follows_the_loudest_chat() {
        assert_eq!(scene(0.5).mood, "sleep");
        assert_eq!(scene(3.0).mood, "thinking");
        assert_eq!(scene(12.0).mood, "working");
        assert_eq!(scene(17.0).mood, "attention");
        // Claude is done but Codex still works
        assert_eq!(scene(23.0).mood, "working");
        assert_eq!(scene(26.0).mood, "done");
        assert_eq!(scene(35.0).mood, "idle");
        assert_eq!(scene(38.0).mood, "sleep");
    }

    #[test]
    fn bubbles_come_and_go_and_the_script_loops() {
        assert_eq!(ids(14.0), vec!["claude", "codex", "jira"]);
        assert_eq!(ids(30.0), vec!["claude", "jira"]);
        assert!(ids(38.0).is_empty());
        assert_eq!(scene(14.0 + 3.0 * PERIOD), scene(14.0));
        assert_eq!((cycle(39.9), cycle(40.0), cycle(85.0)), (0, 1, 2));
        let jira = scene(14.0).bubbles.into_iter().find(|b| b.id == "jira").unwrap();
        assert_eq!((jira.section, jira.app_colour), (Section::Reviews, None));
    }
}

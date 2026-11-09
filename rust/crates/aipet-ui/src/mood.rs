//! The pet's mood from what the app knows (fnt-72): the Board's front chat, a review arriving, the time of day, how
//! long the front chat has been going, and the pointer. Called once a tick; the tick only reads what it picks.
//!
//! The order, highest first: attention, news, working or thinking, sad (a failed turn), done, tired, idle, sleep.
//! Reactions (running, dangling, a poke, a landing, the hover wave) are the pet's own and win over all of them.

/// How long a review's news shows, in seconds.
pub const NEWS_S: f64 = 6.0;
/// How long a chat goes before an idle pet is tired, in seconds.
pub const TIRED_AFTER_S: f64 = 2.0 * 3600.0;

/// Whether `hour` (0 to 23, local) is late enough for the pet to be tired: 23:00 to 04:59.
pub fn night(hour: u32) -> bool {
    hour >= 23 || hour < 5
}

/// What the mood step remembers between ticks: the news window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moods {
    news_until: f64,
}

impl Moods {
    /// A review arrived at `t` (pet clock): the news shows for [`NEWS_S`]. One arriving while it shows neither starts
    /// it again nor makes it longer.
    pub fn review(&mut self, t: f64) {
        if t >= self.news_until {
            self.news_until = t + NEWS_S;
        }
    }

    /// When the news stops showing (pet clock).
    #[cfg(test)]
    pub fn news_until(&self) -> f64 {
        self.news_until
    }

    /// The pet's state at `t`, from the Board's (`board`), whether it is `night`, how long the front chat has been
    /// going (`chat_age`, seconds) and whether the pointer is over the pet (`hover`).
    pub fn pick(&self, board: &'static str, t: f64, night: bool, chat_age: Option<f64>, hover: bool) -> &'static str {
        match board {
            "attention" => "attention",
            _ if t < self.news_until => "news",
            "working" | "thinking" => board,
            "error" => "sad",
            "idle" if night || chat_age.is_some_and(|age| age >= TIRED_AFTER_S) => "tired",
            // asleep stays asleep; at night, the pointer over it shows a tired pet
            "sleep" if night && hover => "tired",
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_is_from_eleven_to_before_five() {
        assert!(night(23) && night(0) && night(4));
        assert!(!night(22) && !night(5) && !night(12));
    }

    #[test]
    fn a_review_shows_news_for_six_seconds_and_a_second_one_doesnt_stretch_it() {
        let mut m = Moods::default();
        m.review(10.0);
        assert_eq!(m.pick("idle", 10.1, false, None, false), "news");
        m.review(15.0);
        assert_eq!(m.news_until(), 16.0);
        assert_eq!(m.pick("idle", 15.9, false, None, false), "news");
        assert_eq!(m.pick("idle", 16.0, false, None, false), "idle");
        // a later one starts it again
        m.review(20.0);
        assert_eq!(m.news_until(), 26.0);
    }

    #[test]
    fn tired_comes_late_at_night_or_after_two_hours_and_only_instead_of_idle() {
        let m = Moods::default();
        assert_eq!(m.pick("idle", 0.0, true, None, false), "tired");
        assert_eq!(m.pick("idle", 0.0, false, Some(TIRED_AFTER_S), false), "tired");
        assert_eq!(m.pick("idle", 0.0, false, Some(TIRED_AFTER_S - 1.0), false), "idle");
        // asleep stays asleep, unless hovered at night
        assert_eq!(m.pick("sleep", 0.0, true, None, false), "sleep");
        assert_eq!(m.pick("sleep", 0.0, true, None, true), "tired");
        assert_eq!(m.pick("sleep", 0.0, false, None, true), "sleep");
        for busy in ["working", "thinking", "done", "attention"] {
            assert_eq!(m.pick(busy, 0.0, true, Some(1e9), true), busy);
        }
    }

    #[test]
    fn every_pair_keeps_the_order() {
        // attention > news > working/thinking > sad > done > tired > idle > sleep: each state against news (the one
        // that comes from outside the Board) and the Board's own in their order
        let mut m = Moods::default();
        m.review(0.0);
        let at = 1.0;
        assert_eq!(m.pick("attention", at, true, Some(1e9), true), "attention");
        for under in ["working", "thinking", "error", "done", "idle", "sleep"] {
            assert_eq!(m.pick(under, at, true, Some(1e9), true), "news", "news over {under}");
        }
        let calm = Moods::default();
        let order = ["attention", "working", "error", "done", "idle", "sleep"];
        let shown = order.map(|b| calm.pick(b, at, false, None, false));
        assert_eq!(shown, ["attention", "working", "sad", "done", "idle", "sleep"]);
    }
}

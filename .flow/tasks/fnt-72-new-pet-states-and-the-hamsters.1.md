---
satisfies: [R1, R2, R3, R4, R5, R6]
---
# fnt-72-new-pet-states-and-the-hamsters.1 Pet-mood step, the error chat, and the behaviour for sad, tired, news, aura and heart

## Description
All behaviour, no art: the error chat on StopFailure, a pet-mood step after the board, the new inputs (news window, clock, chat age, hover at night), the new `fx`/`prop`/`eyes`/`state` values, and the fallbacks for packs that lack them. Proves that the C# golden replays still pass (the spec's early proof point).

**Size:** M
**Files:** `rust/crates/aipet-core/src/sessions/claude.rs`, `rust/crates/aipet-core/src/board.rs`, `rust/crates/aipet-core/src/log.rs`, `rust/crates/aipet-sprite/src/{pose,pet}.rs`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-ui/src/{cards,settings/general}.rs`, `rust/crates/aipet-core/tests/board.rs`
**Touches:** [rust/crates/aipet-core/src/**, rust/crates/aipet-core/tests/**, rust/crates/aipet-sprite/src/pose.rs, rust/crates/aipet-sprite/src/pet.rs, rust/crates/aipet-sprite/tests/**, rust/crates/aipet-ui/src/**]

### Approach
- StopFailure: today it sets `attention` with a `stop_error` detail (`sessions/claude.rs:141,193`); set `error` instead, with the "Failed" label and red dot (`board.rs:36` priority already ranks `error` with done; `status_color` has it). Make `effective()` (`board.rs:639`) expire a chat's `error` after 3600 s like attention. In `tests/board.rs`, list the StopFailure cases as the one known deviation from the C# goldens; never edit the golden files.
- Pet-mood step: after `Board::state` in `aipet-ui/src/lib.rs:575-585`, pick the mood per the spec's priority from the front chat, `news_until`, the clock, the chat's age and hover. `Mood::of` (`pose.rs:79`) gains `error` -> Sad, plus Tired and News moods.
- news: on `News::Alert` from `NewReviews` (`aipet/src/core_thread.rs:275`), set a new `news_until = t + 6 s` beside `alert_until` (`lib.rs:66,796`); leave `alert_until` and its forced attention (`pet.rs:345`) for the golden replay and chat alerts. Check `NewReviews` does not fire for reviews present at startup; if it does, suppress the first poll's.
- aura: in `values()` (`pet.rs:700-707,724`) done's first 1.8 s gives fx `aura`, then sparkles; done's mood and timings stay.
- heart: a poke gives fx `heart` instead of `sparkles`.
- tired: share `local_time()` (`aipet-core/src/log.rs:203`) through an injected clock; add a first-seen time to the session entry (`sessions/mod.rs:58`, `board.rs:78`) for the 2-hour rule.
- Fallbacks: when the worn pack lacks a value (`compose.rs` fallback chains), the behaviour substitutes per the spec's table so nothing becomes plain idle.
- State names also map in `aipet-ui/src/settings/general.rs:161` and `cards.rs:189`.

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/src/pet.rs:342-500,672-730` - compute_pose and values
- `rust/crates/aipet-sprite/src/pose.rs:67-130,304-430` - Mood, Prop, Fx, fx_key
- `rust/crates/aipet-core/src/board.rs:36-60,440-460,620-650`
- `rust/crates/aipet-sprite/tests/golden.rs` - frame goldens that must keep passing

### Key context
- The golden replays never set the new inputs; keep it that way.

## Acceptance
- [ ] `tests/golden.rs` and `tests/board.rs` pass, the latter naming the StopFailure deviation
- [ ] Tests: StopFailure -> red "Failed" bubble and a sad pet; the next event clears it; 3600 s expires it
- [ ] Tests: a review alert gives 6 s of news with the envelope and no "!"; a second alert inside the window doesn't restart it; an attention chat wins
- [ ] Tests: done opens with `aura` for about 1.8 s, then sparkles; a poke gives `heart`
- [ ] Tests: tired at 23:00 and 04:59 but not 05:00 or 22:59; a chat of exactly 2 h; a sleeping pet hovered at night shows tired
- [ ] Tests: a pack without the new values gets the fallback table, never plain idle

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

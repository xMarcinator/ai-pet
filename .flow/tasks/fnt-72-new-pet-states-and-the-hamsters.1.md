---
satisfies: [R1, R2, R3, R4, R5, R6]
---
# fnt-72-new-pet-states-and-the-hamsters.1 Pet-mood step, the error chat, and the behaviour for sad, tired, news, aura and heart

## Description
All behaviour, no art: the error chat on StopFailure, a pet-mood step after the board, the new inputs (news window, clock, chat age, hover at night), the new `fx`/`prop`/`eyes`/`state` values, and the fallbacks for packs that lack them. Proves that the C# golden replays still pass (the spec's early proof point).

**Size:** M
**Files:** `rust/crates/aipet-core/src/sessions/claude.rs`, `rust/crates/aipet-core/src/board.rs`, `rust/crates/aipet-core/src/log.rs`, `rust/crates/aipet-sprite/src/{pose,pet}.rs`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-ui/src/{cards,settings/general}.rs`, `rust/crates/aipet-core/tests/board.rs`
**Touches:** [rust/crates/aipet-core/src/**, rust/crates/aipet/src/core_thread.rs, rust/crates/aipet-core/tests/**, rust/crates/aipet-sprite/src/pose.rs, rust/crates/aipet-sprite/src/pet.rs, rust/crates/aipet-sprite/tests/**, rust/crates/aipet-ui/src/**]

### Approach
- StopFailure: today it sets `attention` with a `stop_error` detail (`sessions/claude.rs:141,193`); set `error` instead, with the "Failed" label and red dot (`board.rs:36` priority already ranks `error` with done; `status_color` has it). Make `effective()` (`board.rs:639`) expire a chat's `error` after 3600 s like attention. In `tests/board.rs`, list the StopFailure cases as the one known deviation from the C# goldens; never edit the golden files.
- Pet-mood step: after `Board::state` in `aipet-ui/src/lib.rs:575-585`, pick the mood per the spec's priority from the front chat, `news_until`, the clock, the chat's age and hover. `Mood::of` (`pose.rs:79`) gains `error` -> Sad, plus Tired and News moods.
- Compatibility switch: one new `PetInput` field (default off) turns on every new behaviour below; the app sets it, the golden replays (`tests/golden.rs`) never do. Off, the pet emits exactly today's values.
- news: in production, `News::Alert` from `NewReviews` (`aipet/src/core_thread.rs:275`) starts `news_until = t + 6 s` only when no news window is open (a later alert never extends it), and no longer sets `alert_until` (`lib.rs:66,796`). `alert_until` and its forced attention (`pet.rs:345`) remain for the golden replay only. Check `NewReviews` does not fire for reviews present at startup; if it does, suppress the first poll's.
- Mood step: its own module (for example `aipet-ui/src/mood.rs`), called once per tick from `PetUi::tick`; the tick only reads its result.
- aura: in `values()` (`pet.rs:700-707,724`) done's first 1.8 s gives fx `aura`, then sparkles; done's mood and timings stay.
- heart: a poke gives fx `heart` instead of `sparkles`.
- tired: share `local_time()` (`aipet-core/src/log.rs:203`) through an injected clock. Add a first-seen time for the 2-hour rule to both board sources: hook sessions (`sessions/mod.rs:58`) and Codex-watcher chats (`codex_watcher.rs`, whose `Chat` keeps only `ts`), set when the pet first sees the chat, so chats open at startup count from startup; carry it on the board entry (`board.rs:78`).
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
- [ ] Tests: a review alert gives 6 s of news with the envelope and no "!"; a second alert at 5 s leaves the deadline at 6 s; an attention chat wins
- [ ] With the compatibility switch off, done and poke give sparkles and an alert gives attention, exactly as today (the golden replays prove it)
- [ ] A matrix test covers every pair in the mood priority, and run, dangle, poke, landing and the hover wave each win over sad, news and tired
- [ ] Tests: a Codex-watcher chat and a hook chat both reach tired after 2 h from first seen
- [ ] Tests: done opens with `aura` for about 1.8 s, then sparkles; a poke gives `heart`
- [ ] Tests: tired at 23:00 and 04:59 but not 05:00 or 22:59; a chat of exactly 2 h; a sleeping pet hovered at night shows tired
- [ ] Tests: a pack without the new values gets the fallback table, never plain idle

## Done summary
Behaviour for sad, tired, news, the aura and the heart, all behind PetInput::moods and Board::moods (on in the app, off in the C# golden replays, which pass unchanged; no board deviation was needed: a StopFailure keeps the C#'s attention state and marks the chat failed, and the Board with moods shows it as a red 'Failed: …' error bubble lapsing after an hour). Mood step in aipet-ui/src/mood.rs with priority attention > news > working/thinking > sad > done > tired > idle > sleep; news 6 s, not stretched; tired at 23:00-04:59 or a front chat 2 h old (Board::front_age, from first seen, hook and Codex log alike); a sleeping pet hovered at night shows tired. Packs without the new values fall back (sad/news -> attention look, tired -> idle, aura/heart -> sparkles). Not done: Codex reports no failures, so Codex chats are never sad; NewReviews at startup not re-checked (the watcher test shows NewReviews only for new keys).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 4f43b6e, d7f8b80, d769325
- Tests: cargo test --workspace
- PRs:
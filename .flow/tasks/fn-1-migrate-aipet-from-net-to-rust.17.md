---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.17 Bubble actions: Open, PR and media buttons, deep links, error bubbles

## Description
Give the Rust bubbles the C#'s buttons and click routing, through a `Platform` trait the shells provide. The trait is
defined here; task 18 implements it.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/{cards,view,lib,platform}.rs`, `rust/crates/aipet-ui/tests/actions.rs`
**Touches:** [rust/crates/aipet-ui/src/cards.rs, rust/crates/aipet-ui/src/view.rs, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-ui/src/platform.rs, rust/crates/aipet-ui/tests/actions.rs]

### Approach
- Buttons by bubble kind, as in `MakeCard` (`src/AiPet.UI/MainWindow.axaml.cs:352-435`): Open in Jira, Open pull
  request and Previous/Play-pause/Next, with tooltips.
- **No Stop button** (the user's decision, 2026-09-30; see the spec's Decision Context). The C#'s Stop (`:375-383`,
  `OnlyDesktopChat` at `:455-456`) focuses the agent's app and sends it Escape; the Rust pet never sends another app
  simulated keys. Don't port Stop, its tooltip or its visibility rule; a working chat keeps its Open button.
- Otherwise, visibility follows `UpdateChrome` (`:470-485`) exactly.
- `OpenItem` routing (`:458-467`): error bubbles open their Settings page, music focuses the player, Jira and GitHub
  bubbles open their URL, and a chat opens its deep link or focuses its agent.
- The buttons join `hit_rects`.
- `Platform` trait: `open_url`, `open_folder`, `focus_agent`, and media (poll, previous, play-pause, next, focus).
  There is no `send_escape`.

### Investigation targets
**Required:**
- `src/AiPet.UI/MainWindow.axaml.cs:319-485`
- `src/AiPet.Core/Platform.cs` — `IPlatform`/`IMediaPlayer`
- `rust/crates/aipet-ui/src/cards.rs`, `rust/crates/aipet-ui/src/view.rs`
**Optional:**
- `src/AiPet.UI/App.axaml` — button styles
## Acceptance
- [ ] Unit tests with a fake platform cover visibility and routing for every kind.
- [ ] No bubble has a Stop button: a working or thinking chat in the desktop app shows Open, which opens the chat or
      brings its app forward, and nothing sends keys to another app (a test checks both).
- [ ] Screenshots show the buttons on hover. The buttons are in the input region.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

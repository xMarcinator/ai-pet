---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.17 Bubble actions: Open, PR, media and Stop buttons, deep links, error bubbles

## Description
Give the Rust bubbles the C#'s buttons and click routing, through a `Platform` trait the shells provide. The trait is
defined here; task 18 implements it.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/{cards,view,lib,platform}.rs`, `rust/crates/aipet-ui/tests/actions.rs`
**Touches:** [rust/crates/aipet-ui/src/cards.rs, rust/crates/aipet-ui/src/view.rs, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-ui/src/platform.rs, rust/crates/aipet-ui/tests/actions.rs]

### Approach
- Buttons by bubble kind, as in `MakeCard` (`src/AiPet.UI/MainWindow.axaml.cs:352-435`): Open in Jira, Open pull
  request, Previous/Play-pause/Next and Stop, with tooltips.
- Visibility follows `UpdateChrome` exactly (`:470-485`). Stop is visible on every chat whose state is working or
  thinking and that runs in the desktop app, whatever the uptime or the number of chats.
- Stop's action has two stages (`:375-383`, `OnlyDesktopChat` at `:455-456`):
  - If the pet has run more than 900 s, and this is the only chat of that agent the pet knows, focus the agent's app.
    Send Escape only if focusing succeeded.
  - Otherwise open the chat, or bring its app forward, for the user to stop it there.
  - The tooltip differs between the two cases ("Stop" or "Open <agent> to stop it").
- `OpenItem` routing (`:458-467`): error bubbles open their Settings page, music focuses the player, Jira and GitHub
  bubbles open their URL, and a chat opens its deep link or focuses its agent.
- The buttons join `hit_rects`.
- `Platform` trait: `open_url`, `open_folder`, `focus_agent`, `send_escape`, and media (poll, previous, play-pause,
  next, focus).

### Investigation targets
**Required:**
- `src/AiPet.UI/MainWindow.axaml.cs:319-485`
- `src/AiPet.Core/Platform.cs` — `IPlatform`/`IMediaPlayer`
- `rust/crates/aipet-ui/src/cards.rs`, `rust/crates/aipet-ui/src/view.rs`
**Optional:**
- `src/AiPet.UI/App.axaml` — button styles
## Acceptance
- [ ] Unit tests with a fake platform cover visibility and routing for every kind.
- [ ] Stop tests check visibility separately from the action. It is visible at 899, 900 and 901 s, with 1 or 2 chats.
      It sends Escape only past 900 s, with one chat, and after a successful focus; otherwise it opens the chat. The
      tooltip matches each case.
- [ ] Screenshots show the buttons on hover. The buttons are in the input region.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

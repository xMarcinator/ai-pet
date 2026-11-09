---
satisfies: [R7, R8, R11]
---
# fnt-71-mcp-server-in-the-pet.5 Avatar, settings and board tools through the UI thread

## Description
Give the listener a command channel to the UI thread with a reply, and build `list_avatars`, `wear_avatar`, `get_settings`, `set_settings`, `get_board` and `dismiss_card` on it. The UI applies each command as the matching Settings message would and saves through the same `save_prefs` path.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-desktop/src/shell.rs`, `rust/crates/aipet-wayland/src/**`, `rust/crates/aipet/src/core_thread.rs`, `rust/crates/aipet-mcp/src/tools.rs` (new)
**Touches:** [rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-desktop/src/**, rust/crates/aipet-wayland/src/**, rust/crates/aipet/src/core_thread.rs, rust/crates/aipet/src/main.rs, rust/crates/aipet-mcp/**]

### Approach
- Add a `News` variant carrying a command and a oneshot reply (`aipet-ui/src/lib.rs:167-183`); the shells already drain `News` each frame (`aipet-desktop/src/shell.rs:361-366`, Wayland similar). Check the drain still runs when the pet is asleep or hidden; if frames stop, wake the shell.
- Map commands onto existing messages: `SelectAvatar`, `SelectTheme`, `SetPetSize`, `SetBubbleSize`, `SetBubbles`, `SetOnTop`, `SetMusic` (`lib.rs:96-136`); validate ranges against `geometry.rs` before sending.
- Board: read the latest `Arc<Board>` the core thread publishes; dismiss through `Handle::dismiss` (`aipet/src/core_thread.rs`).
- 5 s reply timeout -> `isError`.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/lib.rs:96-265,1046,1066` - Message, News, Prefs, Host, wear, save_prefs
- `rust/crates/aipet/src/main.rs:288-366` - App as Host, config writes
- `rust/crates/aipet/src/core_thread.rs:1-200` - Board publishing and Handle
- `rust/crates/aipet-ui/src/geometry.rs:56-57` - size ranges

### Key context
- fn-2.20 and fn-2.21 also edit `aipet-ui/src/lib.rs` and sizes: rebase on them.
- `config.json` must stay byte-compatible with the .NET pet (`aipet/src/config.rs`).

## Acceptance
- [ ] Tests drive each tool against a headless UI and see the Prefs, worn avatar and saved `config.json` match what the Settings message produces
- [ ] An out-of-range size, unknown avatar or theme, and unknown card id each give `isError` and change nothing
- [ ] A UI that never replies gives `isError` after 5 s
- [ ] Hands-on: an open Settings window shows a size changed by the tool

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

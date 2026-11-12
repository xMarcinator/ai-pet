---
satisfies: [R7, R8, R11]
---
# fnt-71-mcp-server-in-the-pet.5 Avatar, settings and board tools through the pet

## Description
Give the listener a command channel to the pet (the iced pet's state that owns Prefs and the config store, on the UI thread; FNT-75 runs after this spec and moves the channel's end to its pet thread) with a reply, and build `list_avatars`, `wear_avatar`, `get_settings`, `set_settings`, `get_board` and `dismiss_card` on it. The pet applies each command as it applies the matching Settings message, and saves through the same `save_prefs` path. Nothing here may depend on Settings: the tools work with Settings closed, absent or (later) in another process.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-desktop/src/shell.rs`, `rust/crates/aipet-wayland/src/**`, `rust/crates/aipet/src/core_thread.rs`, `rust/crates/aipet-mcp/src/tools.rs` (new)
**Touches:** [rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-desktop/src/**, rust/crates/aipet-wayland/src/**, rust/crates/aipet/src/core_thread.rs, rust/crates/aipet/src/main.rs, rust/crates/aipet-mcp/**]

### Approach
- Add a `News` variant carrying a command and a oneshot reply (`aipet-ui/src/lib.rs:167-183`); the shells already drain `News` each frame (`aipet-desktop/src/shell.rs:361-366`, Wayland similar). Check the drain still runs when the pet is asleep or hidden; if frames stop, wake the shell.
- Map commands onto existing messages: `SelectAvatar` (a pet's default style, by `load::groups`), `SelectStyle` (FNT-74), `SelectTheme`, `SetPetSize`, `SetBubbleSize`, `SetBubbles`, `SetOnTop`, `SetMusic` (`lib.rs:96-136`); validate ranges against `geometry.rs` before sending.
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
- [ ] Tests drive each tool against a headless pet, with no Settings, and see the Prefs, worn avatar and saved `config.json` match what the Settings message produces
- [ ] The six tools are in task 2's conformance table
- [ ] An out-of-range size, unknown pet, style or theme, and unknown card id each give `isError` and change nothing
- [ ] `wear_avatar {name: "Hood", style: "Vector"}` wears the vector Hood; `list_avatars` lists four built-in pets with their styles
- [ ] A UI that never replies gives `isError` after 5 s
- [ ] Hands-on: an open Settings window shows a size changed by the tool

## Done summary
list_avatars, wear_avatar, get_settings, set_settings, get_board and dismiss_card ask the pet over a PetLink (aipet_core::control): a Request that comes as news, which the shells hand over every frame and whose effects (on top) they now apply. The pet checks each command whole, then does it through Settings' messages, so it saves config.json as Settings does; 5 s without an answer is isError. Tests: aipet-ui's control tests compare each tool with the Settings message; the listener's tests drive a real headless PetUi, with the six tools in the conformance table. Built-in pets are three now (Sprout, Hood, Pip; the Hamster stopped being built in), so the test checks three, not the plan's four. Hands-on on Hyprland, a sandboxed pet: wear_avatar and set_settings show in an open Settings within 0.5 s and in config.json. It found that Wayland's Settings didn't redraw on an agent's change (fixed: a frame that brings a command redraws every window), and the owner asked for the size sliders in the accent (done).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 0078d50, a61641d
- Tests: cargo test -p aipet-core -p aipet-ui -p aipet-mcp -p aipet-desktop -p aipet-wayland -p aipet
- PRs:
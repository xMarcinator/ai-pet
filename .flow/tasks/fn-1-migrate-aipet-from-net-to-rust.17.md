---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.17 Bubble actions: Open, PR and media buttons, deep links, error bubbles

## Description
Give the Rust bubbles the C#'s buttons and click routing, through the `Platform` trait the shells provide. The trait
and its recording fake come from task 15's groundwork (`aipet-ui/src/platform.rs`); task 18 implements it.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/{cards,view,lib}.rs`, `rust/crates/aipet-ui/tests/actions.rs`
**Touches:** [rust/crates/aipet-ui/src/cards.rs, rust/crates/aipet-ui/src/view.rs, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-ui/tests/actions.rs]

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
- Call the `Platform` trait from task 15 (`open_url`, `open_folder`, `focus_agent`, and media: poll, previous,
  play-pause, next, focus); there is no `send_escape`.

### Investigation targets
**Required:**
- `src/AiPet.UI/MainWindow.axaml.cs:319-485`
- `src/AiPet.Core/Platform.cs` — `IPlatform`/`IMediaPlayer`
- `rust/crates/aipet-ui/src/cards.rs`, `rust/crates/aipet-ui/src/view.rs`
**Optional:**
- `src/AiPet.UI/App.axaml` — button styles
**From task 12 (2026-10-01):** a bubble's actions find their chat with `Board::find(id)`, and `Board::dismiss(id, now)`
hides a bubble until it does something new (`now` from `aipet_ipc::protocol::unix_time`). `board::app_of(&Session)`
gives the app label and colour; `pr_label`, `agent_label` and `status_color` are the C#'s tables.
**From task 15 (2026-10-01):** your extension points are `aipet-ui/src/{cards,view,lib}.rs`: `PetUi::platform()`, the
Board in `PetUi.board` (`find`), and `Effect::OpenSettings` with `settings.page` set first. lib.rs interns each Board id
once for the pet's life, because cards.rs types bubble ids as `&'static str`. Parity gaps task 15 found in your files:
there is no music stack (the C# keeps the music bubble in a stack of its own just above the pet, always the front of
its stack; the Rust puts it in the chats stack, behind the chats); cards.rs adds the thinking dots after "+N more"; and
view.rs gives a GitHub review's dot the review blue, where the C# uses purple.
## Acceptance
- [ ] Unit tests with a fake platform cover visibility and routing for every kind.
- [ ] No bubble has a Stop button: a working or thinking chat in the desktop app shows Open, which opens the chat or
      brings its app forward, and nothing sends keys to another app (a test checks both).
- [ ] Screenshots show the buttons on hover. The buttons are in the input region.
## Done summary
The Rust bubbles now have the C#'s round buttons, click routing and tooltips (MakeCard, UpdateChrome, OpenItem), all through the `Platform` trait. One commit, 87cfb37 on wave/fn-1.17, carries it. Its gates are green, and the task waits for the conductor's review (still in_progress, no `flowctl done`).

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

### What the commit does (aipet-ui/src/{cards,view,lib}.rs, aipet-ui/tests/actions.rs)

- **Buttons by kind.** They show only while the pointer is on a bubble that is interactive: front of its stack (or the stack is spread) and not leaving, as UpdateChrome has it.
  - Open in Jira when there's a ticket URL, Open pull request when there's a PR URL.
  - Previous, Play / pause and Next on the player. The icon is pause while playing, play while paused.
  - The icons use MakeCard's path data, drawn on a canvas. The styles are Button.round's.
  - The buttons lie inside the bubble's body, so they are in `hit_rects`.
- **No Stop.** A working or thinking chat in its agent's desktop app shows **Open** where Stop stood. Open does what Stop did when it couldn't stop: open the chat's deep link, or bring the agent's app forward. The `Platform` trait has no way to send keys.
- **Text beside the buttons** is cut to 190 px (MaxWidth), or less where the bubble would exceed 350.
- **Clicks.**
  - `Message::CardPressed` spreads a folded stack, else runs OpenItem:
    - jira-error and github-error open their Settings page (`Effect::OpenSettings`, page set first);
    - the player calls the player's focus;
    - Jira and GitHub items open their URL;
    - a chat opens its link, else `focus_agent(agent, None)`;
    - a bubble the Board doesn't have (the ghost) focuses Claude.
  - The new `Message::Button(id, Button)`, with `pub use cards::Button`, carries the round buttons' clicks.
- **Tooltips** follow Avalonia 12.1.3's Fluent dark ToolTip; I probed the values headless from the package.
  - Timing: 400 ms delay, or at once within 100 ms of the last one closing; a 0.15 s fade.
  - Placement: 20 px below the pointer, slid back inside the surface.
  - Style: #2B2B2B, 1 px border of black at 0.36, radius 5, padding 8,5,8,7, 12 px text wrapped at 320.
  - Texts: each button's tip, "Dismiss" on the dismiss button, and the bubble's own (its title, plus "In the <app>" for a chat).
  - A tooltip is in `drawn_rects` but not in `hit_rects`.
- **Parity gaps task 15 found, fixed:**
  - the music bubble has its own stack right above the pet;
  - thinking dots come before "+N more";
  - a GitHub review's dot is purple.
- **A gap I found:** a paused player's dot is now the Board table's green (0xFF5E8F6E) instead of grey.

### Verification at 87cfb37

- `cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check`: 347 passed, 1 ignored. That is the baseline's 337 plus 3 new unit tests and 7 in tests/actions.rs. Green receipt 87cfb37f-unittest.
- `dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build`: 160 passed, 52 skipped, as at the baseline (no C# file changed). Green receipt 87cfb37f-dotnet.
- Linux cross-check, clean, with every target including tests/actions.rs: `cargo clippy --offline -p aipet-ui -p aipet-wayland -p aipet-desktop --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings`. It uses task 15's stand-ins (C:\Users\MJE\AppData\Local\Temp\t15x).
- Also clean: `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo clippy -p aipet-ui` with and without `--features demo`, all with `-D warnings`.
- Mutation checks: 15 deliberate breaks, each reverted, each failed a test.
  - The rules for which buttons show, and the folded stack's back bubbles.
  - The routing: links, ticket URLs, error pages, the player's focus.
  - The tooltip delays.
  - The parity fixes: the order of the dots and "+N more", the music stack, the GitHub dot.
  - The narrower bubble's hold on the pointer, in the hover test and in both regions.
- Screenshots: no GUI could be started here. Offscreen renders of the real `pet_view` through iced's headless wgpu renderer (no window) are in C:/Users/MJE/Documents/projects/ai-pet/.flow/tmp/wave-2026-09-29/task-17-shots/, with the scratch tool's source. They show each hovered kind's buttons and the tooltips. Hands-on screenshots are for tasks 22 and 27.

baseline: green at e3c3e82 (Rust 337 passed, 1 ignored, clippy and fmt clean; C# 160 passed, 52 skipped).

### Choices to review

- **The chat's Open button is my reading of AC2** ("a working or thinking chat in the desktop app shows Open"). The C# has no Open button on chats; its Open is "Open in Jira". So Open takes Stop's place with Stop's visibility rule, and its tooltip is "Open in the Claude app" or "Open in the ChatGPT app".
- **A hovered bubble that gets narrower** (its text cut to 190) keeps the pointer on the strip it gave up. The strip is in `hit_rects` and `drawn_rects` (the Windows region). The C# would flicker there.
- **Tooltips on the dismiss button and the bubble body** come from the same mechanism. They are C# features, but no task named them.
- **Test hooks:** `PetUi::shown(id)`, `PetUi::tooltip()` and `aipet_ui::Shown` are doc-hidden, as the core's `poll()` is.
- **A click on a leaving bubble** routes as in the C#: it may spread its stack, or focus Claude when the Board no longer has the bubble.

### Follow-ups outside Touches

- style.rs: `status_colour` is now only the fallback for a state the Board's table lacks, which no real state is. Drop it when style.rs is next touched (task 27 has it), and use `board::status_color(state).unwrap_or(0xFF888888)`, the C#'s default.
- aipet-wayland `redraw_scope`: `Ui::Button(..)` falls to `Scope::All`, so Settings is redrawn too. It's harmless; `Scope::Window(pet)` would match `CardPressed`.

### Manual checks for tasks 22 and 27 (Git Bash, repository root, the .NET pet quit first)

1. Run `(cd rust && cargo build -p aipet) && AIPET_DEBUG=1 rust/target/debug/AiPet.exe`.
2. Point at each kind of bubble: a working Claude-app chat, a Jira issue with a PR, a GitHub review, the player, an error bubble. Its buttons and the dismiss X appear.
3. Rest the pointer on each button for half a second. Its tooltip shows.
4. Click each button: Jira and the PR open in the browser; Previous, Play / pause and Next work Spotify; the chat's Open opens the chat or brings its app forward.
5. Click a bubble body: a folded stack spreads first, then the click opens the item. An error bubble opens its Settings page.
6. The buttons take clicks, and the gaps between bubbles click through.
7. Take screenshots of the hovered bubbles (AC3).
8. `cd rust && cargo run -p aipet --features demo` shows the same on the demo's bubbles. Its clicks use the real platform: the review opens example.atlassian.net.

Review fixes: d362ded (tooltip docs and a test step), 72a4fb9 (Windows lets the mouse through outside hit_rects: WS_EX_TRANSPARENT|WS_EX_LAYERED while the pointer is outside, decided by the pure function ignores_mouse).

stage: impl-review - ran (codex: NEEDS_WORK then SHIP in 3 rounds)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 87cfb37f211db780d149b2d7870c85363f7fc807, d362ded, 72a4fb9
- Tests: baseline: green at e3c3e82 (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: 337 passed, 1 ignored; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 52 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: green at 87cfb37 (347 passed, 1 ignored; aipet-ui 51 unit + 7 in tests/actions.rs) - green receipt 87cfb37f-unittest, dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: green at 87cfb37 (160 passed, 52 skipped; no C# change) - green receipt 87cfb37f-dotnet, cargo clippy --offline -p aipet-ui -p aipet-wayland -p aipet-desktop --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings (task 15 stand-ins in %TEMP%	15x, RUSTC_BOOTSTRAP=1): clean at 87cfb37, cd rust && cargo clippy --workspace --all-targets -- -D warnings: clean; cargo clippy -p aipet-ui [--features demo] --all-targets -- -D warnings: clean, mutation checks (15, each reverted): every one failed a test, offscreen renders of pet_view through iced headless wgpu (no window): C:/Users/MJE/Documents/projects/ai-pet/.flow/tmp/wave-2026-09-29/task-17-shots/*.png, integrated verify (Windows, work branch 1d84482 with tasks 16, 17 and 18 and their review fixes merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; at fd770c6: dotnet build and test 160 passed, 53 skipped, and AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server green; the full gate's antivirus failures under load (two C# RegistrationTests with 'Access to the path is denied', the live registration and doctor replays, claude_registration_is_the_csharps) all passed when rerun alone at 73c1310 and f0ebc65
- PRs:
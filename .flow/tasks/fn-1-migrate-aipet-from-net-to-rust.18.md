---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.18 Platform services for Linux and Windows: focus, open, media

## Description
Implement the `Platform` trait with the C#'s mechanisms: xdotool/wmctrl/xdg-open and MPRIS on Linux; Win32 focus and
Spotify's window title on Windows. macOS stays a stub, as in the C#. There is no send-Escape service: the Stop button it
served is dropped (the user's decision, 2026-09-30; see the spec's Decision Context), so the pet never sends keys to
another app.

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/platform/{mod,linux,windows,mac}.rs` (stubs from task 15), `rust/crates/aipet-desktop/tests/platform.rs`
**Touches:** [rust/crates/aipet-desktop/src/platform/**, rust/crates/aipet-desktop/tests/platform.rs]

### Approach
- Task 15's groundwork declares the platform modules in `lib.rs`, hands them to the UI in `main.rs` and adds the
  windows-sys features they need, so this task fills the stubs without touching `lib.rs`, `main.rs` or a Cargo file.
- Linux (`src/AiPet.UI/Platform/LinuxPlatform.cs`):
  - `FocusAgent` through `xdotool`, falling back to `wmctrl` (`:28-70`; not `SendEscape`); `xdg-open` for URLs and
    folders.
  - `Mpris` (`:334-421`): `playerctl` first, `dbus-send` as the fallback.
  - The same `Sh` helper, with its timeouts (`:214-265`).
- Windows (`src/AiPet.UI/Platform/WindowsPlatform.cs`):
  - Find agent windows by exe name with `EnumWindows` (`:73-97`).
  - Bring them forward with `SetForegroundWindow`, and `AttachThreadInput` when that fails (`:58-71`).
  - Not `SendEscape` and its `keybd_event` helper (`:99-118`).
  - Open with `ShellExecuteW`.
  - Spotify (`:231-303`): read the title of a `Chrome_WidgetWin*` window, and control it with `WM_APPCOMMAND`.
- The Wayland and winit shells both use this implementation.

### Investigation targets
**Required:**
- `src/AiPet.UI/Platform/LinuxPlatform.cs:1-265, 334-421`
- `src/AiPet.UI/Platform/WindowsPlatform.cs:1-130, 231-303`
**Optional:**
- `src/AiPet.UI/Platform/MacPlatform.cs` — the stub to mirror
**From task 12 (2026-10-01):** `aipet_core::board::Media { name, song: Option<_>, artist, playing, track_since }` is
what the Board reads of IMediaPlayer. The players can fill it each second, as MainWindow polls media every 1 s; it
goes into `Sources::read(..., media, music_on)`.
**From task 15 (2026-10-01):** `aipet-desktop/src/platform/{windows,linux,mac}.rs` each have `pub struct
Windows/Linux/Mac` with `new()` and a no-op `impl Platform`; `platform::new()` picks one. Windows features added:
`Win32_System_Threading` (AttachThreadInput, OpenProcess, QueryFullProcessImageNameW) and
`Win32_System_SystemServices` (APPCOMMAND_*); EnumWindows, SetForegroundWindow, ShellExecuteW and WM_APPCOMMAND were
already there. aipet-core is a dependency, for aipet.log. The core thread polls the player every 1 s while Music is on.
## Acceptance
- [ ] Unit tests parse Spotify titles (`Artist - Song`, paused) and `playerctl`/`dbus-send` output.
- [ ] Manual checklist on Linux (X11 and XWayland apps) and Windows: focus, opening links and folders, and media
      controls behave as the C#.
## Done summary
On Linux and Windows the Rust pet now does what the C# pet's platform code did. A chat bubble brings the agent's desktop app to the front. Links and folders open in the desktop's own apps. The music bubble follows Spotify (on Linux, any MPRIS player) and its buttons control it. macOS keeps the C#'s stub, which opens links and folders.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

The task's code is one commit, dfcb860. Its subject says "(partial)" because yesterday's run stopped before the gates. The gates and cross-checks below ran on dfcb860 today, nothing needed fixing, and no further commit was made.

### What the pet does now
- **Linux** (`platform/linux.rs`, from LinuxPlatform.cs):
  - xdotool brings an agent's window forward. The pet waits until xdotool names it the active window (11 looks, 10 waits of 15 ms), and logs `couldn't bring <agent> to the front` if it never does.
  - Without an xdotool window, wmctrl tries. Without any window, the chat's link opens. Codex is looked for under ChatGPT's window class first.
  - xdg-open opens links and folders.
  - The player is read as MPRIS through playerctl, else dbus-send. The C#'s two regexes over dbus-send's output are ported by hand, with their exact matching rules.
  - Every tool runs under the C#'s 3 s limit (`linux::capture`). A tool still running then is stopped and counts as failed.
  - The media keys run on a thread of their own, as the C#'s run on the thread pool.
- **Windows** (`platform/windows.rs`, from WindowsPlatform.cs):
  - An agent's app is the first main window of its exe: claude.exe for Claude, and chatgpt.exe, else codex.exe, for Codex. The pet restores the window if it is minimised and brings it forward. When SetForegroundWindow refuses, the pet tries again through the foreground window's thread input.
  - Without a window, the chat's link opens, else `claude://`. Nothing opens ChatGPT. The C#'s three log lines are kept.
  - ShellExecuteW opens links and folders.
  - Spotify is the first titled `Chrome_WidgetWin*` window of a Spotify process. Its title says what plays ("Artist - Song"). A paused title ("Spotify Premium", "Advertisement", or a blank one) keeps the last track. WM_APPCOMMAND sends the media keys to that window alone.
- **macOS** (`platform/mac.rs`) mirrors MacPlatform.cs. `open` opens links and folders, and `focus_agent` opens the chat's link.
- **Shared code** (`platform/mod.rs`):
  - `Track` ports MediaPlayerBase.Set. The track's clock moves only for another song or artist.
  - `start` spawns a program and reaps it on a thread of its own.
  - `unix_now` is the C#'s millisecond Unix time.

### How it is tested
- Both platforms go through a seam:
  - `linux::Tools` covers the tools, the waits, the clock and the log;
  - `windows::Win32` has one method per Win32 call.
- The seams and the logic compile on every OS, so `tests/platform.rs` drives both platforms over fakes on Windows and in CI's Linux job. No test starts xdotool, xdg-open or a browser, or touches a real window or media key.
- 14 tests, plus an ignored helper, cover:
  - Spotify titles: `Artist - Song`, the first ` - ` splitting, paused titles, and Spotify closing, with the track's clock;
  - playerctl's output, and dbus-send's ListNames, Metadata and PlaybackStatus replies, from fixtures;
  - the exact command lines on Linux;
  - on a fake desktop, the window chosen, the attach sequence, the waits and the links;
  - `capture`'s exit code, output and time limit, with this test binary run again as the tool.
- 32 mutations of the sources each failed a test. The one detail no test pins is the quotes in `status.contains("\"Playing\"")`, which no realistic fixture can tell apart.

### Gates
- **Baseline: green**, at e3c3e82 before any edit. No receipt existed, so both gates ran in full.
  - The Rust quick command passed 337 tests (1 ignored), with clippy and fmt clean.
  - The C# suite passed 160 and skipped 52.
- **Verify, at dfcb860:**
  - The Rust quick command, run with `CARGO_NET_OFFLINE=true`, passed 351 tests: the 337, plus the 14 in tests/platform.rs. 2 were ignored: the baseline's one and the helper. Clippy and fmt were clean. Green receipt: dfcb8609-unittest.
  - The C# suite passed 160 and skipped 52. Green receipt: dfcb8609-dotnet.
  - `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- **Cross-target clippy with `-D warnings`** (task 15's recipe) is clean:
  - for aipet-wayland, aipet-desktop and aipet-ui on x86_64-unknown-linux-gnu;
  - for aipet-desktop and aipet-ui on aarch64-apple-darwin.
  - Cargo's cache now has rfd's crates for both targets, so these check the real crates.

### Choices to review
- **The player's name on Linux stays "Spotify".** `MediaPlayer::name()` lends a `&str`, and the UI reads it once at start, when the C#'s Name is "Spotify" too. The player heard since (e.g. "Vlc") rides in `poll()`'s `Media.name`, which the Board shows.
- **Linux media keys don't poll again at once,** as the C#'s do. The core polls each second, and the platform can't ask it to poll sooner. The bubble shows the change within a second, as the C#'s does on Windows.
- **Finding Spotify's processes on Windows.** The C# lists them every 10 s with GetProcessesByName, and the declared windows-sys features have no process listing. The Rust looks up the exe of each `Chrome_WidgetWin*` window's process (QueryFullProcessImageNameW) and caches the answer per process for 10 s. It finds a Spotify started in between at once.
- **Send's `lastTitle = null` isn't ported.** Re-reading an unchanged title sets the same track, so it changed nothing.
- **macOS opens a folder as one argument.** The C#'s `Process.Start("open", path)` split it at spaces, which breaks the data folder's `Application Support`.
- **`shell_open` refuses a target with a NUL in it,** as std refuses such an argument on Linux.
- **Each OS's build compiles both platforms' logic,** so every OS tests it. Only the Win32 calls are `cfg(windows)`. The linker drops the unused platform.

### Manual checks for the hands-on passes
None of these real calls ran here.

Windows (task 27). Run `(cd rust && cargo build -p aipet) && rust/target/debug/AiPet.exe` from the repository root, with no other pet running.
1. Leave the Claude desktop app open behind another window, and click a Claude chat bubble that has no deep link. Claude comes to the front, restored if it was minimised.
2. Close Claude and click again. `claude://` opens Claude, and aipet.log says `no claude window found; opening claude://`.
3. With ChatGPT open, click a Codex chat bubble without a deep link. ChatGPT comes forward. With ChatGPT closed, nothing opens and aipet.log says `no codex window found`.
4. Play Spotify and turn on Settings → Listen along. Within a second, the music bubble shows the song, with "♫ Artist". Pausing in Spotify shows "Paused · Artist".
5. Previous, Play/Pause and Next control Spotify only, even with another player running. Clicking the music bubble brings Spotify forward.
6. The ticket, pull request and error links open in the default browser. Settings → Open data folder and Avatars → Open folder open Explorer.

Linux (task 22), on X11 and with XWayland apps:
1. With xdotool installed, a Claude or Codex chat bubble without a deep link brings the app's window forward. When the window manager refuses, aipet.log says `couldn't bring claude to the front`.
2. Without xdotool but with wmctrl, the window still comes forward.
3. With neither, a chat's deep link opens in the browser.
4. Native Wayland windows aren't found. A chat with a link opens the link.
5. With playerctl, the music bubble shows Spotify's track, else any MPRIS player's under its own name, e.g. "♫ Vlc" when there's no artist. The buttons work, and clicking the bubble brings Spotify forward when wmctrl is installed.
6. Without playerctl (dbus-send only), the same.
7. xdg-open opens links, the data folder and the avatars folder.

### Follow-ups (outside these Touches)
- **Settings' "Listen along with …" label.** To follow the player heard as the C#'s does after polls, aipet-ui would need to read the name again, and the trait would need to give an owned name.

No Cargo or lockfile change, and nothing outside the Touches. No process this task started is still running.

Review fix (4c10519): MediaPlayer::name() returns the player the last poll named, and Settings reads it on each draw; a test polls VLC and expects 'Listen along with Vlc'.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP in 2 rounds)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: dfcb86098df9686aed7449ba0a27d3e6ca48b526, 4c10519
- Tests: baseline: green at e3c3e82, no receipt, so both gates ran (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: 337 passed, 1 ignored, clippy and fmt clean; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 52 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: green at dfcb860, CARGO_NET_OFFLINE=true (351 passed, 0 failed, 2 ignored: the baseline one and the tool helper; tests/platform.rs ran its 14; green receipt dfcb8609-unittest), dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: green at dfcb860 (160 passed, 52 skipped; green receipt dfcb8609-dotnet), cd rust && cargo clippy --workspace --all-targets --locked -- -D warnings: clean at dfcb860, cd rust && cargo clippy --offline -p aipet-wayland -p aipet-desktop -p aipet-ui --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings (task 15 stand-ins, RUSTC_BOOTSTRAP=1): clean at dfcb860, cd rust && cargo clippy --offline -p aipet-desktop -p aipet-ui --all-targets -Zbuild-std --target aarch64-apple-darwin -- -D warnings (task 15 stand-ins, RUSTC_BOOTSTRAP=1): clean at dfcb860, mutation checks on dfcb860 sources: 32 mutations of linux.rs, windows.rs and mod.rs each fail a test in tests/platform.rs; the one not caught: the quotes in status.contains("\"Playing\""), integrated verify (Windows, work branch 1d84482 with tasks 16, 17 and 18 and their review fixes merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; at fd770c6: dotnet build and test 160 passed, 53 skipped, and AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server green; the full gate's antivirus failures under load (two C# RegistrationTests with 'Access to the path is denied', the live registration and doctor replays, claude_registration_is_the_csharps) all passed when rerun alone at 73c1310 and f0ebc65
- PRs:
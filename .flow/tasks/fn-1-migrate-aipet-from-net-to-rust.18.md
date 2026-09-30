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
## Acceptance
- [ ] Unit tests parse Spotify titles (`Artist - Song`, paused) and `playerctl`/`dbus-send` output.
- [ ] Manual checklist on Linux (X11 and XWayland apps) and Windows: focus, opening links and folders, and media
      controls behave as the C#.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

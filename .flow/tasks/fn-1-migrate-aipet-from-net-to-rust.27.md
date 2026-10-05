---
satisfies: [R13]
---
# fn-1-migrate-aipet-from-net-to-rust.27 Windows hands-on parity pass with the real-data pet

## Description
Run the hands-on tests with the real-data pet on Windows 10 and 11, and fix what fails. **It needs the user** or a
Windows machine with real input. Split from task 22 so each fits an iteration. It doesn't wait for task 22 (the user's
decision, 2026-09-30): the two passes own separate files and can run side by side. The shared
`aipet-desktop/src/shell.rs` is in neither pass's Touches: a fix there is reported, and the conductor adds the file to
this task's Touches while task 22 isn't running. This task can't be completed while a Windows check fails.

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/{native/win32,platform/windows}.rs`, `rust/proofs/windows-hands-on.md` (results, new)
**Touches:** [rust/crates/aipet-desktop/src/native/win32.rs, rust/crates/aipet-desktop/src/platform/windows.rs, rust/crates/aipet-ui/src/style.rs, rust/crates/aipet-ui/src/menu.rs, rust/crates/aipet-ui/src/view.rs, rust/proofs/windows-hands-on.md]

### Approach
- Use task 13's chosen transparency route (`rust/proofs/windows.md`). Check:
  - poke, drag and landing at 100 % and 150 %, and across two monitors;
  - the menu inline above the pet, and click-through in the gaps between bubbles;
  - topmost, and staying out of Alt+Tab and the taskbar;
  - Open and media buttons against the Claude and Codex desktop apps and Spotify (there is no Stop button);
  - the Settings pages, including updates from task 21;
  - antivirus reactions to the new `AiPet.exe` and `aipet-hook.exe`, recorded for the false-positive reports.
- Fix failures here. A failure too big for this task becomes a new task under this spec. That new task must be added
  as a dependency of this task and of task 23.

### Investigation targets
**Required:**
- `rust/proofs/windows.md` — task 13's results and fallbacks
- `rust/SPIKE.md` — the hands-on tests for Windows
**Optional:**
- `src/AiPet.UI/Platform/WindowsPlatform.cs` — the behaviour to match

**From task 13's proof (2026-09-29)** ([rust/proofs/windows.md](../../rust/proofs/windows.md)):
- **Crossing monitors at different scales fails.** Dragging between a 150 % and a 100 % monitor flips the scale up to
  7 times at the edge, and the pet jumps to the far side before snapping back. The window's larger part decides its
  DPI, and its size changes with the DPI. Chosen fix: the win32 subclass holds `WM_DPICHANGED` while the drag's button
  is down and applies the window's DPI at the drop, placed so the grabbed spot stays under the pointer.
- **Re-run by hand:** ending a drag on focus loss (pressing Win mid-drag used to leave the pet following the pointer),
  and the `AIPET_DEBUG` GPU probe moved to before the window shows. Both are unit-tested only.
- **Not shown by hand at all** (found when the proof was checked against its logs): the menu at 150 % (it was only
  opened after the pet had moved to the 100 % screen), a drag onto the other screen at 100 %, and click-through with
  each backend (only the default, Vulkan, was reported), GL included. On the test machine the second screen is a
  portrait one to the right of the main one, so the crossings seen so far were sideways.
- **Menu rows:** each item's label and check mark sit at the top of its 32 px row, 6–7 px above centre, because an iced
  button lays its content out from the top. Centre them in `menu.rs` (for example `container(..).height(Fill)
  .align_y(Center)`). The cause is iced's layout, so the fix applies on Linux as well.
- **Line height:** `style::LINE_HEIGHT` stays 1.362 (Noto Sans) on Windows, where Segoe UI's is 1.330. `view.rs`
  hard-codes 1.362 in `TITLE_Y` and `DETAIL_Y`; derive both from `style` to make Windows exact.

**From task 15 (2026-10-01):** task 15's manual checks 1, 2, 4, 5 and 7 are this pass's, unless the user has done them by
then: real Claude and Codex chats in the Rust pet, the .NET and Rust pets never together in either order or at the same
moment, the demo, the groundwork run, and the release build opening no console window (the exact commands are in task
15's done summary). The desktop shell's title and X11 class are `AiPet`, and Settings is `AiPet · Settings`.
## Acceptance
- [ ] Every check passes on Windows 11, recorded in `rust/proofs/windows-hands-on.md`. No failure is left open.
      Windows 10 is not tested: the user has no Windows 10 machine (the user's decision, 2026-10-05). The record
      says so, and which checks would differ there.
- [ ] Antivirus results are recorded, and any detections are reported to their vendors.


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

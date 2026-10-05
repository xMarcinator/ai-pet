---
satisfies: [R13]
---
# fn-1-migrate-aipet-from-net-to-rust.13 Windows proof for the winit shell: transparency, click-through, topmost, Alt+Tab, DPI, font

## Description
Run the spike's desktop shell on real Windows for the first time. Settle transparency, the window region, topmost,
hiding from Alt+Tab and the taskbar, dragging across DPI, and the font. This is an early proof point with no
dependencies. **It needs the user or a Windows machine.**

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/{shell,native/win32}.rs`, `rust/crates/aipet-ui/src/style.rs` (font fallback), `rust/proofs/windows.md` (results, new)
**Touches:** [rust/crates/aipet-desktop/src/shell.rs, rust/crates/aipet-desktop/src/native/**, rust/crates/aipet-ui/src/style.rs, rust/proofs/windows.md, rust/proofs/windows/*.png (widened by the conductor for the screenshots)]

### Approach
- Build and run the desktop shell on Windows 10 or 11 (`cargo run -p aipet-spike`) with DX12, Vulkan and GL, and record
  the surface's alpha modes.
- If the window isn't transparent with any of them, prototype the fallback: render on the CPU and present through
  `UpdateLayeredWindow`. Or find another wgpu composition path. Record the choice.
- Check each of the following, and fix what's small:
  - `SetWindowRgn` click-through, and flicker while regions update;
  - `HWND_TOPMOST`;
  - the `WS_EX_TOOLWINDOW` subclass;
  - dragging with `GetCursorPos` at 100 % and 150 %, and across two monitors at different DPI.
- Noto Sans isn't on Windows. Bundle it under OFL, or fall back to Segoe UI. Note the licence.

### Investigation targets
**Required:**
- `rust/SPIKE.md` — hands-on tests for Windows and the known limits
- `rust/notes/native.md` §2 (Windows) — the risks listed there
- `rust/crates/aipet-desktop/src/native/win32.rs`
**Optional:**
- `src/AiPet.UI/Platform/WindowsPlatform.cs:120-150` — what the C# does
## Acceptance
- [ ] `rust/proofs/windows.md` records results for each item above, with screenshots.
- [ ] Each item either passes or has a chosen fallback. The "Windows transparency route" parked unknown in the spec is
      resolved.
- [ ] CPU and RSS are measured on Windows.
## Done summary
The Windows proof is written up in rust/proofs/windows.md. It covers the user's hands-on runs on Windows 11 with
Intel UHD and an NVIDIA RTX 2000 Ada, a landscape main monitor and a portrait one above it. Two small fixes found by
those runs are in shell.rs. The task is still in_progress, as a parallel-wave task should be.

Results:
- **Transparency route, the parked unknown:** resolved. The plain wgpu window is transparent with Vulkan (the default
  here) and with GL. DX12 is opaque: its HWND swapchain ignores alpha, and iced 0.14 can't reach wgpu's DirectComposition
  path. No CPU or UpdateLayeredWindow fallback is needed.
- **Windows backend default:** recommended for task 15, not done here. Set `WGPU_BACKEND=gl` on Windows as well. GL
  shows the pet in 0.9 s where Vulkan takes 4.5-7.3 s, it doesn't load the NVIDIA driver, and it never picks DX12.
  The change belongs in the spike's main.rs, which is outside this task's Touches.
- **Passed:**
  - click-through (the window region), with no flicker at 14-18 region updates a second;
  - topmost;
  - out of Alt+Tab, Task View and the taskbar;
  - drags at 100 %, and at 150 % on one monitor;
  - 150 % rendering;
  - the first click, and a click with the pointer resting on the pet.
- **Failed, fixed here (d0ba9b5):**
  - Lost release: the pet kept following after the Start menu took the release. Losing the focus now ends a drag. A
    unit test covers it; it has not been re-run by hand.
  - A press arrived without a position once, while the AIPET_DEBUG GPU probe blocked the loop for 4.1 s. The probe
    now runs before the window is shown.
- **Failed, fix chosen for task 27:** dragging between the 150 % and 100 % monitors makes the scale flip up to 7
  times at the crossing. The window's majority area decides its DPI, and the window's size changes with the DPI. The
  chosen fix is to hold WM_DPICHANGED in the win32 subclass during a drag and replay it at the drop.
- **Font:** Segoe UI on Windows; nothing bundled and no licence to add (db3db70). The line height stays 1.362.
- **Follow-up:** the menu's labels and check marks sit 6-7 px above the centre of their rows. The cause is in
  menu.rs, which is outside Touches.
- **CPU and RSS (release, Vulkan, demo script):**
  - pet alone: 12.8 % of one core on average (median 10.7 %), working set 177.9 MB, private 140.1 MB;
  - with Settings open: 17.5 % (median 17.5 %), working set 215.1 MB, private 168.7 MB.

Screenshots: windows.md links seven screenshots at rust/proofs/windows/*.png. They still need copying there from
C:/Users/MJE/Pictures/aipet-task-13, which needs a Touches change. 01-taskbar.png should be left out: it shows the
user's pinned apps.

Gates at d0ba9b5:
- Rust quick command with the credential test skipped: 145 passed, clippy and fmt clean.
- `dotnet test AiPet.slnx -p:UseAppHost=false`: 160 passed, 42 skipped.
- Both have green receipts.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

After the hand-back: the conductor added the seven screenshots (1618f3d, Touches widened), and a proof check against the raw logs and code confirmed 18 errors, all corrected in 8fcac16: the second monitor is to the right, click-through was reported with the default backend only, and the drag onto the other monitor at 100 % and the menu at 150 % weren't shown by hand (now in Limits and task 27). The review's fix 565162e makes the GPU probe resolve Auto as wgpu does; 58c81a4 fixes the re-run section's paths.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fix 565162e)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: db3db70e652c0d500ffd5e18188ff8c08e290a79, d0ba9b58fff7769e4acf14bf1a9765d52154195b, 1618f3d, 8fcac16, 565162e, 58c81a4
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check; 144 passed, pre-edit), cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps && cargo clippy --workspace --all-targets && cargo fmt --all -- --check (at d0ba9b5: 145 passed, 1 filtered, clippy and fmt clean; green receipt d0ba9b58-unittest), dotnet build AiPet.slnx -p:UseAppHost=false; dotnet test AiPet.slnx -p:UseAppHost=false (at d0ba9b5: 160 passed, 42 skipped, 0 failed; green receipt d0ba9b58-dotnet), red first: cargo test -p aipet-ui --lib style:: failed with Windows has no "Noto Sans" before the font change, red first: cargo test -p aipet-desktop --lib focus failed (assertion shell.drag.is_none()) before the focus-loss change, hands-on (user, 2026-09-29, runbook .flow/tmp/wave-2026-09-29/task-13-runbook.md on db3db70 release): logs .flow/tmp/wave-2026-09-29/task-13-logs/, screenshots C:/Users/MJE/Pictures/aipet-task-13/; results in rust/proofs/windows.md, INCONCLUSIVE: the lost-release fix and the moved GPU probe (d0ba9b5) are unit-tested only, not re-run by hand, proof check (conductor workflow, 2026-09-29): two independent checkers compared rust/proofs/windows.md with the raw logs, the runbook, the code and the user's reports (200 claims); a skeptic tried to refute each of the 28 discrepancies raised: 18 confirmed, 10 refuted; all 18 corrected in 8fcac16 (for example, the second monitor is to the right, not above; click-through was reported with the default backend only; the drag onto the other monitor at 100 % and the menu at 150 % were not shown), integrated verify (Windows, work branch 74c7e76 with the review work merged): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped, INCONCLUSIVE (hands-on): d0ba9b5's lost-release fix and moved GPU probe, the menu at 150 %, the drag onto the other monitor at 100 %, and click-through with each backend were not shown by hand; recorded in the proof's Limits and in task 27
- PRs:
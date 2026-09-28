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
**Touches:** [rust/crates/aipet-desktop/src/shell.rs, rust/crates/aipet-desktop/src/native/**, rust/crates/aipet-ui/src/style.rs, rust/proofs/windows.md]

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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

---
satisfies: [R13]
---
# fn-1-migrate-aipet-from-net-to-rust.27 Windows hands-on parity pass with the real-data pet

## Description
Run the hands-on tests with the real-data pet on Windows 10 and 11, and fix what fails. **It needs the user** or a
Windows machine with real input. Split from task 22 so each fits an iteration, and ordered after it because both fix
the desktop shell. This task can't be completed while a Windows check fails.

**Size:** M
**Files:** `rust/crates/aipet-desktop/src/{shell,native/win32,platform/windows}.rs`, `rust/proofs/windows-hands-on.md` (results, new)
**Touches:** [rust/crates/aipet-desktop/src/**, rust/crates/aipet-ui/src/style.rs, rust/proofs/windows-hands-on.md]

### Approach
- Use task 13's chosen transparency route (`rust/proofs/windows.md`). Check:
  - poke, drag and landing at 100 % and 150 %, and across two monitors;
  - the menu inline above the pet, and click-through in the gaps between bubbles;
  - topmost, and staying out of Alt+Tab and the taskbar;
  - Stop, Open and media buttons against the Claude and Codex desktop apps and Spotify;
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

## Acceptance
- [ ] Every check passes on Windows 10 and 11, recorded in `rust/proofs/windows-hands-on.md`. No failure is left open.
- [ ] Antivirus results are recorded, and any detections are reported to their vendors.


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

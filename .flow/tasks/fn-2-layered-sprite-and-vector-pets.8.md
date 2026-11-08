---
satisfies: [R14, R3, R7, R9, R11, R13, R16, R17, R22]
---
# fn-2-layered-sprite-and-vector-pets.8 Performance, memory and hands-on verification across pack kinds

## Description
Measure calm CPU and RSS for every built-in pet (Sprout, Hood, Hood (green) pixel and vector, vector Pip, painted Pip, Hamster pixel and vector, Sprout vector) on Windows and Linux with fn-1's performance script, against fn-1's R18 budgets. Check the binary-size budget. Then run a Windows hands-on pass with the user: every pet's look, eye-follow, props, headphones, effects, click-through on the art, crisp pixels, reduced motion and the Settings page. Record results in a proof file.

**Size:** M
**Files:** `scripts/perf/**`, `rust/proofs/sprite-packs.md` (new)
**Touches:** [scripts/perf/**, rust/proofs/sprite-packs.md]

### Approach
- Reuse fn-1 task 28's script and method (60 s settle, 5 paired 30 s samples, medians); add a pet-selection parameter.
- Binary size: compare release `AiPet.exe` sizes before and after the embedded packs.
- Hands-on: follow the checklist shape of `rust/proofs/windows-hands-on.md`. Use the demo pet for repeatable scenes.
- Fixes found here go into small commits; a budget miss on one pet goes back to its task.

### Investigation targets
**Required:**
- `rust/proofs/performance.md`, `rust/proofs/memory.md` (from fn-1 tasks 28 and 29) — baselines and method
- `rust/proofs/windows-hands-on.md` — checklist format

### Key context
Windows rules: Git Bash only, never PowerShell; GUI checks need the user present.

## Acceptance
- [ ] All nine built-in pets meet the calm-CPU and RSS budgets on Windows and Linux, recorded with numbers
- [ ] Embedded art adds ≤ 3 MB to the release binary
- [ ] Windows hands-on pass recorded with the user, covering looks, behaviours, click-through, crispness, reduced motion and Settings

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

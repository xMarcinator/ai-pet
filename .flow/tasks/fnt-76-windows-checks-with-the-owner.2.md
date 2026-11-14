---
satisfies: [R2, R3]
---
# fnt-76-windows-checks-with-the-owner.2 Every pet on Windows, and the hands-on pass

## Description
The Windows half of fn-2 task 8: each built-in pet's calm CPU and memory, then the hands-on pass with the owner.

**Tooling ready (2026-11-14, tested on Linux only):** `scripts/perf/pet-calm-windows.sh` (pet-calm.sh's method on the sampler) and `scripts/perf/pets-calm-windows.sh`, the loop over every built-in pet with the C# pet first and last.

**Size:** M
**Files:** `scripts/perf/pets-calm-windows.sh`, `rust/proofs/sprite-packs.md`, `rust/proofs/windows-hands-on.md`
**Touches:** [scripts/perf/**, rust/proofs/sprite-packs.md, rust/proofs/windows-hands-on.md]

### Approach
- Per-pet runs as `scripts/perf/pets-calm.sh` does on Linux, with task 1's sampler; the C# pet first and last for drift.
- Hands-on checklist in `rust/proofs/windows-hands-on.md`'s shape, covering R3's list; the demo pet for repeatable scenes.

### Investigation targets
**Required:**
- `scripts/perf/pets-calm.sh`
- `rust/proofs/sprite-packs.md` - the Linux figures and the to-do list
- `rust/proofs/windows-hands-on.md` - checklist format

## Acceptance
- [ ] Every built-in pet's Windows figures recorded beside the Linux ones, within budget
- [ ] The hands-on pass recorded item by item

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

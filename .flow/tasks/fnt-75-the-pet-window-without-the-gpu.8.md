---
satisfies: [R8, R10, R11]
---
# fnt-75-the-pet-window-without-the-gpu.8 Measurements, hands-on passes and docs

## Description
fn-1's gate and the per-pet calm runs on the new stack on Linux and Windows, the hands-on passes, and the docs.

**Size:** M
**Files:** `rust/proofs/{performance,memory,sprite-packs,ui-framework,linux-hands-on,windows-hands-on}.md`, `README.md`, `docs/ARCHITECTURE.md`, `docs/HANDOFF.md`, `CHANGELOG.md`, `THIRD-PARTY-NOTICES.md`, `rust/Cargo.toml` comments
**Touches:** [rust/proofs/**, README.md, docs/**, CHANGELOG.md, THIRD-PARTY-NOTICES.md, rust/Cargo.toml, rust/crates/*/Cargo.toml, rust/SPIKE.md, rust/notes/**]

### Approach
- `scripts/perf/pet-bench.sh` and `pets-calm.sh` on Linux; the Windows script with the owner; record against R18 and R1's budgets.
- `grep` the pet's `/proc/<pid>/maps` for GPU libraries with Settings open and closed.
- Hands-on on Hyprland, X11 and GNOME/XWayland, and Windows with the owner: looks, moods, drag both modes, click-through, menu, scale changes, output loss, Settings live updates.
- Docs per the docs-gap findings in `## Resolved via Research`: ARCHITECTURE §3.6 renderers and the Rust window, HANDOFF's native-Wayland item and decisions, README's Wayland/XWayland and Hyprland text, CHANGELOG, THIRD-PARTY-NOTICES (sctk, cosmic-text and its font crates, x11rb, windows-sys, wayland crates), superseded notes in memory/windows proofs, SPIKE.md and notes.
- Re-scope fn-3's hands-on to the new shells.

### Investigation targets
**Required:**
- `scripts/perf/pet-bench.sh`, `scripts/perf/pets-calm.sh`
- `rust/proofs/performance.md`, `rust/proofs/memory.md`
- `THIRD-PARTY-NOTICES.md:40-55`

## Acceptance
- [ ] R18 and R1's budgets met on Linux and Windows, recorded with numbers
- [ ] No GPU library in the pet process, recorded
- [ ] Hands-on passes recorded; every doc in R11 updated

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

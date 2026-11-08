---
satisfies: [R2, R3, R4]
---
# fnt-70-rust-build-hygiene-hakari-sccache-and.2 sccache across worktrees, target sweeps after gates, and the measurements

## Description
Share dependency compiles across worktrees with sccache, sweep stale output after gates, and measure the result (R2, R3, the second half of R4).

**Size:** S
**Files:** `scripts/build-env.sh` (new), `scripts/sweep.sh` (new), `rust/proofs/build-hygiene.md`, the contributor docs

### Approach
- `scripts/build-env.sh`, sourced: when `sccache` is on PATH, `export RUSTC_WRAPPER=sccache SCCACHE_CACHE_SIZE=${SCCACHE_CACHE_SIZE:-20G}` and print one line; else print that plain cargo is used. Install sccache and cargo-sweep with `cargo install --locked` (ask the owner before installing system packages; cargo installs into ~/.cargo are fine).
- `scripts/sweep.sh [checkout]`: find `cargo`/`rustc` processes whose cwd is inside the checkout (`/proc/<pid>/cwd` on Linux; on Windows Git Bash, `ps -W`); refuse with exit 1 if any; else `cargo sweep --time ${DAYS:-7}` and `cargo sweep --installed` in `<checkout>/rust`, printing the size before and after.
- Run it at the end of each gate: document it where the gates are (the flow-next work loop, the waves' scripts under `../ai-pet-waves` if they have one).
- Measure: the main checkout's target before and after a sweep; a clean build in a second worktree (`git worktree add`, then remove it) with `sccache --show-stats` before and after, giving the hit rate; record in the proof.

## Acceptance
- [ ] With `scripts/build-env.sh` sourced, a second worktree's clean build hits sccache for the dependencies; the hit rate is recorded
- [ ] `scripts/sweep.sh` frees stale output and refuses while cargo runs in that checkout (both shown)
- [ ] `rust/proofs/build-hygiene.md` has the before-and-after numbers: target size, duplicate copies, clean build time, hit rate


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

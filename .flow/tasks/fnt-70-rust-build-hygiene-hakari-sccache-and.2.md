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
- [x] With `scripts/cargo-env.sh` (which replaced the planned `build-env.sh`) sourced, a second worktree's clean build hits sccache for the dependencies; the hit rate is recorded
- [x] `scripts/sweep.sh` frees stale output and refuses while cargo runs in that checkout (both shown)
- [x] `rust/proofs/build-hygiene.md` has the before-and-after numbers: target size, duplicate copies, clean build time, hit rate


## Done summary
R2: the opt-in build environment is scripts/cargo-env.sh (+ cargo-env.ps1, tests in scripts/test-cargo-env.sh), merged in bfdbd75; it replaces the planned scripts/build-env.sh, which was not added. In a second worktree at the same commit, a clean build hit sccache 85.6 % (Rust 84.7 %) against 35.0 % in the first worktree, but saved only about 5 s of 85 s on 24 threads (binaries, proc-macros, incremental workspace crates and linking aren't cached).
R3: scripts/sweep.sh [checkout] runs `cargo-sweep sweep --time DAYS` (default 7) and `--installed` in <checkout>/rust and prints the size before and after; it refuses with exit 1, "not sweeping: cargo is running in <checkout>", while cargo/rustc/rustdoc/clippy-driver works inside the checkout (/proc cwd, lsof, any build on Windows Git Bash). It refused during a real cargo build, and a real sweep freed 431 MiB (1.8 -> 1.3 GiB) of stale output, with a rebuild afterwards compiling nothing. mise.toml pins cargo-sweep 0.8.0; scripts/test-sweep.sh (20 cases, fake cargo-sweep and fake build) runs in CI under dash and bash.
R4: rust/proofs/build-hygiene.md: target 42 GB on 2026-11-08 (9.0 GiB freed on 2026-11-10); hakari cuts a per-crate-then-workspace sequence from 100 s to 74 s, extra crate copies from 86 to 66 and the target from 11 to 10 GB, and gains nothing on whole-workspace builds (84 vs 85 s, 59 vs 58 extra copies, syn 7 copies both); the sccache hit rates above. Docs: README (sweeping paragraph, scripts table), docs/ARCHITECTURE.md.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: cb2b00f2d8b201677b5a3e8c4cf3a59f8dc372f2, d3398d2f68a7e063264401e5c323ea01b2f2bdb9, 79013787bbecef10821c2028d8b723458e8aba57
- Tests: sh scripts/test-sweep.sh, sh scripts/test-cargo-env.sh
- PRs:
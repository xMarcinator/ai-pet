# Build hygiene: hakari, sccache and sweeps (FNT-70)

What the workspace-hack crate (cargo-hakari), the shared compiler cache (sccache) and the sweep script do to the size
of `rust/target` and to build times, measured on 2026-11-10.

Machine: Arch Linux, 24 threads, 62 GB of memory, rustc 1.99.1, sccache 0.19.0. Every build ran from an empty target
in a temporary worktree, inside `systemd-run --user --scope -p MemoryMax=12G -p MemorySwapMax=0`, one at a time.
"Before" is ab856a3, the last commit without the hack crate; "after" is bfdbd75, the merge that brought it and
sccache. Each number is one run, so a few seconds either way is noise.

## Where it started

`rust/target` in the main checkout reached **42 GB** on 2026-11-08, after it had filled the disk that morning. On
2026-11-10, two days after a clean, `cargo clean` freed **9.0 GiB** there. Each worktree keeps its own target, so every
extra worktree adds another one of these.

## The hack crate: one feature set

**Per-crate builds**, the loop of working on one crate and then checking everything: `cargo test --no-run` for
aipet-core, aipet-sprite, aipet-mcp and aipet-testkit one at a time, then for the whole workspace.

| Step | Before | After |
|---|---|---|
| `-p aipet-core` | 12.6 s | 13.0 s |
| `-p aipet-sprite` | 7.6 s | 5.6 s |
| `-p aipet-mcp` | 31.3 s | 11.9 s |
| `-p aipet-testkit` | 1.2 s | 0.7 s |
| `--workspace` | 47.8 s | 42.9 s |
| **All five** | **100 s** | **74 s** |
| Compiled libraries (`.rlib`) at the end | 406 | 387 |
| Crates compiled more than once | 67 | 58 |
| Extra copies | 86 | 66 |
| Target | 11 GB | 10 GB |

Before, `-p aipet-mcp` recompiled 84 libraries that the crates before it had already built with other features; after,
it adds 64. The most duplicated crates before were syn (6 copies), serde_json and serde_core (4 each), and ureq,
thiserror, smallvec and serde (3 each); after, syn (5), and thiserror, smallvec, hashbrown and getrandom (3 each).

**Whole-workspace builds** (`cargo build --workspace`, `cargo test --workspace --no-run`, `cargo clippy --workspace`,
then `cargo build -p aipet-sprite`) gain nothing: 84 s and 11 GB before, 85 s and 11 GB after, 379 libraries both
times, 59 and 58 extra copies, syn at 7 copies both times. A workspace build already unifies features across its
members, so there's nothing for the hack crate to add.

What's left is not the hack crate's to fix: three major versions of syn (1, 2 and 3) are in the lock file, proc-macros
and build scripts compile for the host apart from the target, the dev-dependencies of tests add features, and the
crates hakari leaves out (the hook, aipet-ipc and the front end) resolve on their own.

## sccache across worktrees

The same whole-workspace sequence, with `eval "$(scripts/cargo-env.sh)"` and `sccache --zero-stats` first: once in a
first worktree, then in a second worktree at the same commit, each from an empty target. The cache already held about
15 GB from earlier work (its cap).

| | First worktree | Second worktree |
|---|---|---|
| Cache hits | 413 | 914 |
| Misses | 766 | 154 |
| Hit rate | 35.0 % | **85.6 %** (Rust 84.7 %, C and assembly 100 %) |
| Time of the sequence | 102 s | 80 s |
| Same sequence without sccache | 85 s | 85 s |

The second worktree took dependencies from the cache, as intended. The time saved is small on this machine: the
first run pays for writing the cache (102 s against 85 s without), and the second saves only about 5 s against no
cache. Much of each build can't be cached at all: of the second worktree's 1,381 compile requests, 94 were binaries,
72 proc-macros, 54 incremental builds of workspace crates and 23 other linked crate types, and linking itself is never
cached. The gain grows with fewer cores and with dependency-heavy builds; with 24 threads, compiling is rarely what a
build waits on.

## The sweep

`scripts/sweep.sh` refuses while cargo works in the checkout. Run against a worktree while `cargo build -p aipet-core`
ran in it, then against the main checkout (no target) and the same worktree after the build:

```
$ scripts/sweep.sh ../ai-pet-fnt70-b      (while cargo build -p aipet-core runs there)
not sweeping: cargo is running in /home/marcinator/Documents/projects/ai-pet-fnt70-b
exit 1
$ scripts/sweep.sh                        (this checkout, nothing building)
nothing to sweep: /home/marcinator/Documents/projects/ai-pet/rust/target doesn't exist
exit 0
$ scripts/sweep.sh ../ai-pet-fnt70-b      (after the build, cargo-sweep not installed)
sweep.sh: cargo-sweep not found; mise install, or cargo install cargo-sweep --version 0.8.0 --locked
exit 2
```

`scripts/test-sweep.sh` covers the rest with a fake cargo-sweep: both sweeps run in `rust/` with `--time 7` (or
`DAYS`) and `--installed`, the size before and after is printed, a build in a folder that only shares the checkout's
name as a prefix doesn't block it, and bad arguments are refused. A sweep with the real cargo-sweep, and the space it
frees, is still to be recorded here (see the end).

## What to do

- Run `scripts/sweep.sh` after a gate; it's what keeps a long-lived target from growing back to 42 GB.
- Keep the hack crate: it saves about a quarter of the time of per-crate work and a fifth of the duplicate copies.
- sccache is worth sourcing for worktrees and for slower machines; on this one it saves little time, and its first
  build is slower.

## Still to record

The real cargo-sweep run: installing cargo-sweep 0.8.0 (`cargo install cargo-sweep --version 0.8.0 --locked`) needs
the owner's go, because it builds code downloaded from crates.io.

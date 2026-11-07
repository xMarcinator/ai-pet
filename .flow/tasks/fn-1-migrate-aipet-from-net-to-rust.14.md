---
satisfies: [R14]
---
# fn-1-migrate-aipet-from-net-to-rust.14 Velopack proof: a Rust package updates an installed .NET release in place

## Description
Prove, on Windows, whether an install packed from .NET with packId `AiPetApp` updates in place to a package whose exe
is Rust. Also prove that the Rust uninstall hook runs. Velopack doesn't document this, so it is an early proof point with
no dependencies.

**Size:** M
**Files:** `.github/workflows/velopack-proof.yml`, `rust/notes/prototypes/velopack-proof/{Cargo.toml,src/main.rs}`, `rust/proofs/velopack.md` (results, new)
**Touches:** [.github/workflows/velopack-proof.yml, rust/notes/prototypes/velopack-proof/**, rust/proofs/velopack.md]

### Approach
- A workflow_dispatch job on windows-2022:
  1. Pack the current .NET app as 0.1.98 into a local feed with the pinned `vpk`, then install its `Setup.exe` silently.
  2. Build a Rust stub called `AiPet.exe`. It calls `VelopackApp::build().run()` first, writes a marker in the data
     folder, and registers an uninstall callback that writes another marker.
  3. Pack the stub as 0.1.99 with the same packId: a full package, plus a delta against 0.1.98.
  4. Apply the update, using `Update.exe` or the installed app's own update path, whichever the docs support, with a
     local source.
  5. Check that `current\AiPet.exe` is the Rust one, the data folder is untouched, and a restart works.
  6. Uninstall, and check the uninstall marker.
- Record whether a delta works across runtimes, or the first Rust release must be full.

### Investigation targets
**Required:**
- `src/AiPet.UI/Updates.cs:43-170`, `src/AiPet.UI/Program.cs:10-25`
- `.github/workflows/release.yml:119-231` (the `vpk` steps)
- https://docs.velopack.io/getting-started/rust
**Optional:**
- `packaging/windows/README.md`, `tests/AiPet.Tests/VelopackTests.cs`
## Acceptance
- [ ] The proof workflow passes or fails with logs. Its result is written in `rust/proofs/velopack.md`.
- [ ] The spec's parked unknown about the update path is resolved: in place (full or delta), or a one-time reinstall
      with the steps written.
## Done summary
The Velopack proof passed and its results are in rust/proofs/velopack.md. An installed .NET 0.1.98 (packId AiPetApp) updates in place to a Rust-packed 0.1.99 by a full package and by a delta across the runtimes (584,265 B against a 2,660,151 B full). The Rust app's --veloapp-updated and --veloapp-uninstall hooks ran, and the data folder stayed byte-identical. The parked unknown resolves to "in place, delta works". No one-time reinstall is needed.

- Harness (e0bdff0): .github/workflows/velopack-proof.yml, rust/notes/prototypes/velopack-proof/ (Rust stand-in, a .NET update driver using the pet's own Velopack calls, proof.ps1). Run 36477504598 passed all three jobs.
- Results doc (3322916): rust/proofs/velopack.md with the method, the full and delta outcomes, the consequences for tasks 21 and 23, and the limits. The same commit fixes a wrong proof.ps1 comment (the Start menu entry points at current\AiPet.exe, not the launcher).
- For task 23: the Rust package must carry aipet-hook.exe at current\. An update replaces current\ wholesale, and install.ps1's registrations name %LOCALAPPDATA%\AiPetApp\current\aipet-hook.exe.
- For task 21: vpk checks VelopackApp.Run only in .NET exes. The Rust pet needs its own test that run() comes first.
- Follow-up for the conductor: the spec's "Parked unknowns" line about the update path can now say it was resolved in place (spec edits are outside this task's Touches).
- Gates on Windows: cargo green after re-checking out avatars/hood-green.json. The worktree still had its pre-merge CRLF copy. dotnet green with -p:UseAppHost=false (160 passed, 42 Unix-only skipped).
- Commit list: the task's own commits only (--first-parent --no-merges). The workspace also holds the conductor's merge 7248b70 of the work branch (2553c06, 242d7cc).

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

stage: impl-review - ran (codex: SHIP, 0 findings, no fix commits)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: e0bdff0bb6de9d99f112309028599edd5ec50a0f, 332291677a1ba19f17d7ace3a8984d463d20eaf2
- Tests: baseline: green (cargo) per the paused run's pre-edit baseline on Linux; dotnet test AiPet.slnx was an inherited env red there (5 ResourceTests/VelopackTests need src/AiPet.UI/bin), green after dotnet build AiPet.slnx, gate classify --base 88b528fb14c628ff0f13d0381fa8a0652ac63454: FULL (unmatched: .gitattributes, from the merged work-branch commit 242d7cc), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check (Windows, first run red: aipet-sprite golden tests, avatars/hood-green.json checked out CRLF before the merge brought 242d7cc's eol=lf attribute; re-checked out that file, no content change, then green: 91 tests passed, fmt clean, clippy exit 0 with pre-existing warnings in aipet-sprite/aipet-ui; receipt 33229167-unittest), dotnet build AiPet.slnx -p:UseAppHost=false --nologo -v q && dotnet test AiPet.slnx --no-build --nologo (Windows: 160 passed, 42 skipped = the Unix/Linux-only tests, 0 failed; receipt 33229167-dotnet-test), proof run https://github.com/xMarcinator/ai-pet/actions/runs/36477504598 at e0bdff0: success (pack, Update full, Update delta), integrated verify (Windows, work branch 42c4d13 = tasks 14 + 2 + the Rust 1.98 upgrade): cd rust && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check green; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped, 0 failed
- PRs:
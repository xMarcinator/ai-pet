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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

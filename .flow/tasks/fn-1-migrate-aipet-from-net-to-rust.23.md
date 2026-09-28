---
satisfies: [R15, R17]
---
# fn-1-migrate-aipet-from-net-to-rust.23 Ship the Rust pet: release pipeline, installers and CI (milestone 2)

## Description
Build and release the Rust pet in place of the .NET app, with the same artifacts and install paths. This is milestone
2. It waits for the Linux and Windows hands-on passes (tasks 22 and 27), the performance gate (task 28) and any
remediation task they add.

**Size:** M
**Files:** `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `install.sh`, `install.ps1`, `packaging/linux/install.sh`, `packaging/linux/aipet.desktop.in`, `scripts/install-from-source.sh`, `scripts/install-from-source.ps1`, `build.sh`, `build.ps1`, `rust/crates/aipet/build.rs`, `tests/AiPet.Tests/ReleaseWorkflowTests.cs`, `tests/AiPet.Tests/InstallerTests.cs`, `THIRD-PARTY-NOTICES.md`, `CHANGELOG.md`
**Touches:** [.github/workflows/**, install.sh, install.ps1, packaging/linux/**, scripts/**, build.sh, build.ps1, rust/crates/aipet/build.rs, tests/AiPet.Tests/ReleaseWorkflowTests.cs, tests/AiPet.Tests/InstallerTests.cs, THIRD-PARTY-NOTICES.md, CHANGELOG.md]

### Approach
- Windows:
  - `cargo build --release` for the pet (`AiPet.exe`, with version resources from `build.rs`) and the hook.
  - `vpk pack` with packId `AiPetApp` and main exe `AiPet.exe`: a full package if task 14 said so. `vpk` still needs the
    .NET SDK in this job.
  - The portable zip and the resource checks, as today.
- Linux: zigbuild for the pet and the hook for x64 and arm64. Record the pet's glibc floor and gate it, at 2.27 if the
  GUI dependencies allow, otherwise a documented floor. The tarball layout (`app/AiPet`, `app/aipet-hook`, installer,
  desktop entry, icons, Hyprland rules) is unchanged.
- Remove `dotnet publish` for the app. The .NET tests keep running until task 25.
- Installers and `install-from-source`: the same install and data paths, and the same uninstall behaviour.
- `THIRD-PARTY-NOTICES.md`: an interim list generated with `cargo-about` from the release crates. Task 26 finalises it.

### Investigation targets
**Required:**
- `.github/workflows/release.yml:1-555`
- `packaging/linux/install.sh`, `install.sh`, `install.ps1`
- `tests/AiPet.Tests/InstallerTests.cs`, `tests/AiPet.Tests/ReleaseWorkflowTests.cs`
**Optional:**
- `rust/proofs/*.md` — the results of tasks 13, 14, 22, 27 and 28
## Acceptance
- [ ] A release dry run produces every artifact in R15, and every gate passes. `InstallerTests` and
      `ReleaseWorkflowTests` pass.
- [ ] A clean Linux and Windows install works. An update from the milestone 1 release keeps the config, tokens and hook
      registrations.
- [ ] Rolling back to the milestone 1 release keeps the data (R17).
- [ ] CHANGELOG entry. The milestone 2 release is cut (the user starts it).
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

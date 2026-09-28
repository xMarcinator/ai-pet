---
satisfies: [R3, R7, R15, R17]
---
# fn-1-migrate-aipet-from-net-to-rust.6 Ship the Rust hook with the shared single-instance lock: build gates, release and install scripts (milestone 1)

## Description
Build and release the Rust hook in place of the NativeAOT one, for the plugin and inside the still-.NET app packages,
with the same gates. The .NET pet also gains the shared Linux single-instance lock, so a later Rust pet can never start
alongside it. This is milestone 1: the Rust hook ships with the .NET pet.

**Size:** M
**Files:** `.github/workflows/release.yml`, `.github/workflows/ci.yml`, `build.sh`, `build.ps1`, `scripts/install-from-source.sh`, `scripts/install-from-source.ps1`, `scripts/check-plugin.sh`, `rust/crates/aipet-hook/build.rs`, `src/AiPet.UI/Program.cs`, `tests/AiPet.Tests/SingleInstanceTests.cs` (new), `tests/AiPet.Tests/ResourceTests.cs`, `tests/AiPet.Tests/ReleaseWorkflowTests.cs`, `CHANGELOG.md`, `README.md` (build prerequisites)
**Touches:** [.github/workflows/release.yml, .github/workflows/ci.yml, build.sh, build.ps1, scripts/install-from-source.sh, scripts/install-from-source.ps1, scripts/check-plugin.sh, rust/crates/aipet-hook/build.rs, src/AiPet.UI/Program.cs, tests/AiPet.Tests/SingleInstanceTests.cs, tests/AiPet.Tests/ResourceTests.cs, tests/AiPet.Tests/ReleaseWorkflowTests.cs, CHANGELOG.md, README.md]

### Approach
- The shared Linux lock, part of the spec's single-instance contract (R7):
  - Before its mutex and socket probes, `Program.Main` (`src/AiPet.UI/Program.cs:18-37`) takes an exclusive,
    non-blocking `flock` on the spec's lock file: the socket path (`Ipc.Endpoint`) with `.lock` in place of `.sock`,
    so it follows the socket's own location rule, including `AIPET_PIPE`. Losing it quits quietly, as losing the mutex does.
  - The lock is held for the whole run and released by the kernel when the process exits.
  - Use a P/Invoke to `flock`, with the file opened 0600 and `O_NOFOLLOW`.
  - `SingleInstanceTests` runs two contenders at once and expects exactly one winner, also when they run with different
    `XDG_RUNTIME_DIR` values and with it unset. Task 15's Rust side contends on the same file.
- Linux (`release.yml:232-331`):
  - Build the hook with `cargo zigbuild --release --target {x86_64,aarch64}-unknown-linux-gnu.2.27` on ubuntu-22.04. It
    no longer needs the cross-build containers; the .NET app build keeps them.
  - Keep the `readelf -W -V` `GLIBC_` gate (`:284-299`) on the new binary.
- Windows (`:119-231`):
  - `cargo build --release --target x86_64-pc-windows-msvc`. `build.rs` embeds the icon and a version resource (with
    `winresource` or `embed-resource`), versioned from the release input.
  - Keep the resource check (`:161`), changed so the hook no longer needs NativeAOT proof.
- The plugin job copies the Rust hooks into `native/<rid>/`. The app packages (still .NET) include the Rust hook.
- `check-plugin.sh` and CI build the Rust hook and use its `--print-plugin-hooks`.
- `build.sh`/`build.ps1` and `install-from-source` build the hook with cargo. This is the user's own install path.
- Startup benchmark: in CI, a loop of 200 event-path runs with no pet, for the Rust and the NativeAOT hooks. Record the
  medians in the job summary, and fail if the Rust one is more than 10 % slower.

### Investigation targets
**Required:**
- `.github/workflows/release.yml:119-331` — the Windows and Linux jobs
- `.github/workflows/ci.yml` — the build and scripts jobs
- `scripts/install-from-source.sh`, `build.sh`
- `tests/AiPet.Tests/ResourceTests.cs`, `tests/AiPet.Tests/ReleaseWorkflowTests.cs`
**Optional:**
- `plugins/aipet/native/aipet-hook.sh` — the launcher's rid layout and loader checks
- `packaging/windows/README.md`

### Key context
- `cargo-zigbuild` can't statically link glibc (`+crt-static`) on a `.2.27` target. Link it dynamically; the gate checks
  the symbol versions.
- Release 0.1.0 used a deploy key and a pinned host key for the plugin repository. Those steps don't change.
## Acceptance
- [ ] A release dry run (workflow_dispatch on a test version, publish skipped) builds all three hook binaries. They pass
      the glibc gate and the resource check.
- [ ] Plugin files, `SHA256SUMS` and the tarball layout are unchanged apart from the hook binaries.
- [ ] The startup benchmark is within budget. `ResourceTests` and `ReleaseWorkflowTests` pass as updated.
- [ ] `install-from-source.sh --uninstall` and a reinstall work with the Rust hook on the developer's machine. Existing
      direct registrations keep working, and Codex doesn't ask to trust the hooks again.
- [ ] Two .NET pets started at the same moment on Linux give exactly one pet (the lock test, plus a manual check).
- [ ] CHANGELOG entry. The milestone 1 release is cut (the user starts it).
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

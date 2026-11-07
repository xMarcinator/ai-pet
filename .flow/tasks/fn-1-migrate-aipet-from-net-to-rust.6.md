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
    so it follows the socket's own location rule; under the `AIPET_PIPE` override it is `<AIPET_PIPE>.lock`, as the spec
    says. Losing it quits quietly, as losing the mutex does.
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
**From the conductor (2026-10-01):** CI's windows-2022 job fails `aipet-ipc`'s
`paths_equal_the_csharps_in_the_same_environment` and `aipet-core`'s `every_claude_case_matches_the_csharp` (the
`title-nul` case) until task 30 lands; both are test setup, not this task's. Task 15 writes the Rust side of the Linux
lock in parallel with this task, so test the C# lock against a lock taken the contract's way; task 23 tests the two
against each other. From task 5: the Windows Codex `--probe` scenarios start PowerShell and run only where `CI` is
set, and the managed-settings scenarios only with `AIPET_TEST_MANAGED_SETTINGS=1`; never set either on this machine.
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
The release, CI and the from-source scripts now build and ship the Rust `aipet-hook` in place of the NativeAOT one (465b425). The .NET pet now takes the shared Linux single-instance lock before its mutex. The Linux-only lock and workflow tests pass on Linux (WSL), and the full Windows gates are green. The release dry run, zigbuild, the glibc gate and the start-up benchmark run only in CI.

Tier: (not given in the dispatch)

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

### The Linux lock (src/AiPet.UI/Program.cs)
- **The rule, as the spec and the conductor's correction give it:**
  - By default the lock is the socket's path with `.lock` in place of `.sock` (`Path.ChangeExtension(Ipc.Endpoint, ".lock")`), so it follows the socket's rule: `/run/user/<uid>`, then `$XDG_RUNTIME_DIR`, then the data folder.
  - With `AIPET_PIPE` set it is `<AIPET_PIPE>.lock`: `.lock` is appended to the full path, even when it ends in `.sock` (`Ipc.Endpoint + ".lock"`).
  - This matches task 15's `single::lock_path`. `TheLock_IsNextToTheSocket` covers both forms.
- **How it's taken:** `open(O_RDWR|O_CREAT|O_CLOEXEC|O_NOFOLLOW, 0600)`, with O_NOFOLLOW's value per architecture, then `flock(LOCK_EX|LOCK_NB)`, retried on EINTR. The fd is never closed. Main takes it after Velopack's `Run` and before the mutex.
- **Losing (EWOULDBLOCK):** the pet quits quietly.
- **Deliberate difference from task 15:** any other failure (a symlink at the path, EACCES, ENOLCK) is logged to aipet.log, and the .NET pet goes on to the mutex and the socket probe, as before the lock existed. Task 15's Rust pet refuses to start instead. The two still never run together. This keeps the shipping .NET pet from failing to start in odd environments. The conductor may want them aligned.
- **SingleInstanceTests (new, Linux only, 7 tests):**
  - Each pet is `dotnet AiPet.Tests.dll` with DOTNET_STARTUP_HOOKS pointing at that same dll. Its `StartupHook` calls `Program.TakeLock` and `LockFile` from the built AiPet.dll by reflection, and never Main: no window, no mutex.
  - Two pets started at the same moment, with XDG_RUNTIME_DIR set to two folders, one folder or none: exactly one runs.
  - A lock taken the contract's way by the test and the pet's lock keep each other out.
  - A killed pet's lock is released by the kernel.
  - A symlink at the lock's path isn't followed.
  - The lock's path follows both rules above.
  - Mutation runs confirmed each test fails for its reason.

### Release and CI
- **release.yml, new `dry_run` input:** it runs on any branch and pushes and publishes nothing. The plugin commit stays local, and the plugin tree and SHA256SUMS are kept as artifacts. Use a never-released version, e.g. 0.2.0-dry.1.
- **Windows hook:** built with `cargo build --release --target x86_64-pc-windows-msvc`, `AIPET_VERSION=$VERSION` and `RUSTFLAGS=-C target-feature=+crt-static`.
  - Without crt-static the exe needs VCRUNTIME140.dll, which a clean Windows may lack and the NativeAOT hook never needed. A new check fails the step if any C runtime DLL name is in the exe.
  - The existing pwsh resource check stays, minus the NativeAOT proof.
- **Linux hooks:** a new `linux-hook` job on ubuntu-22.04 builds them with `cargo zigbuild --target <triple>.2.27`, linked dynamically, and runs the glibc gate there.
  - zig and cargo-zigbuild are pinned to ziglang 0.13.0 and cargo-zigbuild 0.19.8. Neither pin was tried here (no network), and the dry run will show whether both resolve.
  - The container `linux` job now only publishes the app, and downloads `hook-<rid>`.
- **rust/crates/aipet-hook/build.rs (winresource, no Cargo change):** embeds the .NET hook's icon (group 32512), version resource and asInvoker manifest. The resources are byte-identical to the .NET hook's: `ResourceTests.RustHook_CarriesTheDotnetHooksResources` checks this, and ci.yml runs it on Windows with AIPET_TEST_HOOK.
- **ci.yml build job:** checks the plugin files with the Rust hook's `--print-plugin-hooks`.
- **ci.yml `hook-startup` (new):** runs the Rust hook (built as the release builds it) and the NativeAOT hook 200 times each from bash, alternating, with no pet. It writes the medians to the job summary and fails if Rust is more than 10 % slower.
- **ReleaseWorkflowTests:** new tests for the dry-run branch check, the dry run's commit without a push, and the glibc gate. All pass on Linux.

### Scripts and docs
- `build.sh`, `build.ps1` and `install-from-source.sh` build the hook with cargo:
  - natively for the host, with crt-static on Windows;
  - for other Linux rids only with cargo-zigbuild, warning and leaving the hook out without it;
  - no win-x64 hook from Linux.
- `install-from-source.sh` and `install-from-source.ps1` count `rust/crates` and the Cargo files as sources in their stale check.
- `check-plugin.sh` docs name the Rust hook, including the `--hook cargo run …` form.
- The README lists Rust in the build prerequisites. The CHANGELOG has an Unreleased entry.

### Left for CI and the user
- **CI:** the release dry run (AC 1, 2), the start-up benchmark (AC 3), the PowerShell parse of build.ps1 and install-from-source.ps1, and shellcheck.
- **The user, on the developer's Linux machine:**
  - `install-from-source.sh --uninstall` and a reinstall (AC 4). The hook path is unchanged and the Rust registration is byte-identical, so Codex shouldn't ask for trust again.
  - The manual two-pets check (AC 5).
  - Cutting the milestone 1 release (AC 6).

### Follow-ups outside this task's Touches
These still describe the NativeAOT hook:
- docs/ARCHITECTURE.md (§6.2, §7, §10, lines ~23, 41, 617, 1019-1027, 1061-1068, 1086, 1217-1249);
- docs/HANDOFF.md:22;
- plugins/aipet/README.md:69;
- packaging/windows/README.md (steps 2-3, :131).

Notes for tasks 15, 21, 23 and integration: NOTES_DIR/task-6-rust-hook-release.md.

Review fix (2b6315a): the .NET pet now refuses to start on any lock error, not only contention, and logs why, as the Rust pet does; a test with an unusable lock file (a symlink or a folder) expects both pets to quit. `PetsStartedTogether_OnTheDefaultLock_OneRuns` makes two pets with AIPET_PIPE unset contend for the default lock, each in its own unshare user and mount namespace whose /run/user is a test folder (skipped where unshare can't make one).

Left open (pre-existing): release.yml's plugin job pushes the plugin branch and tag before the publish job computes SHA256SUMS.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fix 2b6315a)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 465b425c9375b3a7c2a903b189e026cacc2785af, 2b6315a591b1b5a8390a96b606c7c5ca1a140902
- Tests: baseline: green at 34a79e4 (cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check: 311 passed; dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped), cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check: green (Windows, rust 1.98.1; 311 passed, as at baseline; build.rs linted) - green receipt 465b425c unittest-rust, dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build: green (Windows; 160 passed, 51 skipped: the 9 new skips are the Linux/Unix-only SingleInstanceTests and ReleaseWorkflowTests entries and RustHook_CarriesTheDotnetHooksResources without AIPET_TEST_HOOK) - green receipt 465b425c unittest, AIPET_TEST_HOOK=rust/target/debug/aipet-hook.exe dotnet test tests/AiPet.Tests --no-build --filter <cross-runtime.yml's 10 hook classes + ResourceTests>: 42 passed, 21 skipped (Unix-only), 0 failed, AIPET_TEST_HOOK=<aipet-hook.exe built with RUSTFLAGS=-C target-feature=+crt-static --target x86_64-pc-windows-msvc> dotnet test --filter ResourceTests.RustHook: passed (resources byte-identical to the .NET hook's; imports only OS DLLs, no VCRUNTIME140.dll/api-ms-win-crt-*), Linux (WSL Ubuntu 24.04, .NET 10.0.12 from the local runtime pack, WSL SDK 8 vstest.console; tests built from LF sources) on 465b425: SingleInstanceTests 7 + ReleaseWorkflowTests 24 + PluginHooksTests 6 = 37 passed, 0 failed, mutation checks (WSL Linux): lock path from XDG_RUNTIME_DIR -> 6 of 7 SingleInstanceTests fail (two pets run, the contract lock doesn't keep the pet out, the path isn't next to the socket); open without O_NOFOLLOW -> ASymlinkAtTheLocksPath_IsNotFollowed fails (target made); release.yml without the dry-run gates -> Check_ReleasesFromMain_DryRunsFromAnyBranch(branch, true) and DryRun_PushesAndPublishesNothing(true) fail; Windows: hook built with AIPET_VERSION=9.8.7-rc.1 -> RustHook_CarriesTheDotnetHooksResources fails on resource (16, 1), bash -n build.sh scripts/install-from-source.sh scripts/check-plugin.sh, and of all 40 bash run steps of release.yml and ci.yml: clean; YAML parse of both workflows: ok; shellcheck not installed here (CI's scripts job runs it), bash scripts/check-plugin.sh --marketplaces . plugins/aipet --hook rust/target/debug/aipet-hook (and --hook cargo run -q --locked --manifest-path rust/Cargo.toml -p aipet-hook --): Plugin files OK, ci.yml hook-startup script logic dry-run locally with the debug Rust hook on both sides (20 runs): runs and summarises; this machine's antivirus adds ~300 ms per start, so it can't measure the budget - CI only, CI only, not run here: release.yml dry run (cargo zigbuild with ziglang 0.13.0 / cargo-zigbuild 0.19.8 pins, the glibc gate on the zigbuild hooks, the Windows hook build, the pwsh resource check), ci.yml's new steps (Rust hook resources on windows-2022, check-plugin with the Rust hook, the hook-startup benchmark), the PowerShell 7 / 5.1 parse of build.ps1 and install-from-source.ps1, shellcheck, integrated verify (Windows, work branch c324cd4, then 50b5ee2 with the review fix): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 52 skipped (the Linux-only SingleInstanceTests among them); AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests|ResourceTests: 23 passed, 10 Unix-only skipped; AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server: 15 passed; with AIPET_GOLDEN every live replay green when run alone. In the full gate right after the hook's rebuild, the server test claude_hook_forwards_the_event_and_its_environment_silently, one C# test on the Rust hook and the codex live replay each failed once (a fresh unsigned exe under the antivirus); each passed on rerun, only CI confirms: the release dry run (zigbuild with the ziglang 0.13.0 and cargo-zigbuild 0.19.8 pins, the glibc gate, the Windows build and its resource check), ci.yml's hook-startup benchmark, the PowerShell parse of build.ps1 and install-from-source.ps1, and shellcheck; the worker and the reviewers ran the Linux SingleInstanceTests (11, three of them in unshare namespaces) in WSL on a .NET 10.0.12 runtime assembled from the local NuGet runtime pack
- PRs:
---
satisfies: [R5]
---
# fn-1-migrate-aipet-from-net-to-rust.5 Port --doctor for Claude and Codex, including --probe

## Description
Port the read-only diagnostic `--doctor claude|codex [--probe]`, with output identical to the C#'s. It depends on the
registration code: the doctor reuses its tables and file readers.

**Size:** M
**Files:** `rust/crates/aipet-hook/src/doctor.rs`, `rust/crates/aipet-hook/tests/doctor.rs`, `rust/golden/Doctor.cs` (doctor scenarios, run through the built C# hook), `rust/crates/aipet-hook/tests/golden/doctor/**`
**Touches:** [rust/crates/aipet-hook/src/doctor.rs, rust/crates/aipet-hook/src/main.rs, rust/crates/aipet-hook/src/claude.rs, rust/crates/aipet-hook/src/codex.rs, rust/crates/aipet-hook/tests/doctor.rs, rust/crates/aipet-hook/tests/common/**, rust/crates/aipet-hook/tests/registration.rs, rust/crates/aipet-hook/tests/codex.rs, rust/crates/aipet-hook/tests/golden/doctor/**, rust/golden/Doctor.cs, .github/workflows/cross-runtime.yml]

### Approach
- Port `src/AiPet.Hook/Doctor.cs`:
  - Claude's settings and org-managed settings (the Windows, Linux and macOS paths), including `disableAllHooks` and
    `allowManagedHooksOnly`.
  - For Codex, drive `codex app-server` over stdio (`initialize` → `initialized` → `hooks/list`) with the C#'s timeouts,
    and fall back to reading the files.
  - `Lint` for Windows command strings (`:287-293`).
  - `Probe`/`ProbeCodex`: pwsh, then cmd on Windows; `/bin/sh -c`, then `$SHELL -lc` on Unix. `PetGot` looks for the
    probe event in the ping's recent list within 5 s.
- Golden scenarios run in a sandbox with a fake `codex` script on `PATH`: no pet; a pet running (the C# `TestEnv`
  server); the plugin enabled; managed settings disabling hooks; `codex` missing; `app-server` reporting trusted and
  untrusted hooks.

### Investigation targets
**Required:**
- `src/AiPet.Hook/Doctor.cs:1-456`
- `tests/AiPet.Tests/RegistrationTests.cs` — the doctor cases
**Optional:**
- `docs/ARCHITECTURE.md` §7 (checking)
**From task 3 (2026-09-29):** `RegistrationTests` now runs the hook through `TestEnv.HookStartInfo`, so its doctor
cases (`Doctor_KnowsLegacyCodexHooks`, `Doctor_WithoutHooksList_KnowsThePlugin` and the other `--doctor` runs) go to
`AIPET_TEST_HOOK` when it is set. Most are Unix-only, so Windows never runs them against the Rust hook. Once
`--doctor` is ported, add `RegistrationTests` to `cross-runtime.yml`'s class list so they run against it on Linux.
Task 3 left `--doctor <agent>` printing "isn't ported to this hook yet" with exit 1 (`main.rs`); replace that stub.
**From task 4 (2026-09-30):** `codex.rs` has `EVENTS` (the OS's list, with background and timeout), `snake`,
`is_ours` and `build_command` (pub(crate)). `CodexConfig.AllHandlers`/`TomlHandlers` aren't ported, since only the
doctor uses them: build them from `toml_text::parse` with codex.rs's `positions`, and `json_positions` with `command_of`
for hooks.json. `plugin_hooks::CODEX_EVENTS` stays the one copy of `PluginEvents`. `install.rs` now shares `backup`,
`real_path`, `full_path` and `local_stamp`, and `upper` follows .NET's ordinal casing; `claude.rs` keeps private copies
of the first four, so delete them and use install's (claude.rs is in this task's Touches for that). `tests/codex.rs`
repeats `tests/registration.rs`'s harness (Scratch, set_up, tree, the held-file rule): if the doctor's tests need it
too, move it to `tests/common/` rather than make a third copy. In the golden, normalise the doctor fixtures' line
endings (`ReplaceLineEndings`, as the Codex corpus does), so the output doesn't depend on the checkout's line endings.
**From the integrated checks (2026-09-30):** `tests/registration.rs`'s `a_mode_without_an_agent_is_a_usage_error`
failed once on a heavily loaded machine (three Codex reviews and builds running) and passed alone: it asserts the hook
exits within 2 s of being spawned, which a fresh, antivirus-scanned exe can miss under load. Since the test holds stdin
open, a hook that waited on stdin would hang rather than take 2 s; make that check robust (for example, a generous
bound, or detect the wait without a wall-clock limit) when the doctor's tests touch this file.
## Acceptance
- [ ] Doctor output and exit codes match the C#'s for every scenario, on Linux and Windows.
- [ ] `--probe` finds the probe event through a live pet, and reports its absence when there is no pet.
- [ ] A missing or failing `codex app-server` falls back to reading files. Timeouts are reported, never a hang.
## Done summary
Ported `aipet-hook --doctor claude|codex [--probe]` to the Rust hook. The rust/golden `doctor` mode runs each scenario through the built C# hook, and `tests/doctor.rs` replays them through the Rust hook in the same sandbox. All 26 Windows scenarios match byte for byte, and so does a live C# corpus written on this machine. On Linux the live replay in CI is the comparison, since the committed corpus is Windows-only. Also fixed a parity gap in task 3's Claude registration that CI's Linux replay found.

Tier: (not given in the dispatch)

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

### What changed (6a70f68 WIP, 73f7875)
- **src/doctor.rs** ports Doctor.cs line for line:
  - Claude's settings and the organisation's managed settings, on the Windows, Linux and macOS paths;
  - `codex app-server` (initialize, initialized, hooks/list, 30 s in all), falling back to reading config.toml and hooks.json;
  - `Features`, the `[[hooks.X]]` check, `Lint` (Windows) and `Short` (in UTF-16 units);
  - `--probe`: pwsh, else `C:\Program Files\PowerShell\7\pwsh.exe`, else `powershell.exe`, then `cmd /C` as a warning only on Windows; `/bin/sh -c` then `$SHELL -lc` on Unix;
  - `PetGot`, which waits up to 5 s for the event in the ping's recent list.
- **Timeouts:** they are reported ("no answer within 10 s", and the file fallback after 30 s). A program that runs out of time is ended with everything it started: a process group on Unix, a job object on Windows (a small kernel32 FFI block, since windows-sys has job objects behind a feature this crate doesn't enable and Cargo.toml is off limits).
- **Deliberate deviations,** in the module doc:
  - where the C# stops with an unhandled exception (a member named twice, a non-string field in hooks/list, a file it may not read), the port carries on;
  - a stdin write to a program that already quit isn't a start failure.
- **main.rs** dispatches `--doctor`.
- **codex.rs** has `files()`, `enabled_plugin()` and `all_handlers()`.
- **claude.rs** shares `dir()` and uses install.rs's `backup`, `real_path`, `full_path` and `local_stamp`; its own copies are gone.
- **rust/golden/Doctor.cs:** the `doctor` mode builds the C# hook with `-p:UseAppHost=false`, runs each scenario in a sandbox, and writes `tests/golden/doctor/doctor.json`:
  - the sandbox has a fake codex on PATH, a C# HookServer as the pet, and tokens for what varies;
  - a run where `dotnet` made the hook slow is set aside;
  - line endings are normalised (`ReplaceLineEndings`).
- **The scenarios:**
  - **Claude:** no settings; no config folder; unparsable; not an object; not registered; registered; registered with a pet; some events with hooks switched off; hook missing; the legacy hook.py; plugin; plugin not installed; plugin and direct; and four with managed settings.
  - **Codex, without a hook list:** codex missing (legacy hooks, nothing registered, plugin, plugin and direct); app-server failing; app-server silent (timeouts); `--version` failing; codex not executable (Unix).
  - **Codex, with a hook list:** trusted (with other tools' hooks, lint problems and a prompt hook); trusted with a pet; untrusted, modified and disabled; not registered; plugin; plugin without a hook; plugin and direct; plugin without a source path; plugin missing some events.
  - **`--probe`:** direct, direct with a pet, from the files, and the plugin's (Unix).
- **tests/doctor.rs** replays the committed corpus, and with `AIPET_GOLDEN` a live one:
  - its pet is the test's own: a Unix socket, or a named pipe owned by the current user (SDDL), with the next instance made before the served one is let go;
  - it checks the doctor changes nothing in the sandbox;
  - runs that say nothing of the port (a held file, a slow machine) are set aside and rerun.
- **Scenarios that run only where they may:**
  - `ci`: Windows Codex `--probe`, which starts PowerShell, runs only where `CI` is set;
  - `managed`: only with `AIPET_TEST_MANAGED_SETTINGS=1`;
  - `no_codex`: Windows, only when no Codex is installed in the known folder.
- **src/doctor.rs unit tests:**
  - `Short` and the padding;
  - `Lint`;
  - `GetDirectoryName`;
  - the `Features` and `[[hooks.X]]` regular expressions;
  - a program that outlives its timeout is killed with its tree, and one that can't start gives -2;
  - Windows: the cmd.exe fallback after PowerShell, which a `Machine` seam keeps from starting.
- **tests/common:** registration.rs and codex.rs now share its harness. registration.rs's usage-error check no longer times the hook against 2 s (from the integrated checks); the exit code and the usage text tell a usage error from a hook run.
- **cross-runtime.yml:**
  - RegistrationTests joins the C# classes run against the Rust hook;
  - a new step, "The doctor with managed settings, against the C#", runs on both OSes with `AIPET_TEST_MANAGED_SETTINGS=1`. On Linux it first gives the runner `/etc/claude-code`. It fails if any managed scenario was left out.

### Fix CI found in task 3's code (claude registration, symlink-dangling)
.NET's `File.Exists` is true for a settings.json symlink it can't follow (dangling, or a loop), so reading it throws. The C#'s `--install` and `--uninstall` then fail with "Couldn't update claude settings: Could not find file '…'." and write nothing. The Rust treated the link as absent: `--uninstall` said nothing was registered, and `--install` created the link's target. A loop was replaced by a regular file.

The fix is in install.rs's shared helpers, with the conductor's go-ahead:
- `is_file` answers as `File.Exists` does. That covers codex.rs and the doctor too, which use `File.Exists` in the same places.
- `thrown` says a missing file as .NET does: `Could not find file`, or `Could not find a part of the path`. A folder read as a file is "Access to the path … is denied.".

I checked the other Unix-only fixtures: the symlink chain, the valid symlinks, hooks-json-symlink, and the mode fixtures (the modes are kept, and the backups' modes are copied, as on .NET). Only the dangling and loop cases differed.

New tests:
- install.rs: `File.Exists` (dangling, loop, folder link), the thrown messages, and the `full_path` test restored from claude.rs's deleted copy;
- claude.rs (cfg(unix)): the dangling and loop cases fail and write nothing.

The Unix parts are linted here with a cross-target clippy for x86_64-unknown-linux-gnu (`-Zbuild-std`). CI's Linux job is what runs them, along with the registration live replay that found the gap.

### Gates
- **Rust:** green on the final tree, with a receipt: `cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`.
- **.NET:** `dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build` is green: 160 passed, 42 skipped, as at baseline.
- **C# PluginHooksTests and RegistrationTests with `AIPET_TEST_HOOK` at the Rust hook:** 14 passed, 10 skipped (Unix-only). The first run failed one in-process C# test with "Access to the path is denied" in its own `File.Move`, the antivirus flake. That test doesn't run the hook, and the rerun was green.
- **Doctor live replay** (`AIPET_GOLDEN`): green.
- **Linux cross-clippy:** clean.

### For the conductor
- **Outside the original Touches:** install.rs (the symlink fix, which the conductor asked for) and the run note `NOTES_DIR/task-5-doctor.md`.
- **Follow-up outside the Touches:** `rust/golden/Program.cs` still says the doctor mode isn't written.
- **Only CI confirms:**
  - the Linux live corpus: probes through `/bin/sh`, the plugin's launcher, codex not executable;
  - the managed-settings step;
  - Windows Codex `--probe` through PowerShell, in `CI` runs;
  - the cfg(unix) tests.

Review fix (d41029b): `codex::all_handlers()` can no longer fail. A hooks.json that names a member twice is left out whole, as an invalid one is, and config.toml's hooks are still listed, so the doctor no longer says AiPet isn't registered. Unit test: `codex::tests::the_files_hooks_are_listed_past_a_hooks_json_that_cant_be_read`.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fix d41029b)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 6a70f68fb13048abe1d2c73f9264f11b46800bed, 73f7875619effa07437fd32591319a24300e6460, d41029ba0e141b4a5f27aabc6ff13939fec834f9
- Tests: baseline: green: cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check (pre-edit, suite_rc=0), baseline: dotnet test AiPet.slnx red pre-edit in the fresh worktree only because AiPet.UI was not built (6 ResourceTests/VelopackTests); green after dotnet build AiPet.slnx -p:UseAppHost=false + dotnet test --no-build (160 passed, 42 skipped), cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check (green at 73f7875, suite_rc=0; green receipt 73f78756-unittest), dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build (green at 73f7875: 160 passed, 42 skipped; green receipt 73f78756-dotnet), AIPET_TEST_HOOK=rust/target/debug/aipet-hook.exe dotnet test AiPet.slnx --no-build --filter PluginHooksTests|RegistrationTests: run 1 failed 1 in-process C# test (InstallClaude_WithThePluginEnabled_RegistersNone: UnauthorizedAccessException in Install.Save File.Move, the antivirus flake; the test does not run the hook); rerun green: 14 passed, 10 skipped (Unix-only), cargo test -p aipet-hook --test doctor (committed corpus, 26 Windows scenarios through the Rust hook: green), AIPET_GOLDEN=<abs>/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test -p aipet-hook --test doctor the_csharp_on_this_os_is_replayed (C# regenerated the corpus on this machine, Rust replayed it: green, 168 s), RUSTC_BOOTSTRAP=1 cargo clippy -p aipet-hook --all-targets --target x86_64-unknown-linux-gnu -Zbuild-std --offline -- -D warnings (clean: lints the Unix code and cfg(unix) tests, which only CI runs), dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll doctor (wrote tests/golden/doctor/doctor.json, 26 scenarios on Windows), integrated verify (Windows, work branch 2569d3c with task 5 and its review fix merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped (a run while two workers built had 2 failures and took 1 m 16 s; the rerun was green in 18 s); AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests: 14 passed, 10 Unix-only skipped; AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server: 15 passed; with AIPET_GOLDEN every live replay (doctor, registration, Codex registration) green except aipet-ipc's paths_equal_the_csharps_in_the_same_environment, which task 30 fixes (it reproduces here at 2569d3c and at f3678f4); an earlier live run failed in the C# generator itself ("Access to the path is denied" in its own settings write, a different case each run: the antivirus flake), and the rerun passed
- PRs:
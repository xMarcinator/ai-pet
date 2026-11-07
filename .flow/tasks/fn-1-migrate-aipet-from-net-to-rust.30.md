---
satisfies: [R8, R10]
---
# fn-1-migrate-aipet-from-net-to-rust.30 CI parity on the Windows runner: known folders in the scenario's environment, and no NUL-device fixture

## Description
CI's first runs on GitHub's windows-2022 runner (run 36837285180) failed two parity tests that pass on the developer's
Windows 11 machine. Neither is a bug in the port's production code. Both come from how the tests are set up, and both
show only on Windows Server 2022. Fix the tests and the fixture; production behaviour must not change.

**Size:** S
**Files:** `rust/crates/aipet-ipc/src/{csharp,paths}.rs`, `rust/golden/Sessions.cs`, `rust/crates/aipet-core/tests/sessions.rs`, `rust/crates/aipet-core/tests/golden/sessions/claude.json`
**Touches:** [rust/crates/aipet-ipc/src/csharp.rs, rust/crates/aipet-ipc/src/paths.rs, rust/golden/Sessions.cs, rust/crates/aipet-core/tests/sessions.rs, rust/crates/aipet-core/tests/golden/sessions/**]

### 1. aipet-ipc: `csharp::paths_equal_the_csharps_in_the_same_environment`
The failing scenario sets `LOCALAPPDATA=C:\nowhere` and `USERPROFILE=C:\nowhere`.
- The C# runs as a child process with the scenario's environment. On the runner its `local_app_data` is `C:\nowhere`
  (so `data_dir` is `C:\nowhere\AiPet`), while `home` stays `C:\Users\runneradmin`.
- The Rust side is computed inside the test's own process (`rust_paths(&vars)`). `Folders::of` asks the shell for the
  known folders (`SHGetKnownFolderPath`, `KF_FLAG_DEFAULT`), and the shell answers from the test process's
  environment. Only the fallback reads `vars`. So the Rust gives `C:\Users\runneradmin\AppData\Local`.
- On Windows 11 the known folders don't follow the variables, so the test passes there. On the windows-2022 runner,
  FOLDERID_LocalAppData follows LOCALAPPDATA: either the shell expands it from the process environment, or the call
  fails and .NET falls back to the variable. The production Rust makes the same call with the same fallback, so it
  would give what the C# gives in the same process environment. The test just never puts it in that environment.

Fix: compute the Rust's values in a child process started with exactly the scenario's environment, the way the C#
is run. For example, re-run the test binary (`std::env::current_exe()`) on an `#[ignore]`d helper test, with
`--exact --ignored --nocapture`. The helper prints the production values (`process_vars`, the cached folders, the
data dir, the endpoint) as JSON on a marked line, and does nothing unless a variable set only by the parent says so.
Both sides then see the same environment, whatever the machine's shell does.

Also correct two comments that say the variables never change the known folders: the scenario's comment in
csharp.rs, and the Windows doc comment of `Folders::of` in paths.rs. On the windows-2022 runner, FOLDERID_LocalAppData
follows LOCALAPPDATA.

### 2. aipet-core: `every_claude_case_matches_the_csharp`
Case `claude/titles`, step 99, `claude:title-nul`: the Rust's `chat_title` is None, where the C#'s golden says
`"A\u0000b"`.
- The case's transcript fixture is `{files}/nul.jsonl`: `rust/golden/Sessions.cs:721` names it after the case,
  `name + ".jsonl"`.
- Windows before 11 (Server 2022 included) treats a file named NUL with any extension as the NUL device. The replay's
  write goes nowhere, and the read finds nothing. Windows 11 dropped that rule, and Linux never had it.

Fix:
- Rename the case so its file isn't a DOS device name, e.g. `nul-char` (session `title-nul-char`).
- Regenerate `rust/crates/aipet-core/tests/golden/sessions/claude.json` with the generator's sessions mode
  (`dotnet run --project rust/golden -c Release -- sessions`, or the built dll), which rewrites codex.json too. Never
  edit golden files by hand. The generator reads the real clock, so a rerun moves every step's times. With the times
  masked, the regenerated files must differ only in that case's name, session id and file name.
- Add a guard that fails on any OS, so Linux CI catches this too. The sessions replay (or the generator) rejects a
  fixture whose base name is a reserved DOS device name: CON, PRN, AUX, NUL, COM0–9, LPT0–9 and the superscript-digit
  forms, in any case, with any extension. No other golden file in the repository has such a name today; the conductor
  checked `rust/crates/*/tests/golden/**`.

### Environment (this machine)
- Windows 11 with a corporate antivirus. Never run PowerShell. Use Git Bash; Rust 1.98.1 is at `~/.cargo/bin`; build
  .NET with `-p:UseAppHost=false`.
- Never set `AIPET_TEST_CREDENTIAL_MANAGER`.
- Never read or write the user's real `~/.claude`, `~/.codex`, AiPet data folder or Credential Manager.
- For a few minutes after a rebuild, the antivirus can make file operations fail with "Access to the path is denied".
  Rerun before treating that as a failure.

## Acceptance
- [ ] `paths_equal_the_csharps_in_the_same_environment` computes the Rust's values in a child process with the
      scenario's environment, using the production entry points. It passes here with `AIPET_GOLDEN` set. The handover
      says why it now also holds on a machine whose known folders follow the variables (the windows-2022 runner),
      which only CI can confirm.
- [ ] The helper that prints the Rust's values does nothing in a normal test run.
- [ ] The comments in csharp.rs and paths.rs describe the known folders as they behave on both machines.
- [ ] The title case no longer writes a file with a reserved DOS device name. claude.json and codex.json are
      regenerated by the generator. With the times a rerun always moves masked, the diff touches only that case.
- [ ] A guard fails on every OS for a fixture whose base name is a reserved DOS device name. A unit test covers the
      check itself (NUL, nul.jsonl, COM1.txt and LPT¹ are refused; null.jsonl, nul-char.jsonl and con2 are not).
- [ ] No production code changes behaviour. `cargo test --workspace` (with and without `AIPET_GOLDEN`), clippy with
      `-D warnings` and fmt are green, and so are `dotnet build AiPet.slnx -p:UseAppHost=false` and
      `dotnet test AiPet.slnx --no-build`.


## Done summary
CI's windows-2022 runner failed two parity tests that passed on Windows 11 and Linux. Both are fixed by construction. The paths test, which failed on this machine too before the change, now passes here. No production code changed behaviour (paths.rs changed only in a doc comment).

- aipet-ipc `csharp::paths_equal_the_csharps_in_the_same_environment` now computes the Rust's values in a child process started with the scenario's environment, as it already did for the C#. The child is this test's own binary, re-run on the ignored helper `csharp::print_this_process_paths` with `--exact --ignored --nocapture` and `AIPET_IPC_TEST_PATHS_CHILD=1`, which only the parent sets. The helper prints the production entry points as one JSON line after the marker `aipet-ipc paths: `. Those are `endpoint::endpoint`, `paths::data_dir`, `config`, `log`, `hook_events_log`, `codex_home`, `home` and `local_app_data`, which read `process_vars` and the `OnceLock` caches. Without the variable the helper returns at once, and a normal run lists it as ignored. `in_scenario` applies a scenario's variables to both commands, so the C# and the Rust get the same overrides.
- aipet-core tests/sessions.rs refuses, on every OS, a step whose fixture base name is a DOS device. `is_dos_device_name` covers CON, PRN, AUX, NUL, COM0-9, LPT0-9 and the superscript digits ¹ ² ³, in any case, with any extension from the first `.` or `:`, and with spaces before the extension. The replay checks write steps, delete steps and `{files}/` transcript paths. `fixtures_named_as_dos_devices_are_refused` covers the check. It refuses NUL, nul.jsonl, COM1.txt, LPT¹, Aux.tar.gz, `nul .jsonl` and `{files}/con.jsonl`, and accepts null.jsonl, nul-char.jsonl, con2, COM10, LPT, `a\0b.jsonl` and the empty name.
- rust/golden/Sessions.cs names the title case `nul-char` (session `title-nul-char`, file `nul-char.jsonl`). The generator's sessions mode rewrote claude.json and codex.json. I masked unix seconds, the time prefix of v7 turn ids in every spelling, and the lags computed from now. After masking, codex.json is identical to the committed file. claude.json differs in 3 lines, all of them this case's. The write step has the new file name. The SessionStart step has the new session id, the new transcript path and the log's 13-character id `title-nul-cha`. The case's final snapshot has the id `claude:title-nul-char`.
- The scenario's comment in csharp.rs and the Windows doc of `Folders::of` in paths.rs now describe what the shell does (next section).

What the known folders do on this machine
- I measured this with a scratch .NET probe that calls SHGetKnownFolderPath with KF_FLAG_DEFAULT (and KF_FLAG_DONT_VERIFY, never KF_FLAG_CREATE) in child processes started with chosen environments, plus `reg query`.
- FOLDERID_Profile comes from the profile list. It stayed C:\Users\MJE whatever USERPROFILE said.
- FOLDERID_LocalAppData is the registry's `User Shell Folders\Local AppData`, which is `%USERPROFILE%\AppData\Local` here. The shell expands it with the asking process's USERPROFILE. With USERPROFILE at a folder that has `AppData\Local`, the call returned `<that folder>\AppData\Local`. With USERPROFILE=C:\nowhere it failed with 0x80070003 (KF_FLAG_DONT_VERIFY gave C:\nowhere\AppData\Local), and .NET's GetFolderPath fell back to LOCALAPPDATA, C:\nowhere. LOCALAPPDATA alone changed nothing.
- The coordinator's mechanism is confirmed, with one addition. LocalAppData also follows USERPROFILE whenever `<USERPROFILE>\AppData\Local` exists, and the LOCALAPPDATA fallback covers the case where it is missing.
- The task file's claim that the known folders ignore the variables on Windows 11 is out of date. The paths test failed here at f3678f4 exactly as on the runner. C:\nowhere was never created (checked after every run).

Why the fixes hold on the windows-2022 runner (only CI can confirm)
- The C#'s `Environment.GetFolderPath` (Environment.Win32.cs) and the Rust's `paths::known_folder` make the same call, SHGetKnownFolderPath with flags 0 and no token, and fall back to the same variable when it fails. Both now run in processes started from the test's environment with the same overrides, so the shell gives both the same answer, whatever the machine's registry or Windows version does with the variables. The runner's failure output (the C# gave local_app_data C:\nowhere and home C:\Users\runneradmin) fits the mechanism measured here. I can't run the runner, so its internals are inferred from that output.
- `nul-char.jsonl` is a plain file name on every Windows, so the title case's transcript is a real file on Server 2022. The next CI run on windows-2022 is the only proof that both tests are green there.

Acceptance
- The paths test computes the Rust's values in a child with the scenario's environment through the production entry points, and passes here with AIPET_GOLDEN set (9 scenarios on Windows, 14 on Unix). The helper does nothing in a normal run (listed as ignored; with `--include-ignored` and no variable it prints nothing).
- Both comments describe the known folders as measured, consistent with the runner's output.
- The title case writes `nul-char.jsonl`. Both golden files were regenerated by the generator, and the masked diff touches only that case.
- The guard fails on every OS. At 6b37be9 (guard, old golden) on Windows 11, `every_claude_case_matches_the_csharp` fails with "claude/titles, step 76: the fixture "nul.jsonl" is named as a DOS device". Its unit test covers the cases the AC names.
- No production behaviour changed. At 3a2a31f, `cargo test --workspace` is green with and without AIPET_GOLDEN, clippy `-D warnings` and fmt are clean, and so are `dotnet build AiPet.slnx -p:UseAppHost=false` and `dotnet test AiPet.slnx --no-build` (160 passed, 42 skipped). aipet-ipc also passes clippy `-D warnings` for x86_64-unknown-linux-gnu through `-Zbuild-std`. That checks types and lints only. The Linux tests run in CI.

Choices to review
- The guard also checks `{files}/` transcript paths. A case that reads a missing `con.jsonl` would open the console on Server 2022, the same defect class.
- The guard lives in the Rust replay only, since the AC allows the replay or the generator. The replay runs in CI on both OSes, and the generator doesn't.
- Commits follow the defect route. 6b37be9 carries the failing guard as the reproduction, c45f1c5 renames the case and regenerates the data, and 3a2a31f moves the paths test into the child.

Defect route:
- prior fixes: none. `gh pr list --state open` is empty, `gh issue list` finds nothing for nul, LOCALAPPDATA or windows-2022, no branch has commits on these files past f3678f4, and their history has no reverts. Unchecked: the memory bug track (memory isn't initialized in this repo).
- diagnosis: for paths, eliminated "LOCALAPPDATA moves the known folder" (the probe with LOCALAPPDATA alone kept C:\Users\MJE\AppData\Local) and "the Rust's call differs from .NET's" (same flags and fallback, and the Rust helper in a child with the scenario's environment printed C:\nowhere like the C#). Confirmed that the shell expands `%USERPROFILE%\AppData\Local` with the asking process's USERPROFILE and fails with 0x80070003 when that folder is missing, so the in-process Rust saw the test's own USERPROFILE (probe output, reg query). For nul, confirmed from CI run 36837285180, where only claude/titles step 99 (`claude:title-nul`, chat_title None) differs while the other 63 Claude cases and every Codex case replay on windows-2022, and from Microsoft's naming rules (NUL.txt is NUL). No runtime check on Server 2022 is possible here.
- introduced by: 03b1404 "feat(rust): add the aipet-ipc contract crate and the hook, core and golden skeletons" (the C:\nowhere scenario) and 018b9d9 "feat(core): port AgentSessions' Claude path, checked against the C# golden data" (the nul case). Bisect skipped. There is no known-good revision, because both tests failed on the first windows-2022 run that ran them, and the paths test's earlier local pass came from the machine, not a revision.
- base: the paths test red at f3678f4 with AIPET_GOLDEN (Rust local_app_data C:\Users\MJE\AppData\Local, C# C:\nowhere), and the sessions replay red at 6b37be9 (nul.jsonl refused) | head: both green at 3a2a31f on Windows 11. windows-2022 is not observable from here.
- live: no live surface

Baseline: red only where the evidence file says. The AIPET_GOLDEN run at f3678f4 failed this task's paths test, plus two tests that are machine noise and passed at verify. One is registration's C# generator ("Access to the path is denied", a different case each run). The other is server's busy-CPU pair ("the busy threads never all ran" while sibling worktrees were building). `dotnet test` failed RegistrationTests.InstallClaude_WithThePluginEnabled_RegistersNone with UnauthorizedAccessException in MoveFile, and it passed at verify. Everything else was green.

Follow-ups (outside Touches)
- The board, data, registration and doctor replays have no DOS-device guard. The conductor found no such fixture today.
- tests/doctor.rs `doctor_env` sets USERPROFILE to `{root}/home` and leaves LOCALAPPDATA alone. The hook child's LocalAppData then falls back to the inherited LOCALAPPDATA, which agrees with the test process's `installed_codex()`. A scenario that creates `{root}/home/AppData/Local` would move the child's LocalAppData into the sandbox and break that agreement.

Gate receipts (this workspace): .flow/tmp/green-receipts/3a2a31f1-unittest.json and 3a2a31f1-dotnet.json.
Notes: NOTES_DIR/task-30-ci-parity.md.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

stage: impl-review - ran (codex: SHIP in one round, no findings)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 6b37be9d21031151abd5dff9c3588ac73620ba7b, c45f1c507673a408960f88d44e9fbfa55db17199, 3a2a31f1730be2cf12996c511b3062b886ab60ef
- Tests: baseline: red (cd rust && AIPET_GOLDEN=<abs>/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test --workspace at f3678f4: csharp::paths_equal_the_csharps_in_the_same_environment, this task's target; also registration::the_csharp_on_this_os_is_replayed (Access to the path is denied in the C# generator) and server::*_while_every_cpu_is_busy (the busy threads never all ran, under sibling builds), both machine noise and green at verify; dotnet test AiPet.slnx --no-build: RegistrationTests.InstallClaude_WithThePluginEnabled_RegistersNone, UnauthorizedAccessException in MoveFile, green at verify). Green pre-edit: cargo test --workspace without AIPET_GOLDEN, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --all -- --check, dotnet build AiPet.slnx -p:UseAppHost=false, cd rust && cargo test --workspace --locked --no-fail-fast (AIPET_GOLDEN unset): green at 3a2a31f, cd rust && AIPET_GOLDEN=<abs>/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test --workspace --locked --no-fail-fast: green at 3a2a31f (aipet-ipc lib 20 passed and 1 ignored, incl. csharp::paths_equal_the_csharps_in_the_same_environment; aipet-core sessions 47 passed, incl. fixtures_named_as_dos_devices_are_refused and every_claude_case_matches_the_csharp; the registration, doctor and codex live replays passed); C:/nowhere not created, cd rust && cargo clippy --workspace --all-targets --locked -- -D warnings: clean at 3a2a31f, cd rust && cargo fmt --all -- --check: clean at 3a2a31f, cd rust && RUSTC_BOOTSTRAP=1 cargo clippy --offline --locked -p aipet-ipc --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings: clean (type and lint check only; Linux CI runs it), dotnet build AiPet.slnx -p:UseAppHost=false: 0 warnings, 0 errors; dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped (at 3a2a31f), reproduction: cd rust && cargo test -p aipet-core --test sessions fails at 6b37be9 (claude/titles, step 76: the fixture nul.jsonl is named as a DOS device) and is green at c45f1c5 and 3a2a31f, golden: dotnet build rust/golden -c Release -p:UseAppHost=false && dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll sessions (CODEX_HOME at an empty scratch folder): claude.json 64 cases / 1831 envelopes, codex.json 79 cases / 1889 envelopes; with unix seconds, v7 turn-id time prefixes and now-derived lags masked, codex.json is identical and claude.json differs only in the nul-char case (3 lines), paths child by hand: AIPET_IPC_TEST_PATHS_CHILD=1 LOCALAPPDATA=C:/nowhere USERPROFILE=C:/nowhere (backslashed) <aipet_ipc test exe> csharp::print_this_process_paths --exact --ignored --nocapture printed local_app_data C:/nowhere, data_dir C:/nowhere/AiPet and home C:/Users/MJE (backslashed), as the C# does, gate receipts at 3a2a31f (workspace .flow/tmp/green-receipts): unittest, dotnet, integrated verify (Windows, work branch 952ce64 with task 30 merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped; AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests: 14 passed, 10 Unix-only skipped; AIPET_TEST_HOOK=<.NET hook dll> cargo test -p aipet-core --test server: green; with AIPET_GOLDEN the whole workspace green, aipet-ipc's paths_equal_the_csharps_in_the_same_environment included, except the two registration live replays, whose C# generator failed under the antivirus while two workers built; both passed when rerun one at a time (registration 47 s, codex 72 s), conductor check of the regenerated golden files (f3678f4 against 952ce64, with epoch times, millisecond lags and v7 turn-id time prefixes masked): claude.json differs only in the renamed case's 6 lines (nul to nul-char); codex.json keeps its 79 cases, 2,065 steps and every outcome, and differs only in time-derived values
- PRs:
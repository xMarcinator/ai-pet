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
  (`dotnet run --project rust/golden -c Release -- sessions`, or the built dll). Never edit golden files by hand.
  Check that the regenerated file differs only in that case's name, id and file name.
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
- [ ] The title case no longer writes a file with a reserved DOS device name. claude.json is regenerated by the
      generator, and its diff touches only that case.
- [ ] A guard fails on every OS for a fixture whose base name is a reserved DOS device name. A unit test covers the
      check itself (NUL, nul.jsonl, COM1.txt and LPT¹ are refused; null.jsonl, nul-char.jsonl and con2 are not).
- [ ] No production code changes behaviour. `cargo test --workspace` (with and without `AIPET_GOLDEN`), clippy with
      `-D warnings` and fmt are green, and so are `dotnet build AiPet.slnx -p:UseAppHost=false` and
      `dotnet test AiPet.slnx --no-build`.


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

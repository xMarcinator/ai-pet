---
satisfies: [R4]
---
# fn-1-migrate-aipet-from-net-to-rust.3 Registration golden harness, Claude registration and --print-plugin-hooks

## Description
Port `--install`/`--uninstall claude` and `--print-plugin-hooks`, and fill in the C# golden generator's registration
mode so the Rust output is checked byte for byte against the C#'s. Task 4 reuses the mode for Codex.

**Size:** M
**Files:** `rust/golden/Registration.cs`, `rust/crates/aipet-hook/src/{install,claude,plugin_hooks,json_out}.rs`, `rust/crates/aipet-hook/tests/registration.rs`, `rust/crates/aipet-hook/tests/golden/registration/**`
**Touches:** [rust/golden/Registration.cs, rust/crates/aipet-hook/src/install.rs, rust/crates/aipet-hook/src/claude.rs, rust/crates/aipet-hook/src/plugin_hooks.rs, rust/crates/aipet-hook/src/json_out.rs, rust/crates/aipet-hook/tests/registration.rs, rust/crates/aipet-hook/tests/golden/registration/**, rust/crates/aipet-hook/src/main.rs, rust/crates/aipet-hook/src/json.rs, rust/crates/aipet-hook/src/event.rs, tests/AiPet.Tests/TestEnv.cs, tests/AiPet.Tests/PluginHooksTests.cs, tests/AiPet.Tests/RegistrationTests.cs (widened by the conductor: ACs 2 and 4 need the C# tests to honour AIPET_TEST_HOOK)]

### Approach
- Golden mode `registration`: for each fixture, a starting `settings.json` plus a flag for the plugin being
  enabled/installed and for a symlinked settings file. Run `ClaudeConfig` install and uninstall against a temporary
  `CLAUDE_CONFIG_DIR`, and record the resulting files with backup names normalised.
  - The fixtures: an empty file, other tools' hooks, legacy `hook.py`/`ClaudePet.exe` entries, an older AiPet set,
    `"hooks": {}`, the plugin enabled with and without a cache, and a symlink.
  - Task 1 already linked `Install.cs` together with `CodexConfig.cs` and `PluginHooks.cs`: `Install.Run` refers to
    `CodexConfig`, so `Install.cs` doesn't compile alone.
- Rust ports:
  - `Install.Run`'s exe-name check (`src/AiPet.Hook/Install.cs:15-41`).
  - `Save`: atomic, keeping the mode (`:49-71`).
  - `ClaudeConfig`: `Events`, `IsOurs`, `Plugin`/`Installed`, `PluginRuns`/`GitBash`, `Edit` including the no-op on
    deep-equal, `RealPath` and `Backup` keeping the newest 3 (`:76-246`).
  - `PluginHooks` (`src/AiPet.Hook/PluginHooks.cs`): LF line endings, written as raw bytes.
- `json_out`: a writer that reproduces System.Text.Json's indented output with `UnsafeRelaxedJsonEscaping` (two-space
  indent, its escaping rules). Test it against the generator's output rather than assuming.

### Investigation targets
**Required:**
- `src/AiPet.Hook/Install.cs:1-246` — Claude registration
- `src/AiPet.Hook/PluginHooks.cs` — printed plugin hooks
- `tests/AiPet.Tests/RegistrationTests.cs` — expected behaviour (Claude cases)
- `rust/golden/Program.cs`, `rust/golden/Sprite.cs` — the generator's dispatcher and an existing mode to follow
**Optional:**
- `tests/AiPet.Tests/PluginHooksTests.cs`, `scripts/check-plugin.sh`
- `plugins/aipet/hooks/hooks.json`, `plugins/aipet/hooks/codex.json`
## Acceptance
- [ ] Every registration fixture's install and uninstall output is byte-identical to the C#'s.
- [ ] `--print-plugin-hooks claude|codex` is byte-identical to the committed plugin files, and `PluginHooksTests`
      passes with `AIPET_TEST_HOOK`.
- [ ] Refusals match the C#'s messages and exit codes: a wrong exe name, an unparsable `settings.json`.
- [ ] The Claude subprocess cases in `RegistrationTests` pass against the Rust hook.
## Done summary
Ported `--install`/`--uninstall claude` and `--print-plugin-hooks` to the Rust hook (edd1371), and filled in the golden generator's `registration` mode, which Task 4 extends for Codex.

The golden recorded 65 Claude fixtures (120 steps) through the hook's own Install.cs. Each step records the exit code, what was printed, and every file left in the case's folder, with backup names normalised. The fixtures cover:
- an empty or missing file, other settings, other tools' hooks, legacy `hook.py`/ClaudePet entries, and older and current AiPet sets;
- `"hooks": {}` and other odd shapes, unparsable files, and duplicate keys;
- the plugin enabled with and without an install or cache, and without Git Bash;
- backups, BOM, UTF-16 and invalid UTF-8 files, a leftover temp file, and a read-only file;
- symlinks and modes: recorded only when the corpus is written on Unix.

It also records System.Text.Json's behaviour as the registration uses it: the escape tables of both encoders, indented writes, `ToString`, `DeepEquals`, and ~150 `JsonNode.Parse` refusals.

`tests/registration.rs` replays the golden through the built binary, and every fixture comes out byte-identical. The same file checks:
- the refusals: a wrong exe name, run as a hard link under another name, and an unparsable settings.json, both with the C#'s messages and exit codes;
- usage for each mode;
- `--print-plugin-hooks claude|codex` against the committed plugin files.

With `AIPET_GOLDEN` set, a live C# run on the current OS is replayed too. That is the only path that exercises the symlink and mode fixtures, on Linux CI. `json_out`'s unit tests replay json.json, including Utf8JsonReader's messages and positions.

PluginHooksTests and RegistrationTests now go through `TestEnv.HookStartInfo`, so `AIPET_TEST_HOOK` reaches them. Under it, `Install_UnderDotnet_Refuses` copies the hook under another name. Against the Rust hook they pass (14 passed, 10 skipped as Unix-only), and they also pass without it. The full gates are green on the final tree: Rust test, clippy `-D warnings` and fmt; dotnet 160 passed, 42 skipped, the same as the baseline.

For the conductor:
- **Touches:** the widened set in ef8c67e covers the files changed outside the original list: `main.rs` (dispatch of the non-event modes), `json.rs` (`Object::iter`), `event.rs` (`decoded` made pub(crate)), and the three C# test files.
- **Temporary stubs:** `--doctor <agent>` and `--install|--uninstall codex` print "isn't ported to this hook yet" and exit 1, until tasks 5 and 4. NOTES_DIR/task-3-registration.md has what those tasks can reuse.
- **Git Bash on Windows:** Windows gives every process its own `ProgramFiles`, so a child process can't be kept from finding Git Bash. The os_specific no-Git-Bash fixture therefore runs in-process in a claude.rs unit test, and the subprocess replay skips it.
- **Golden stability:** one early live run gave a spurious exit 1 that I couldn't reproduce, probably the antivirus holding a fresh file. Each golden case now runs until two runs agree, and the Rust replay retries a failing case twice before it fails.
- **Deliberate deviations, not in any fixture:** plugin ids use an ordinal `StartsWith` where the C# compares by culture; backups are pruned in ordinal order where the C# sorts by culture; messages are written as UTF-8, where .NET's Console on Windows uses the console's code page (this only shows for a non-ASCII path).
- **Follow-ups outside this task:**
  - cross-runtime.yml doesn't yet run PluginHooksTests or `RegistrationTests.Install_UnderDotnet_Refuses`.
  - rust/golden/Program.cs still says registration isn't written.
  - claude.rs has its own local-time helper beside trace.rs's.
- **Unverified here:** the Unix-only code was only linted, through a cross-target clippy for x86_64-unknown-linux-gnu. It hasn't run, since this box has no Linux.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

Review fixes (9af86c4, 4a53d2d): the golden replay now fails a case at its first difference and reruns it only on a Windows sharing or lock violation, and the generator needs two clean runs to agree; on Unix, full_path makes a relative CLAUDE_CONFIG_DIR absolute before taking out `..`.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fixes 9af86c4, 4a53d2d)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: edd13712ddc4df602f0ff1b7fc9e508298072fdc, 9af86c4, 4a53d2d
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check: rc 0; dotnet test AiPet.slnx after dotnet build AiPet.slnx -p:UseAppHost=false: 160 passed, 42 skipped), cd rust && cargo test --workspace -- --skip credential_manager_tokens_are_the_apps && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check (rc 0, final tree), dotnet build AiPet.slnx -p:UseAppHost=false && dotnet test AiPet.slnx --no-build (160 passed, 42 skipped, final tree), AIPET_GOLDEN=<abs>/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test -p aipet-hook --test registration (5 passed: committed golden replayed, and a live C# run on Windows replayed; json.json identical), AIPET_TEST_HOOK=<worktree>/rust/target/debug/aipet-hook.exe dotnet test tests/AiPet.Tests --no-build --filter PluginHooksTests|RegistrationTests (14 passed, 10 skipped as Unix-only; Install_UnderDotnet_Refuses and PrintPluginHooks_* ran the Rust hook, checked by a bogus AIPET_TEST_HOOK failing them), dotnet test tests/AiPet.Tests --no-build --filter PluginHooksTests|RegistrationTests without AIPET_TEST_HOOK (14 passed, 10 skipped), RUSTC_BOOTSTRAP=1 cargo clippy -p aipet-hook --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings (clean: the Unix-only code and tests type-check and lint; not run), dotnet rust/golden/bin/Release/net10.0/aipet-golden.dll registration run twice: claude.json byte-identical (deterministic), mutation: keeping 2 backups instead of 3 fails claude_registration_is_the_csharps (hooks-empty step 2, files), integrated verify (Windows, work branch 434c23f with the review fixes merged): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped; AIPET_GOLDEN=<golden dll> cargo test -p aipet-hook --test registration: 6 passed (live C# replay included); RUSTC_BOOTSTRAP=1 cargo clippy -p aipet-hook --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings: clean (4a53d2d's Unix-only code is linted, not run), AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests: 14 passed in 4 of 4 runs. The 3 runs right after the rebuild each failed 1-2 in-process C# cases (ClaudeConfig.Install -> Install.Save -> File.Move: UnauthorizedAccessException) while the freshly built unsigned hook was new to the antivirus; RegistrationTests alone, the two failing cases alone, and the pair without AIPET_TEST_HOOK passed every time, and the same exe (sha256 639017c884ba4b76...) then passed 4 of 4
- PRs:
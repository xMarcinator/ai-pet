---
satisfies: [R4]
---
# fn-1-migrate-aipet-from-net-to-rust.4 Codex registration: config.toml text editor, hooks.json and trust preservation

## Description
Port `--install`/`--uninstall codex`: the hand-written, line-based `config.toml` editor and the `hooks.json` path. It
must stay byte-identical to the C#, so Codex's trust keys, and other tools', survive the upgrade. This is the riskiest
file to port, so it has a task of its own.

**Size:** M
**Files:** `rust/crates/aipet-hook/src/{codex,toml_text}.rs`, `rust/golden/Registration.cs` (Codex fixtures), `rust/crates/aipet-hook/tests/golden/codex/**`, `rust/crates/aipet-hook/tests/codex.rs`
**Touches:** [rust/crates/aipet-hook/src/codex.rs, rust/crates/aipet-hook/src/install.rs, rust/crates/aipet-hook/src/toml_text.rs, rust/crates/aipet-hook/tests/codex.rs, rust/crates/aipet-hook/tests/golden/codex/**, rust/golden/Registration.cs]

### Approach
- Port `CodexConfig` (`src/AiPet.Hook/CodexConfig.cs`):
  - `Events`, which depends on the OS, and `PluginEvents` (`:32-42`); `Snake`, `IsOurs` and `BuildCommand` (`:44-69`,
    8.3 paths through `GetShortPathNameW`).
  - The parser: `ParseToml`, `ScanLine`, `HeaderKey`, `StringValue`, `Unescape` (`:199-370`).
  - `InlineHooks` refusal (`:436-449`); `EditToml` with its trust-state pruning by `Normal` and `WarnIfShifting`
    (`:453-594`).
  - `EditToml`'s 3 attempts with an mtime check, as the C#. If the file changed under it, re-read and re-edit.
  - Intentional correction (R4): just before the atomic replace, re-check the file's identity and mtime. If it changed
    on the last attempt, write nothing, print the spec's message and exit 1. The C# overwrites here.
  - The `hooks.json` path (`:653-717`) and `Plugin()` detection (`:161-181`).
- Golden fixtures for `config.toml`:
  - comments, other tools' `[[hooks.X]]` and trust `[hooks.state.'…']` entries;
  - multi-line basic and literal strings, CRLF line endings, a missing final newline;
  - an old AiPet block, and a block whose definitions changed;
  - inline `hooks = {…}` and `[hooks.Stop]` (refused);
  - a plugin enabled or disabled through an inline table.
- Run the Windows fixtures (PowerShell command forms, paths with spaces) on the Windows CI.

### Investigation targets
**Required:**
- `src/AiPet.Hook/CodexConfig.cs:1-718` — the whole editor
- `tests/AiPet.Tests/RegistrationTests.cs` — Codex cases
- `docs/ARCHITECTURE.md` §7.1–7.3 — trust and why the file is edited as text
**Optional:**
- `docs/HANDOFF.md` Decisions — the frozen Codex definitions

### Key context
- Never re-serialise `config.toml`. A format-preserving crate may be used to read it, but the output must come from the
  line-based algorithm.
**From task 3 (2026-09-29):** `install::run` (`install.rs`) answers `codex` with "registering with Codex isn't ported
to this hook yet" and exit 1; replace that arm with `codex::install` / `codex::uninstall`. The exe-name check already
runs before the agent split. Reuse `install::{save, read_text, …}`, `json_out` (JsonNode.Parse with .NET's messages,
the indented writer) and `plugin_hooks::CODEX_EVENTS` instead of second copies, and add the Codex fixtures to
`rust/golden/Registration.cs` beside `ClaudeCorpus` with its `Step`, `Settled` and `Tree` helpers. Since task 3's review
the harness is strict: the Rust replay fails a case at its first difference and reruns it only when the hook or the
harness hit a Windows sharing or lock violation (`a_held_file_is_known_from_what_the_hook_says`), and the generator
needs two clean runs to agree, stopping when they differ. Keep the Codex cases under the same rules.
## Acceptance
- [ ] Every Codex fixture's install and uninstall output (`config.toml`, `hooks.json`, and stderr warnings) is
      byte-identical to the C#'s, on Linux and Windows.
- [ ] Trust entries are dropped only for definitions that changed. Reinstalling at the same path changes nothing.
- [ ] Inline hooks are refused with the C#'s message and nothing is written.
- [ ] A test changes the file before every attempt: the editor re-reads twice, then writes nothing, prints the message
      and exits 1.
- [ ] A test changes it once: the second attempt succeeds, with the same bytes as the C#'s uncontended edit.
- [ ] The Codex subprocess cases in `RegistrationTests` pass against the Rust hook.
## Done summary
Ported `--install`/`--uninstall codex` to the Rust hook (8d020e6). The C#'s line-based config.toml editor is in `toml_text.rs`. `codex.rs` holds the rest: trust pruning by definition, WarnIfShifting, the inline-hooks refusal, plugin detection, the hooks.json path, 8.3 and PowerShell command forms, and the R4 correction. Every Codex fixture comes out byte-identical to the C#'s.

The golden generator now writes `tests/golden/codex/codex.json`: 70 fixtures (128 steps) run through the hook's own CodexConfig.cs. The fixtures cover:
- comments and other settings;
- other tools' hooks and their `[hooks.state.'…']` trust entries, in literal and basic-string keys;
- multi-line basic and literal strings, CRLF, a missing final newline, a BOM, and invalid UTF-8;
- the current block (reinstall changes nothing), an older set (trust is dropped only where the definition changed), an older path, and the legacy `AIPET-~1.EXE` and plugin commands;
- a hooks header AiPet can't read, and the inline, dotted and `[hooks.Stop]` refusals;
- the hooks.json path: other tools', only AiPet's (the file is deleted), mixed, invalid, non-object, and duplicate names (exit 1, the C#'s message);
- the plugin enabled, disabled, dotted and inline, without a cache, and from another marketplace;
- backups, a leftover temp file, and a read-only file (Windows);
- modes and symlinks (Unix, replayed only by CI's live run).

The corpus also records the C#'s own answers for BuildCommand, IsOurs, Snake, OrdinalIgnoreCase, and the `\d`/`\b` classes of .NET's regex.

How it's checked:
- `tests/codex.rs` replays the corpus through the built hook, with the same strict held-file rule as task 3. With `AIPET_GOLDEN` set it replays a live C# corpus too.
- codex.rs's unit tests check the helpers against the golden, and `build_command` in real folders: 8.3 short names where the volume makes them, and `& '…''…'`.
- `an_edit_reads_again_when_the_file_changed_once`: the file is changed once, the edit reads it twice, and the result is the same bytes as the uncontended edit and as the golden's.
- `an_edit_that_keeps_changing_writes_nothing`: the file changes before every attempt, by replacement and by in-place rewrite. There are 3 reads, then an error saying config.toml kept changing and to close Codex and retry. Nothing is written, no backup and no temp file. install::run prints the error as "Couldn't update codex settings: …" with exit 1.
- Both contention tests were confirmed red with the change check disabled.
- RegistrationTests run against the Rust hook: 14 passed, 10 skipped as Unix-only. The Codex doctor cases are task 5's.

Deliberate choices, for the conductor:
- **Attempts:** 3 reads, per the spec and the ACs. The C# reads up to 4 times (`attempt < 3`) and then writes anyway.
- **When the check runs:** the file's identity and last-write time are checked right before the backup and the atomic replace. A failed attempt therefore leaves no backup either.
- **`install::upper` changed:** it is now .NET's ordinal casing. It keeps `ı` and `ſ` and maps `ᾳ` to `ᾼ`, as a .NET probe and the golden's "ordinal" section show. This also affects claude.rs's `is_ours` and `is_hook`, for those characters only; the Claude golden still replays.
- **Culture:** the corpus is written in the invariant culture, as the shipped hook (InvariantGlobalization) runs. Under en-US, .NET's `IgnoreCase` would also let `I` match `İ`.
- **Shared helpers:** `backup`, `real_path`, `full_path` and `local_stamp` are now pub(crate) in install.rs. claude.rs keeps private copies because it is outside the Touches. Follow-up: delete them and use install's.

Follow-ups outside the Touches:
- main.rs's module doc and rust/golden/Program.cs still say Codex registration isn't written.
- tests/codex.rs repeats registration.rs's harness helpers; a tests/common module could share them.
- The Claude corpus' raw-string fixtures take the checkout's line endings. Regenerating claude.json in a CRLF checkout changes them, so don't recommit it from one. codex.json normalises to LF.
- Not verified here: the Linux run. The Unix-only code passed a cross-target clippy for x86_64-unknown-linux-gnu with `-D warnings`, but only CI's live golden runs it.

NOTES_DIR/task-4-codex-registration.md has what task 5 can reuse (EVENTS, snake, is_ours, build_command, toml_text::parse, json_positions) and these traps.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

Review fixes: eed8209 writes the edit and its backup next to config.toml first, then checks the file's identity and last-write time, and only then renames (a failed check removes both; a failed replace keeps the backup, as the C# does); 0a714c0 makes upper() the hook's InvariantGlobalization casing, with Registration.cs computing the ordinal answers under DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1 and a test over every code point.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fixes eed8209, 0a714c0)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 8d020e65d79bc4fc778bc508f933d8bf6312b80a, eed8209, 0a714c0
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check; dotnet test AiPet.slnx -p:UseAppHost=false: 160 passed, 42 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check, dotnet test AiPet.slnx -p:UseAppHost=false (160 passed, 42 skipped, as the baseline), cargo clippy --workspace --all-targets -- -D warnings, AIPET_GOLDEN=<abs>/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test -p aipet-hook --test codex --test registration (live C# corpus on Windows replayed), AIPET_TEST_HOOK=<abs>/rust/target/debug/aipet-hook.exe dotnet test tests/AiPet.Tests -p:UseAppHost=false --filter RegistrationTests|PluginHooksTests (14 passed, 10 skipped as Unix-only), RUSTC_BOOTSTRAP=1 cargo clippy -p aipet-hook --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu -- -D warnings (Unix-only code linted; not run: no Linux here), integrated verify (Windows, work branch 53b3625 with the review fixes merged): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped; AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests: 14 passed, 10 Unix-only skipped; AIPET_GOLDEN live replay: tests/codex.rs 2 passed, AIPET_GOLDEN live replay of tests/registration.rs: 6 passed on the third run; the first two failed in the C# generator's own live run, which recorded 'Access to the path is denied' from its File.Move in a different case each time (hooks-json-no-final-newline, then ours-not-last) while the Rust side matched every case the C# completed: the known antivirus flake on this machine (the committed golden replays passed every time)
- PRs:
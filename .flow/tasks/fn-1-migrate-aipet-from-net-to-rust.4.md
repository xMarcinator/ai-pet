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
**Touches:** [rust/crates/aipet-hook/src/codex.rs, rust/crates/aipet-hook/src/toml_text.rs, rust/crates/aipet-hook/tests/codex.rs, rust/crates/aipet-hook/tests/golden/codex/**, rust/golden/Registration.cs]

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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

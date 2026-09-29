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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

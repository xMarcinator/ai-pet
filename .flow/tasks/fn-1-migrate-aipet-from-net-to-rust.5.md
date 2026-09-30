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
## Acceptance
- [ ] Doctor output and exit codes match the C#'s for every scenario, on Linux and Windows.
- [ ] `--probe` finds the probe event through a live pet, and reports its absence when there is no pet.
- [ ] A missing or failing `codex app-server` falls back to reading files. Timeouts are reported, never a hang.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

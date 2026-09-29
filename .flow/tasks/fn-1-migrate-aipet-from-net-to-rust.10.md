---
satisfies: [R10]
---
# fn-1-migrate-aipet-from-net-to-rust.10 Secrets, config, logs, uninstall cleanup and update status with .NET compatibility

## Description
Port the core's data layer so the Rust pet reads and writes what the .NET app does:
- secrets in the C#'s exact schema;
- `config.json` and its companions;
- `aipet.log`;
- the uninstall hook cleanup;
- the update status texts.

**Size:** M
**Files:** `rust/crates/aipet-core/src/{secrets/{mod,windows,linux,file}.rs,config.rs,log.rs,cleanup.rs,update_status.rs}`, `rust/golden/Data.cs`, `rust/crates/aipet-core/tests/data.rs`, `rust/crates/aipet-core/tests/golden/data/**`
**Touches:** [rust/crates/aipet-core/src/secrets/**, rust/crates/aipet-core/src/config.rs, rust/crates/aipet-core/src/log.rs, rust/crates/aipet-core/src/cleanup.rs, rust/crates/aipet-core/src/update_status.rs, rust/crates/aipet-core/tests/data.rs, rust/crates/aipet-core/tests/golden/data/**, rust/golden/Data.cs]

### Approach
- Secrets:
  - Windows: `CredReadW`/`CredWriteW`/`CredDeleteW`, generic, with target name = the key
    (`src/AiPet.UI/Platform/WindowsPlatform.cs:179-226`).
  - Linux: the C#'s `SecretTool` (`src/AiPet.UI/Platform/LinuxPlatform.cs:270-330`): `secret-tool` with the attributes
    `service=aipet` and `key=<key>`, and the same 0600 `secrets.json` fallback and its triggers.
  - macOS: the in-memory stub, as the C#.
- `config.json` (`src/AiPet.UI/MainWindow.axaml.cs:42-56, 92, 194-201`), with C# parity as the oracle:
  - A serde struct with PascalCase names. Unknown fields are ignored on read and not written back, as the C# does (it
    has no extension data). The legacy `Toolbar` is read, and omitted when null.
  - Compact on write, as the C# writes it. `jira.json`/`github.json` indented, as their C# writers do.
  - The loader reports whether the file was read successfully (`Loaded`, `Missing` or `Corrupt`). Task 16 uses that to
    avoid writing until something really changes.
- `log.rs`: `Log.Write` with rotation at 256 KB (`src/AiPet.Core/Platform.cs`).
- `cleanup.rs`: `HookCleanup.Names`/`Agents`/`Run` (`src/AiPet.Core/HookCleanup.cs`), which run `aipet-hook --uninstall`.
- `update_status.rs`: the texts from `src/AiPet.Core/UpdateStatus.cs`.
- Golden mode `data`:
  - Config fixtures with only known fields: the C#'s parse-then-write output. Rust must match byte for byte.
  - The other direction: the generator also parses what the Rust tests write with the C# `Config` type, which proves
    that the .NET release reads it (the rollback path).
  - The `HookCleanup` matching cases from `tests/AiPet.Tests/VelopackTests.cs`.
  - On the Windows CI the generator writes a credential with the C# code, and a Rust test reads it back.

### Investigation targets
**Required:**
- `src/AiPet.UI/Platform/LinuxPlatform.cs:270-330`, `src/AiPet.UI/Platform/WindowsPlatform.cs:179-226`
- `src/AiPet.UI/MainWindow.axaml.cs:42-56, 160-201` — Config
- `src/AiPet.Core/HookCleanup.cs`, `tests/AiPet.Tests/VelopackTests.cs`
**Optional:**
- `src/AiPet.Core/Platform.cs` — `ISecretStore`, `Log`

### Key context
- The keyring crate's defaults differ from the C#: on Windows its target is `{user}.{service}`, and its Secret Service
  attributes differ. Its defaults would silently lose every saved token. Reproduce the C#'s schema exactly.
## Acceptance
- [ ] A token saved by the C# is read by Rust, and the other way round: on the Windows CI, and by a documented manual
      check with `secret-tool` on Linux.
- [ ] `config.json` with known fields round-trips byte for byte against the golden cases, including the legacy
      `Toolbar`. The C# reads every file the Rust writes.
- [ ] A missing or corrupt config gives defaults with the right load status. The loader never writes.
- [ ] The `HookCleanup` cases match the C#'s. The log rotates at 256 KB.
## Done summary
Ported the core's data layer to aipet-core: secrets in the C#'s schema (Credential Manager target = key; secret-tool service=aipet key=<key> with the 0600 secrets.json fallback; memory on macOS), config.json/jira.json/github.json with System.Text.Json's read/write rules and a Loaded/Missing/Corrupt load status (the loader never writes), aipet.log with the 256 KB rotation, HookCleanup Names/Agents/Run, and the update status texts. Golden mode `data` (rust/golden/Data.cs) records the app's own reads and writes; tests/data.rs replays them byte for byte and, with AIPET_GOLDEN, has the C# read every file the Rust writes and round-trip a test Credential Manager target with the Rust.

Acceptance:
- Tokens both ways: Windows `credential_manager_tokens_are_the_apps` (tests/data.rs; C# half needs AIPET_GOLDEN, as CI sets it; ran green locally). Linux: the manual check is the ignored test `secret_service_round_trip_with_the_csharps_commands`, documented in secrets/linux.rs; stand-in secret-tool tests pin the C#'s exact argv, stdin, TrimEnd and fallbacks.
- config.json byte-for-byte round trip incl. legacy Toolbar: 74 golden file cases (config, jira, github); `the_app_reads_what_the_rust_writes` has the C# read every Rust-written file.
- Missing/corrupt give defaults with the right status, and the load leaves the file untouched and creates nothing (asserted per case).
- HookCleanup: VelopackTests' Names cases plus extra ones, Agents on configs the hook's own registration code wrote, Run tests (Unix stand-in script and hang; Windows with whoami.exe). Log rotation test at exactly 256 KB and 256 KB + 1.

Choices to review:
- C# nulls are `Option` (Avatar, the jira/github strings, the GitHub lists and their items), so null round-trips.
- .NET 10 reads `WindowHeight: 1e400` as infinity and then can't write it; Rust matches (save_to errs, writes nothing).
- `SecretStore::write` returns io::Result. The C# ignores a failed CredWrite, while the Linux file fallback throws; the caller decides.
- `cleanup::names` works on UTF-16 like the C#. Past ASCII, "letter or digit" is Rust's is_alphanumeric (documented); the two differ only for marks and non-decimal numbers.
- aipet.log always uses ':' as the time separator (the C# uses the culture's).
- The golden copies the app's private Config and CredentialManager into Data.cs. Every run checks both, and the config.json read/write lines, against src/AiPet.UI's source. Golden.csproj is unchanged.

Follow-ups (outside Touches): rust/golden/Program.cs still says only sprite and ipc are written. On a fresh worktree `dotnet test AiPet.slnx` needs `dotnet build AiPet.slnx` first (inherited). Linux-only code is cross-target clippy-checked (build-std) but its tests run only in Linux CI. Notes: NOTES_DIR/task-10-data-layer.md.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fix d8c80ae)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 5ad6b8ad5210f17d5a4bb11881fb165588fee4c2, d8c80ae
- Tests: cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check, dotnet test AiPet.slnx, AIPET_GOLDEN=<repo>/rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test -p aipet-core, dotnet run --project rust/golden -c Release -- data (regenerated: no diff), RUSTC_BOOTSTRAP=1 cargo clippy --all-targets -Zbuild-std --target x86_64-unknown-linux-gnu|aarch64-apple-darwin -- -D warnings (aipet-core via scratch manifest), baseline: cargo green; dotnet test AiPet.slnx red pre-edit on the fresh worktree (6 tests need AiPet.UI built; green after dotnet build AiPet.slnx), integrated verify (Windows, work branch 952d0c3): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green (that test writes a test entry into the real Credential Manager; it first runs in Windows CI), clippy --workspace --all-targets -D warnings and fmt --check clean, rust/golden builds (0 warnings), not run anywhere yet: the Linux-only tests (stand-in secret-tool, 0600 file, Unix Run tests, a_locale_names_its_culture) - first in Linux CI
- PRs:
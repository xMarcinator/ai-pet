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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

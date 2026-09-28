---
satisfies: [R16]
---
# fn-1-migrate-aipet-from-net-to-rust.24 Test migration and the parity ledger

## Description
Finish moving the tests. Port the remaining C# white-box tests to Rust, and rehost the black-box script tests in a Rust
sandbox harness. Keep a ledger that maps every C# test to its Rust equivalent, checked in CI.

**Size:** M
**Files:** `rust/crates/aipet-testkit/{Cargo.toml,src/lib.rs}` (sandbox harness), `rust/crates/*/tests/**`, `rust/PARITY.md`, `scripts/check-parity.sh`, `.github/workflows/ci.yml`
**Touches:** [rust/crates/aipet-testkit/**, rust/crates/*/tests/**, rust/PARITY.md, scripts/check-parity.sh, .github/workflows/ci.yml, rust/Cargo.toml, rust/Cargo.lock]

### Approach
- `aipet-testkit`: the equivalent of `TestEnv` and `Scripts.Sandbox` (`tests/AiPet.Tests/Scripts.cs:74-147`).
  - A fresh HOME, XDG folders, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `AIPET_DATA_DIR`, `TMPDIR` and pipe name.
  - `PATH` limited to an allow-list of tools plus stubs.
  - Skip attributes by platform, reported as skipped.
- Port what isn't ported yet: `RegistrationTests`' in-process cases, `VelopackTests`, `AvatarTests` and
  `ResourceTests` (reading the Windows PE version).
- Rehost the script tests: `InstallerTests`, `LauncherTests`, `ReleaseWorkflowTests` and `PluginHooksTests`.
- `rust/PARITY.md` lists all 141 C# test attributes. Each maps to Rust test names, to a black-box run, or to a reason
  it no longer applies. `scripts/check-parity.sh` fails CI when an attribute in `tests/` is unmapped.

### Investigation targets
**Required:**
- `tests/AiPet.Tests/Scripts.cs`, `tests/AiPet.Tests/TestEnv.cs`
- `tests/AiPet.Tests/*.cs` — the inventory
**Optional:**
- `rust/crates/aipet-sprite/tests/golden.rs` — golden replay style
## Acceptance
- [ ] Every entry in `PARITY.md` is mapped, and the check script passes in CI.
- [ ] No Rust test reads or writes the real home, config or data (the sandbox is asserted in the testkit's own tests).
- [ ] Rust test counts per area are at least the C#'s case counts for that area.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

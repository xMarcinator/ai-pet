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
**From task 11 (2026-09-30):** the watchers' expected requests and error texts in `aipet-core/tests/watchers.rs` were
recorded once from .NET 10 (the C#'s request code against a local stub, in a scratch app), not by `rust/golden`, so CI
doesn't replay them against the C#. Record that in the ledger, or add a golden mode before the cutover removes the C#.
The deliberate deviations are listed in the module docs of `jira.rs`, `github.rs` and `http.rs`.
**From task 9 (2026-09-30):** a .NET 10 probe of the C#'s HookServer found events the C# applies but the port's
`Envelope::parse` (sessions, tasks 7 and 8) refuses, so the Rust server gives them no answer: a number past a double's
range (`1e400`), a lone-surrogate escape (the C# applies it, or answers `error:InvalidOperationException` when Apply
reads that string), and nesting exactly 128 deep (serde_json stops at 127; System.Text.Json allows 128). A payload
with a member twice gets `error:ArgumentException` from the C#, while the port applies it with the last value. No
hook sends any of these (the hooks mend lone surrogates, nest at most 65 deep, and send `{}` for a repeated member).
Record them as deviations in the ledger, or propose a follow-up task that backs `Envelope` with raw JSON values;
the sessions code is outside this task's Touches.
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

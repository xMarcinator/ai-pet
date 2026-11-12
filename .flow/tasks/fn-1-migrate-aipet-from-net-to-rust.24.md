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
The C# tests are mapped and the missing ones ported. `aipet-testkit` is the C#'s TestEnv and Scripts.Sandbox: fresh HOME, XDG, agent, data and temp folders, an endpoint no pet listens on, and a PATH of allowed tools plus stand-ins, asserted by its own tests (tests/sandbox.rs: only the sandbox's variables reach a program, PATH is the allow-list and stand-ins, files land in the sandbox and go with it).

- Rehosted in the sandbox against the Rust hook: InstallerTests, LauncherTests, ReleaseWorkflowTests, check-plugin.sh's half of PluginHooksTests.
- Ported: RegistrationTests' in-process cases (black-box against the built hook), VelopackTests' release.yml/main-order checks, ResourceTests (PE reader; Windows-only), HookContractTests (fake sockets in the testkit, real pet in aipet-core's server.rs), NoPetTests' stale socket, AvatarTests, and three SingleInstanceTests cases (test code in aipet/src/single.rs).
- rust/PARITY.md: all 162 attributes (263 cases; the task said 141) mapped, 0 n/a; 413 Rust cases. Records tasks 9 and 11's deviations, the Windows-only tests, and what the cutover must change.
- scripts/check-parity.sh checks rows, case counts, that each Rust fn exists, and Rust ≥ C# per row and per file; ci.yml's Shell scripts job runs it. With tests/ gone it checks the ledger against the Rust alone.

Verified: cargo test --workspace (648 passed, 10 ignored) three times, clippy -D warnings, fmt --check, check-parity.sh against tests/. Windows-only tests unverified here.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: c763811c7ef6a5a542cc661a8285883e229af899, dd7d5fd6f3bc541e464e06c4fa1efb83afa4cb48, 88abd8b8bbedc0ed23f95d9f222cc9549f1ef7aa, d9936d23a0437fb1c3c4523ea536a78023cdd9d0, 1565d0742faa2c70790d6f12b7b59ae98c9c5c57
- Tests: cd rust && cargo test --offline --workspace
- PRs:
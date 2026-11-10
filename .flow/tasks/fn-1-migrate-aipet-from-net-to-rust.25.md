---
satisfies: [R17, R19]
---
# fn-1-migrate-aipet-from-net-to-rust.25 Cutover: remove .NET and cut the milestone 3 release

## Description
Remove the .NET code and tooling now that the Rust pet has shipped and the tests are moved. The golden generators go;
their fixtures stay frozen. Cut milestone 3.

**Size:** M
**Files:** `src/**` (delete), `tests/AiPet.Tests/**` (delete), `AiPet.slnx`, `Directory.Build.props`, `rust/golden/**` (delete; fixtures under the crates stay), `.github/workflows/{ci,release}.yml`, `scripts/check-plugin.sh`, `.gitignore`, `CHANGELOG.md`
**Touches:** [src/**, tests/**, AiPet.slnx, Directory.Build.props, rust/golden/**, .github/workflows/**, scripts/**, .gitignore, CHANGELOG.md]

### Approach
- Delete the C# projects, the solution and the build props.
- Delete the golden generators, and note in the fixtures' README that they are frozen outputs of the last C# build, with
  its commit hash.
- CI and release: remove the `setup-dotnet`/`dotnet` steps, except the .NET SDK that `vpk` needs in the Windows release
  job. Remove the cross-runtime jobs. Drop `AIPET_TEST_HOOK`.
- Check that nothing still references `src/` or the `.slnx` (a `git grep` in CI).
- CHANGELOG entry for milestone 3. The release is cut (the user starts it).

### Investigation targets
**Required:**
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- `rust/PARITY.md` — everything is mapped before deleting
**Optional:**
- `docs/HANDOFF.md` — "Before going public" (the history squash)
## Acceptance
- [ ] The repository builds, tests and releases with no .NET project. Only the Windows `vpk` step installs the .NET SDK.
- [ ] `git grep` finds no reference to removed paths outside the CHANGELOG.
- [ ] The milestone 3 dry run passes.
## Done summary
The .NET code is gone on branch `fn-1-cutover`, which merges into main only after 0.2.0 (milestone 2) is released from `fn-2-layered-sprite-and-vector-pets`. **Merge it, don't squash it:** the frozen C# values and fixtures name bf3696ea4a, the last commit with the C#.

**Deleted:**
- the C# projects, the solution and the build props;
- `rust/golden`, whose fixtures stay as frozen outputs and whose Linux registration and doctor corpora were recorded from the last C# build;
- `build/Win32Resources.targets` and `tools/IconGen`;
- `cross-runtime.yml`, whose two Rust-only steps moved to ci.yml;
- velopack-proof's .NET-to-Rust jobs, which `rust/proofs/velopack.md` keeps as the record.

**Tests and CI:**
- The tests that read C# sources now hold its values frozen.
- CI installs no .NET; the release installs it only for vpk on Windows.
- `scripts/check-removed-paths.sh` keeps removed paths from coming back.
- `check-parity.sh` checks the ledger against the Rust tests alone.

**Checks:**
- The milestone 3 dry run, 0.3.0-dry.1 (run 42803357583), passed every job.
- CI on the branch (run 42807766973) is green on Linux and Windows, after two fixes: an unused import on Windows, and fn-1.24's Windows-only testkit suites, fixed on the release branch and merged in.

**Still the owner's:**
- drop the removed jobs ("Hook start-up", the cross-runtime jobs) from branch protection if they're required;
- cut milestone 3 after 0.2.0 and the merge.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits:
- Tests:
- PRs:
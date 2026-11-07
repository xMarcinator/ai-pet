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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

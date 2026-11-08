---
satisfies: [R1, R4]
---
# fnt-70-rust-build-hygiene-hakari-sccache-and.1 Baseline, and one feature set with cargo-hakari

## Description
Measure the baseline, then give every build one feature set with cargo-hakari (R1, the first half of R4).

**Size:** S
**Files:** `rust/workspace-hack/**` (generated), `rust/.config/hakari.toml`, `rust/Cargo.toml`, each `rust/crates/*/Cargo.toml`, `rust/Cargo.lock`, `.github/workflows/ci.yml`, `rust/proofs/build-hygiene.md` (new)

### Approach
- Baseline first, in `rust/proofs/build-hygiene.md`: `du -sh rust/target` per profile, the number of hashed copies of the most duplicated crates (`ls target/*/deps | sed 's/-[0-9a-f]\{16\}.*//' | sort | uniq -c | sort -rn | head`), and a clean release and debug build's wall time.
- `cargo hakari init workspace-hack` (cargo-hakari is installed), `cargo hakari generate`, `cargo hakari manage-deps`; commit the crate and config. Keep `publish = false` and the workspace's metadata.
- Add `cargo hakari generate --diff` and `cargo hakari manage-deps --dry-run` (or `verify`) to `ci.yml`'s lint job and to the local gate used before commits (as a note in the contributor docs if there's no script).
- Check the release binary's size and that the pet starts.

## Acceptance
- [ ] The baseline is recorded (target size, duplicate copies, clean build times)
- [ ] `workspace-hack` is generated and committed; every crate depends on it; fmt, clippy and every test pass
- [ ] CI and the local checks fail on a stale hack crate (shown by a deliberate stale change, then reverted)
- [ ] The release binary's size is noted, and the pet starts


## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

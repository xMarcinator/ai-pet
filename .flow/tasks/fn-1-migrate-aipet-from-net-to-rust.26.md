---
satisfies: [R19]
---
# fn-1-migrate-aipet-from-net-to-rust.26 Documentation for the Rust codebase

## Description
Rewrite the docs for the Rust code: the architecture, the README's build section, the handoff's decisions (superseding
"stay on .NET"), the notices, and the plugin and Windows packaging READMEs. Fold in or retire the spike's documents.

**Size:** M
**Files:** `docs/ARCHITECTURE.md`, `README.md`, `docs/HANDOFF.md`, `THIRD-PARTY-NOTICES.md`, `plugins/aipet/README.md`, `packaging/windows/README.md`, `rust/SPIKE.md`, `rust/notes/**`
**Touches:** [docs/**, README.md, THIRD-PARTY-NOTICES.md, plugins/aipet/README.md, packaging/windows/README.md, rust/SPIKE.md, rust/notes/**, rust/proofs/**]

### Approach
- `ARCHITECTURE.md`:
  - Replace every `path/File.cs:NNN` citation with the Rust crate and module.
  - Rewrite §2 (layout), §3–4 (rendering, the window and bubbles, now iced and the layer-shell and winit shells), §9
    (runtime files) and §10 (threads).
  - Keep §6's contract text, which didn't change.
- README "Building from source": the Rust prerequisites from `SPIKE.md`, per distribution. The path table uses the
  crates.
- The handoff's Decisions: record that the migration supersedes "stay on .NET", and retire the antivirus and
  `UseAppHost` workaround.
- `THIRD-PARTY-NOTICES.md`: generated with `cargo-about`, including licence texts for Boost, Zlib, ISC, CC0 and
  Unicode-3.0, plus the font's OFL if one is bundled.
- The plugin README's maintainer commands and the Windows packaging README are updated for cargo.
- Fold `SPIKE.md`, `notes/` and `proofs/` into the architecture document or an appendix, then delete them.

### Investigation targets
**Required:**
- `docs/ARCHITECTURE.md`, `README.md`, `docs/HANDOFF.md`
- `THIRD-PARTY-NOTICES.md`, `plugins/aipet/README.md`, `packaging/windows/README.md`
**Optional:**
- `rust/SPIKE.md`, `rust/notes/*.md`
**From task 15 (2026-10-01):** rust/SPIKE.md and rust/proofs/windows.md still say `aipet-spike` (run commands, class and
namespace); the binary is now `AiPet` in the `aipet` crate.
## Acceptance
- [ ] No doc cites a removed `.cs` path or a `dotnet` build command as the way to build.
- [ ] The notices list every crate in the release builds with its licence text (`cargo about` check in CI).
- [ ] The handoff records the decision reversal. `SPIKE.md`, `notes/` and `proofs/` are retired.
## Done summary
Done on the fn-1-docs branch, merged into main as 2818b90 (commits marked fn-1.26, e.g. dc0bd91, 5bf91b8, 47e61cc, 548b950). ARCHITECTURE describes the Rust crates (layout, rendering, the two shells, runtime files, threads; §6's contract kept); README "Building from source" has the Rust prerequisites per distribution and a crate path table; the handoff records that the move to Rust supersedes "stay on .NET"; THIRD-PARTY-NOTICES.md is generated whole by cargo-about with every licence text, checked in CI (scripts/check-notices.sh); the plugin and Windows packaging READMEs use cargo. No doc cites a removed .cs path or a dotnet build command (scripts/check-removed-paths.sh in CI).
Deviation, accepted by the owner on 2026-11-10: rust/SPIKE.md, rust/notes/ and rust/proofs/ are kept as research records rather than deleted, because open specs still cite them; ARCHITECTURE's Appendix A lists them, and the milestone 3 changelog says so.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 6ee1ac5a702a467b56d7c433261d6570c10f1bad, 980bad6fda9bdae06605cce42661e82857dedabd, be2f986b7e042acf93590044e2cc00bf0b33d6c2, 256e340a7edd862f98897abed104d0c4984c91aa, 62ac5a6eab4919663deaa0979c61d404900aa49c, b01e6fa56d7eddebf87466331d856957193dd2d7, dc0bd913e6f04b19d65a51a33c0d11dc138a2de8, 5bf91b86c61d119f0828870a43cf9eab0d13781f, 47e61cc07ab2463772487825d9bd8d5fd0dfde64, 548b9501f48001246d06a7c70920dab934562362, 2818b90f8e4f5992691527230449fb8c75801b5c
- Tests:
- PRs:
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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

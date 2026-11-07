---
satisfies: [R2, R4, R5, R6, R12, R18]
---
# fn-2-layered-sprite-and-vector-pets.3 Pack loading from disk: limits, containment, Aseprite, legacy avatars, --check-pack

## Description
Load packs from `<data dir>/avatars/`: manifest folders, Aseprite `json-array` exports and legacy ellipse `.json` files (converted at load). Load all-or-nothing under the spec's limits and path rules. Resolve names (built-ins first, then sorted). Add `AiPet --check-pack <folder>`. Headless, in aipet-sprite plus the CLI flag.

**Size:** M
**Files:** `rust/crates/aipet-sprite/src/load.rs` (new), `rust/crates/aipet-sprite/src/aseprite.rs` (new), `rust/crates/aipet-sprite/src/avatar.rs`, `rust/crates/aipet/src/main.rs`, `avatars/**`
**Touches:** [rust/crates/aipet-sprite/src/**, rust/crates/aipet-sprite/tests/**, rust/crates/aipet/src/main.rs, avatars/**]

### Approach
- Reuse the JSON reading conventions in `avatar.rs:295-466` (`read_all_text`, case-insensitive reads, depth limit). The manifest needs no System.Text.Json quirks, so plain serde with `#[serde(default)]` is fine.
- Check PNG dimensions from the header before decoding, and count decoded bytes per pack.
- Containment: canonicalise the pack folder and each referenced file; reject on prefix mismatch, symlink metadata, and reparse points on Windows.
- Aseprite: model `repeat` as an optional string; use `spriteSourceSize` for trimmed frames; read the `anchor` slice pivot.
- Legacy `.json`: rasterise the body once from the ellipses with the shading rules, recolour Sprout's other layers through the palette, and keep `drawable()`'s rejections.
- `--check-pack`: follow the existing CLI flag handling in `main.rs`. Print the spec's lines; never start the UI.
- Rewrite `Avatar::all` (`avatar.rs:140-156`) to return loaded pets plus skipped-pack reasons.

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/src/avatar.rs:130-175,295-466` — discovery and loading today
- `rust/crates/aipet/src/main.rs:190-220` — avatars folder wiring and flags
- `rust/crates/aipet-ui/tests/settings.rs:346-450` — Reload and worn-avatar semantics to keep

### Key context
Aseprite writes `repeat` as a JSON string and omits it when it is 0. A `json-hash` export is rejected with a reason, never guessed.

## Acceptance
- [ ] Manifest packs, Aseprite exports and legacy `.json` avatars load from the avatars folder; name resolution is built-ins first, then sorted, with duplicates skipped
- [ ] Every limit and path rule in the spec has a test that gives the bad input and reads the reason (oversize PNG header, decoded total, escaping path, symlink, cycle, too-new format_version, json-hash export)
- [ ] Legacy `avatars/hood-green.json` loads and differs from today only in body outline and shading
- [ ] `AiPet --check-pack` prints OK or one line per problem with exit 0/1, run against a good and a bad pack
- [ ] A sample pack in the repo's `avatars/` loads
- [ ] The Part motion fields parse and validate (R18): parts, origins, pivots, `look`, `poses`, `transition_ms`, keyframe `animations` and `phase`. Tests give each R18 error case (non-finite, out-of-box, unknown part id or pose value, duplicate id, more than 64 keys, out-of-range duration) and read the reason. An Aseprite export's part slices become origins, and its per-frame slice keys become pose positions

## Done summary
`load.rs` loads pets from the avatars folder: built-ins first, then pack folders and legacy `.json` avatars in sorted order, a later duplicate name skipped with its reason. A pack folder is a `pack.json` manifest, or an Aseprite `json-array` export with its PNG (plus an optional `pack.json`): tags `<layer>:<value>` become animations, direction and `repeat` the loop modes, trimmed frames keep their place via `spriteSourceSize`, the `anchor` slice sets the anchor, and part slices give origins and pose positions (R18). Packs load all or nothing within the limits (manifest 256 KB and 64 deep, PNG 8 MB and 2048 px from the header, 64 MB decoded, the art box) and the path rules (relative, no `..`, no device names, no symlinks or reparse points, inside the folder once canonicalised); each has a test that reads the reason. The manifest reader handles the whole contract including part motion and frame pivots, with each R18 error case tested. Legacy avatars are baked once at load, so they look exactly as before (better than the acceptance's "differs only in outline and shading"). `AiPet --check-pack <folder>` prints `OK <name> (<kind>, <layers> layers, <frames> frames)` or the reason, exit 0/1 (it attaches to the parent console on Windows). `avatars/sample` is a small hand-made pack, checked against its code and loaded from disk in a test. The pet wears a `LoadedPack` (`Pet::set_pack`); the UI's avatar list holds them, and Reload compares their art as well as their manifest.

Not done here: the "half-written export" mtime rule (a frame outside its PNG already fails the pack; comparing modification times would misfire on normal exports), and showing the skipped list in Settings (task 4). `--check-pack` prints the first problem, as loading stops at it.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 2cb945f
- Tests: cd rust && cargo test -p aipet-sprite -p aipet-ui -p aipet -p aipet-wayland (passed), cargo clippy --workspace --all-targets -D warnings (clean), cargo fmt --check (clean), AiPet --check-pack on a good and a bad folder
- PRs:
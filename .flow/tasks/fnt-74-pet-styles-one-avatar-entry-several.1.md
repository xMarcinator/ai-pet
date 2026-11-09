---
satisfies: [R1, R2, R5, R6]
---
# fnt-74-pet-styles-one-avatar-entry-several.1 Loader: pets and styles, pet folders, the built-ins as four pets, check-pack

## Description
Group loaded packs into pets with styles (R1), tag the built-ins (R2), keep theme files attaching by pack name (R5), and extend `--check-pack` (R6). Headless: no UI yet.

**Size:** M
**Files:** `rust/crates/aipet-sprite/src/{load,manifest,pack,builtin}.rs`, `rust/crates/aipet/src/main.rs`, `avatars/reference/{hood_gen,sprout_gen,pip_gen,hamster_pixel_gen}.py`, the built-in `pack.json` files, `rust/crates/aipet-sprite/src/painted.rs`
**Touches:** [rust/crates/aipet-sprite/src/**, rust/crates/aipet-sprite/packs/**, rust/crates/aipet/src/main.rs, avatars/reference/hood_gen.py, avatars/reference/sprout_gen.py, avatars/reference/pip_gen.py, avatars/reference/hamster_pixel_gen.py, rust/crates/aipet-sprite/tests/**]

### Approach
- Manifest: optional `pet` and `style` (`manifest.rs:28-55` `PackJson`; unknown keys are already ignored by older readers); `Pack` gains `pet` and `style` (defaults: `name`, kind's name).
- Loader: `load::all` (`load.rs:134-187`) gains a pet folder pass: a folder whose subfolders hold `pack.json` (or `.aseprite`) loads each as a style of the folder's name. `keep()` (`load.rs:177`) dedupes by (pet, style) and still by name, and refuses a pack named as another pet, each skip with a reason that names the first, so pack names and pet names stay unambiguous for theme files and saved configs.
- Containment: refuse a pet folder that is a symlink or reparse point before reading its children, as `load_folder` refuses a linked pack today.
- `Found` gains the grouping: pets in load order (built-ins first), each with its styles and its default (the pack named as the pet, else the first). Keep the flat `pets` list for callers until task 2 moves them.
- Built-ins: add `"pet"` to the generated manifests (Hood (vector) -> Hood; Sprout (vector) -> Sprout; Hamster (pixel) -> Hamster) and to painted Pip (`painted.rs:23`, pet Pip); regenerate and rebake (`AIPET_BAKE=1`).
- Theme files: `attach()` (`load.rs:231-248`) still matches a pack name first; a bare pet name attaches to its default style. Return the pet and style it attached to, and print both in `check_theme`'s line.
- `--check-pack` (`load.rs:990-1056`, `main.rs:59-83`): add `pet <P>, style <S>` to the OK line; a pet folder checks each style.

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/src/load.rs:115-250,990-1056`
- `rust/crates/aipet-sprite/src/builtin.rs`
- `rust/crates/aipet-sprite/src/load.rs:1105-1130` - the test pinning the built-in list

### Key context
- FNT-72's art tasks edit the same generators and packs: rebase on whichever lands first.

## Acceptance
- [ ] Tests: the built-ins group into Hood, Sprout, Pip, Hamster with their styles and defaults
- [ ] Tests: a pet folder with pixel and vector subfolders loads as one pet; a duplicate (pet, style) is skipped with a reason naming the first; a built-in wins
- [ ] Tests: a theme file naming `Hood (vector)` attaches to the vector Hood; one naming `Hood` to the pixel Hood
- [ ] `--check-pack` prints pet and style, one line per style for a pet folder, exit 1 if any failed; on a theme file it names the pet and style it attaches to
- [ ] Tests: two vector styles of one pet with distinct `style` labels both load; a child folder without a pack and a pet folder with no loadable style are skipped with their reasons
- [ ] Tests: two packs with one name, and a pack named as another pet, are skipped with reasons; a linked pet folder is refused (Unix symlink; Windows where the test can make one)

## Done summary
Pack pet and style (optional overrides; defaults name and kind label), written only when set. load::all reads pet folders (subfolders as styles, pet defaulting to the folder name; a link refused; no loadable style -> skipped). Clash rules: same name, same (pet, style), or a name that is another pet's -> skipped with a reason naming the first; built-ins win. Found::groups / load::groups give pets in load order with their styles and default (the drawing named as the pet, else the first). Built-ins tagged (Hood (vector), Sprout (vector), Hamster (pixel), painted Pip) -> Sprout, Hood, Pip, Hamster. Theme files may name a pet alone (default style); check lines name pet and style; a pet folder checks a line per style.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: f29fc14
- Tests: cargo test --workspace
- PRs:
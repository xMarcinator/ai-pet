---
satisfies: [R7, R8]
---
# fnt-72-new-pet-states-and-the-hamsters.4 Baked pixel packs get the new states, and R22 covers every pack and state

## Description
The Sprout and Hood pixel packs (Hood's Green theme included) are baked in Rust, so their new art is new draw code plus a rebake. Then extend the R22 coverage test with a state channel and the baked packs, which makes every built-in pet prove it draws every new value.

**Size:** M
**Files:** `rust/crates/aipet-sprite/src/bake.rs`, `rust/crates/aipet-sprite/packs/{sprout,hood}/**`, `rust/crates/aipet-sprite/tests/{bake,vector_packs}.rs`
**Touches:** [rust/crates/aipet-sprite/src/bake.rs, rust/crates/aipet-sprite/packs/sprout/**, rust/crates/aipet-sprite/packs/hood/**, rust/crates/aipet-sprite/tests/**]

### Approach
- Add `draw_*` functions for tears, aura, heart, the envelope, sleepy eyes and the yawn beside the existing ones (`bake.rs:288-396`), in the pixel pet's grid; register the values in `built_in` (`bake.rs:730`); rebake with `AIPET_BAKE=1 cargo test -p aipet-sprite --test bake`.
- `tests/vector_packs.rs:82` `USED`: add a `state` channel (sad, tired, news) and the new fx, prop and eyes values; run it over the baked packs too, naming the pack and missing value on failure. Keep the per-layer `explicit()` check, and add that at least one layer of each pack resolves every required value to drawn art: a `fallback: null` alone must fail.

### Investigation targets
**Required:**
- `rust/crates/aipet-sprite/src/bake.rs:288-396,700-760`
- `rust/crates/aipet-sprite/tests/vector_packs.rs:60-120`
- `rust/crates/aipet-sprite/tests/bake.rs`

### Key context
- The golden frame replay draws the legacy pixel pets: the new values must not change any frame it replays (it never sets the new inputs).

## Acceptance
- [ ] The baked Sprout and Hood (Blue and Green) draw every new value; `tests/bake.rs` passes
- [ ] The R22 test checks the state channel and every built-in pack, baked ones included, and fails naming pack and value when one is removed or replaced by a `null` fallback
- [ ] `tests/golden.rs` still passes

## Done summary
Baked pixel Sprout and Hood (Blue and Green) draw sleepy eyes and a yawn (tired), an envelope (news), tears, hearts, and the golden glow with stars; dust/fx/sparkles skip the new values. R22 now covers every built-in pack and theme, the state channel through each mood's own art or sign, and drawn art per value (a null fallback alone fails; a negative test proves it). UI tests updated: Sprout now shows news as news, calm while holding the envelope.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: af5cc36
- Tests: cargo test --workspace, cargo clippy --workspace --all-targets
- PRs:
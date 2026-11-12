# fn-2-layered-sprite-and-vector-pets.25 Polish from the Windows hands-on: bubble gap, remembered styles, ghosts, Hamster

## Description
Fixes from the owner's Windows run of the 0.2.0-dry.3 installer (2026-11-10, `rust/proofs/install.md`), kept out of fn-1.23 so the next tasks can start.

**Size:** M
**Files:** `avatars/reference/hood_gen.py`, `rust/crates/aipet-sprite/packs/hood-vector/pack.json`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-ui/src/settings/avatars.rs`, `rust/crates/aipet-desktop/src/**`
**Touches:** [avatars/reference/**, rust/crates/aipet-sprite/packs/**, rust/crates/aipet-ui/src/**, rust/crates/aipet-desktop/src/**]

### Items
1. **Bubble gap (done, 65bb49b).** The vector Hood's effects were drawn 11-14 art px higher than the pixel Hood's, and its glow reached above the art box, so the bubbles (which clear the highest effect, R24) sat about 20 px higher at 150 %. Effects lowered 12 art px (tears and dust kept in place), glow box (130, 144, 96, 92). Reach -17 against the pixel Hood's -15.
2. **Settings remember each pet's style and theme (written, UI tests pass, not yet committed when this task was made).** `PetUi::chosen` keeps the style and theme each pet was last worn in this run; its tile shows them and clicking it puts them back on; reload keeps the tiles' themes. Test `a_pets_tile_puts_back_the_style_and_theme_it_was_last_worn_in`. Not kept across restarts (config.json holds only the worn pet's).
3. **Ghosts on Windows when the pet jumps or its size changes.** The owner's screenshot shows a larger stale copy of the pet over the bubbles. The desktop shell's window never resizes (380 x 600), so it isn't Linux's surface-shrink ghost (ecdb558). Hypothesis: GL on Windows with an `Opaque` swapchain (`rust/proofs/windows.md`): pixels cleared to alpha 0 may not replace what DWM composited there before, inside the `SetWindowRgn` region. First check with the owner: does `WGPU_BACKEND=vulkan` make the ghosts go? FNT-75 replaces this path on Windows (UpdateLayeredWindow); if no small fix holds, record it there.
   **Owner's check (2026-11-10):** with `WGPU_BACKEND=vulkan` there are no ghosts, so GL on Windows causes them. A little flicker and clipping remains while the size changes: likely the `SetWindowRgn` region changing a frame before the frame drawn at the new size, which FNT-75's UpdateLayeredWindow does in one step. Open: make Vulkan the Windows default until FNT-75 (no ghosts; 5-7 s to show instead of 0.9 s, and the NVIDIA driver loaded on hybrid laptops), or keep GL.
4. **Hamster likeness (needs the owner).** The owner asked for the pixel Hamster to look exactly like JLandbo/hamster-pet's, with our states, and the vector Hamster to follow. That repository has no licence (the standing rule: copy ideas, never pixels), so this waits for Jacob's permission, ideally a licence in his repository.

## Acceptance
- [ ] The vector and pixel Hood's bubbles sit at the same height (within 3 px at 150 %)
- [ ] Switching pets in Settings and back puts on the style and theme last worn
- [ ] No stale pet copies on Windows when it jumps or changes size, or the cause recorded in FNT-75
- [ ] The Hamster decision recorded (permission, or it stays an original drawing)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

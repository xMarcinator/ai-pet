---
satisfies: [R3, R4, R9, R10]
---
# fnt-75-the-pet-window-without-the-gpu.5 The Wayland shell on smithay-client-toolkit, and the backend switch

## Description
Replace iced_exwlshell for the pet with an sctk 0.21 layer surface presenting the scene's pixmap. Also wire the backend choice: the new shells by default, `AIPET_BACKEND=iced` for the old pet shell, and drop the GPU defaults from the pet path.

**Size:** M
**Files:** `rust/crates/aipet-wayland/src/{shell,lib,probe}.rs`, `rust/crates/aipet/src/main.rs`, `rust/Cargo.toml`
**Touches:** [rust/crates/aipet-wayland/**, rust/crates/aipet/src/main.rs, rust/Cargo.toml, rust/Cargo.lock]

### Approach
- Layer surface as `shell.rs:51-68,340` (namespace `aipet`, Top layer, bottom-right, START); commit, wait for configure, ack, then attach.
- Two-buffer `SlotPool`, R/B swap of changed rects, `damage_buffer`, frame callbacks while animating with a timer fallback; nothing committed when calm.
- Input region at most every 100 ms and never mid-drag (`sync_region` :748); drag via `drag.rs` both strategies; cursor-shape v1; xdg popup menu with the inline fallback (:505-590); output loss and re-make (:630).
- Fractional scale and viewporter; integer fallback.
- `choose_shell` (`main.rs:175`): order layer-shell, X11; `AIPET_BACKEND=iced` keeps the old shell; `set_gpu_defaults`/`mesa.rs` only for the iced shell.
- Port the 16 shell tests to the new shell's state machine.

### Investigation targets
**Required:**
- `rust/crates/aipet-wayland/src/shell.rs:51-68,340,505-590,630,748-836,875-1281`
- `rust/crates/aipet-wayland/src/drag.rs:36-73,234,306`
- `rust/crates/aipet/src/main.rs:146-257`

### Key context
- sctk 0.21: `delegate_dispatch2!`; no fractional-scale helper (bind the protocol with `SimpleGlobal`).
- Using an shm format the compositor didn't advertise kills the connection: ARGB8888 only.

## Acceptance
- [ ] On Hyprland the pet runs on the new shell with placement, region, both drag modes, menu, output loss and fractional scale as today
- [ ] `AIPET_BACKEND=iced` runs the old shell; an unknown value logs the choices
- [ ] The ported shell tests pass; Hyprland rules still match

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

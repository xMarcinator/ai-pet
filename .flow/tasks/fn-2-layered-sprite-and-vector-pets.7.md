---
satisfies: [R13]
---
# fn-2-layered-sprite-and-vector-pets.7 Reduced motion

## Description
Read the system's reduced-motion setting on each platform and apply the spec's reduced behaviour: first frames only, no bob, squash or idle acts, but state and expression changes still show. `AIPET_REDUCE_MOTION=1` forces it on.

**Size:** S
**Files:** `rust/crates/aipet-desktop/src/platform/**`, `rust/crates/aipet-wayland/src/**`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet-sprite/src/pose.rs`
**Touches:** [rust/crates/aipet-desktop/src/platform/**, rust/crates/aipet-wayland/src/**, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet-sprite/src/pose.rs]

### Approach
- Windows: `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)`. Add the windows-sys feature only if it isn't present (`rust/crates/aipet-desktop/Cargo.toml:31-40`).
- Linux: the XDG desktop portal settings (`org.freedesktop.appearance`, if it offers a reduced-motion key), else GNOME `enable-animations` via gsettings over D-Bus. Don't spawn processes.
- macOS: `NSWorkspace.accessibilityDisplayShouldReduceMotion`.
- Re-read on Settings open, not every tick.

### Investigation targets
**Required:**
- `rust/crates/aipet-desktop/src/platform/` — platform service pattern
- `rust/crates/aipet-ui/src/lib.rs:482-534` — tick

### Key context
A platform that can't read the setting counts as off.

## Acceptance
- [ ] With `AIPET_REDUCE_MOTION=1`, animations hold their first frame and motion is still; state and expression changes still show (tests)
- [ ] Windows and Linux read the system setting (unit tests where the API can be stubbed; checked hands-on in the verification task)

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

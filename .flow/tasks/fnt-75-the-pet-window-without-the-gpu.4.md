---
satisfies: [R7]
---
# fnt-75-the-pet-window-without-the-gpu.4 Settings as a child process: `AiPet --settings` and the Settings pipe

## Description
Move Settings out of the pet process. The pet starts `AiPet --settings` with piped stdin/stdout and speaks the spec's Settings pipe; the child runs today's iced Settings pages against a state mirror and sends changes back. Try iced's tiny-skia renderer for the child; keep wgpu if a page needs it.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/**`, `rust/crates/aipet-ui/src/lib.rs`, `rust/crates/aipet/src/main.rs`, a pipe module
**Touches:** [rust/crates/aipet-ui/src/settings/**, rust/crates/aipet-ui/src/lib.rs, rust/crates/aipet/src/**]

### Approach
- Split `settings::update` (`lib.rs:968`, `settings/mod.rs:10-33,177-210`) into page state (child) and effects on the pet (sent as `set`, `wear`, `themes_changed`, `services_changed`, `mood`).
- The child does its own connection tests with its own `Http` and writes secrets to the shared `SecretStore`; the pet reloads watchers on `services_changed`.
- The pet sends `state` on start and after every change; one child at a time, `raise` for a second open; reap and log a crashed child; the child exits when stdin closes.
- Window title stays "AiPet · Settings"; `AIPET_OPEN_SETTINGS=1` keeps working.
- Every page: a round-trip test per page (General, Avatars, Jira, GitHub, About) that the state the pet sends fills every field, each action reaches the pet as its message (including reset position, check for updates, restart to update), and a change made outside Settings (menu, drag, a config change) refreshes the open window.
- Check GPU libraries: `grep -E 'libGL|libEGL|libvulkan|nvidia' /proc/<pet>/maps` empty with Settings open.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/settings/mod.rs` - pages, services
- `rust/crates/aipet-ui/src/lib.rs:415,968,1150` - Settings inside PetUi
- `rust/crates/aipet-ui/tests/settings.rs`, `tests/settings_services.rs` - behaviour to keep

### Key context
- The hook socket is frozen v1 shared with the C# pet: don't extend it.

## Acceptance
- [ ] `AiPet --settings` opens from the menu, every page works, and changes show live in both directions
- [ ] The pet process maps no GPU or GL library with Settings open or closed
- [ ] Second open raises, crash is logged and reopenable, pet exit closes the child; settings tests ported and passing

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

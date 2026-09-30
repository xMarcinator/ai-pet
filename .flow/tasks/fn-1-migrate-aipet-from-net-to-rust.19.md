---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.19 Settings: General and Avatars pages, Import defaults

## Description
Bring the Settings window to parity for the General page (without its updates section) and the Avatars page. The
spike's page has switches and an avatar picker only.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/{general,avatars}.rs` (stubs from task 15), `rust/crates/aipet-ui/tests/settings.rs`
**Touches:** [rust/crates/aipet-ui/src/settings/general.rs, rust/crates/aipet-ui/src/settings/avatars.rs, rust/crates/aipet-ui/tests/settings.rs]

### Approach
- Task 15's groundwork provides `settings/mod.rs` (the window, the shared rows and fields, and the actions),
  `rfd`, and the `Platform` trait. Reset position emits the action task 16 handles; Open data folder and Open avatars
  folder go through the platform. Keep the General page's slot for the updates section (`settings/updates.rs`, task
  21's). This task touches no `mod.rs` or Cargo file.
- General (`src/AiPet.UI/SettingsWindow.axaml.cs:96-125`):
  - the switches: chat bubbles, always on top, and "Listen along with <player>" when a player exists;
  - Reset position, and Open data folder.
  - Import defaults… (`:154-204`): a file dialog (`rfd`), `Preset` parsing, applying it to the Jira and GitHub settings,
    and forgetting a token when the preset moves that service to another site.
- Avatars (`:214-261`): every avatar as a still, Reload, and Open avatars folder (created if missing). Picking one
  applies it and its accent colour.
- The spike's QA mood picker is compiled only with the `demo` feature.

### Investigation targets
**Required:**
- `src/AiPet.UI/SettingsWindow.axaml.cs:1-272`, `src/AiPet.UI/SettingsWindow.axaml`
- `rust/crates/aipet-ui/src/settings.rs`
**Optional:**
- `src/AiPet.Core/Presets.cs` — `MovesJira`/`MovesGitHub`
## Acceptance
- [ ] Tests: importing a preset that moves the Jira or GitHub site forgets that token; one that doesn't keeps it; a bad
      file shows the C#'s message.
- [ ] Screenshots of both pages. The switches persist through `config.json`.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

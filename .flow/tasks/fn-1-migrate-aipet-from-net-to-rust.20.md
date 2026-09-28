---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.20 Settings: Jira and GitHub pages

## Description
The Jira and GitHub Settings pages: fields, Test, Save and Forget, a token box that never shows the saved token, and the
watchers restarting on save.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/{jira,github}.rs`, `rust/crates/aipet-ui/tests/settings_services.rs`
**Touches:** [rust/crates/aipet-ui/src/settings/jira.rs, rust/crates/aipet-ui/src/settings/github.rs, rust/crates/aipet-ui/src/settings/mod.rs, rust/crates/aipet-ui/tests/settings_services.rs]

### Approach
- Jira (`src/AiPet.UI/SettingsWindow.axaml.cs:284-347`): site, email, JQL and token. Test runs the watcher's search on a
  thread and shows the C#'s messages. Save writes `jira.json` and the secret, then restarts the watcher. Forget removes
  the secret.
- GitHub (`:350-412`): host, orgs and token. Test uses the watcher's test call. Save and Forget as for Jira.
- The token field is write-only: it never shows a saved token.

### Investigation targets
**Required:**
- `src/AiPet.UI/SettingsWindow.axaml.cs:280-413`
- `src/AiPet.Core/Jira.cs`, `src/AiPet.Core/GitHub.cs` — the Test, Save and Forget APIs
**Optional:**
- `src/AiPet.UI/SettingsWindow.axaml` — layout
## Acceptance
- [ ] With a mock server, Test success and failure show the C#'s texts. Save persists and restarts the watcher. Forget
      removes the token.
- [ ] A saved token is never shown in the UI.
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

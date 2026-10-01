---
satisfies: [R12]
---
# fn-1-migrate-aipet-from-net-to-rust.20 Settings: Jira and GitHub pages

## Description
The Jira and GitHub Settings pages: fields, Test, Save and Forget, a token box that never shows the saved token, and the
watchers restarting on save.

**Size:** M
**Files:** `rust/crates/aipet-ui/src/settings/{jira,github}.rs` (stubs from task 15), `rust/crates/aipet-ui/tests/settings_services.rs`
**Touches:** [rust/crates/aipet-ui/src/settings/jira.rs, rust/crates/aipet-ui/src/settings/github.rs, rust/crates/aipet-ui/tests/settings_services.rs]

### Approach
- Task 15's groundwork provides `settings/mod.rs`, the page stubs and the shared fields; this task doesn't touch
  `mod.rs`.
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
**From task 11 (2026-09-30):** Jira's Test is `jira::search(&http, &settings, token)`; the page's own texts ("Paste an
API token first.", "Connected. The search finds ...") are this task's. GitHub's is `github.test(host, typed_token)`,
which gives `Ok(message)` or `Err(message)`. `save(settings, new_token)` (an empty token keeps the stored one) and
`forget_token()` restart the loop, as in the C#. Import defaults uses `Preset::parse` (it takes the file's text and
drops a leading BOM), `apply_to_jira`, `apply_to_github`, `moves_jira`, `moves_github` and `describe`. Mock-server tests
use `Http::local(timeout)`, plain HTTP to this computer only (point the site or host at `127.0.0.1:port`), and the
doc-hidden `watcher.poll()` for one synchronous poll.
**From task 15 (2026-10-01):** your files are `aipet-ui/src/settings/{jira,github}.rs`: empty `Message` enums and page
states, `Services { jira, github, secrets, http }`, `field(.., secret)`, `status` with a `Tone`, and `PetUi::settings()`
for tests.
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

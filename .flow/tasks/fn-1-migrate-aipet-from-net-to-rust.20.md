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
I filled in Settings' Jira and GitHub pages (aipet-ui/src/settings/jira.rs, github.rs). Each page has its watch box, its fields, its buttons (Test, Save, Create a token, Remove saved token) and a status line with the C#'s texts. Test runs on a thread, and `tick` picks up the result. Save writes jira.json or github.json and the typed token, then restarts the watcher. Forget deletes the token. The token box only ever holds what was typed: Save empties it, and the saved token is never put back into it.

Acceptance:
- Test with a mock server, success and failure, shows the C#'s texts. This is tests/settings_services.rs, `jira_tests_saves_and_forgets_with_the_csharps_texts` and `github_tests_saves_and_forgets_with_the_csharps_texts`, against real watchers, `Http::local` and a stub.
  - Jira: "Paste an API token first.", "Searching…", "Connected. The search finds 2 issue(s), e.g. ABC-1: Fix the login", the 401 text, and "Connected. The search finds no issues right now.".
  - GitHub: "Connecting…", "Connected as octo. 3 pull request(s) waiting on your review.", "GitHub didn't accept the token" and "Paste an access token first.".
  - Save writes the file and the secret (GitHub's trimmed), shows the C#'s Saved text, and the stub then receives the restarted watcher's first poll with the saved token. Forget deletes the secret and shows the C#'s text.
  - Create a token opens the C#'s URLs, through the recording platform.
  - `github_says_where_the_watcher_stands_when_no_button_has_spoken` checks UpdateGitHubStatus's default lines.
- A saved token is never shown. `a_saved_token_is_never_shown`: with both tokens saved, the token boxes are empty and the hints say a token is saved. Test uses the saved token. Save with an empty box keeps the saved token. At no point does the pages' state (Debug of both pages and the status line) contain either token.
- Also checked: `settings_saved_elsewhere_fill_the_fields_and_leave_the_token_box_alone`. A save through the watcher (Import defaults) refills the fields on the next frame and keeps the typed token. Removing that refill fails this test, and removing the token clear or the GitHub status reset fails the others.

Choices to review:
- Settings lives as long as the pet, so the fields are filled on the first frame and again whenever the watcher's saved settings change. Task 19's import only has to save through the watchers.
- When no button has set a status, the line is worked out live: Jira shows the watcher's last error, GitHub says where it stands. The C# worked this out once, when the window opened.
- Without watchers (the demo), Save, Remove and GitHub's Test are disabled.
- A failed Save shows "Couldn't save the settings: <error>".
- The watch box, hint and status box helpers live in jira.rs, and github.rs uses them. mod.rs is untouched.

baseline: green (Rust quick command at 2d6f460).
Verify: green at 216c4a1, with `-D warnings` and `--offline`; receipt .flow/tmp/green-receipts/216c4a1a-unittest.json.
- The first verify run hit the inherited 500 ms timing flake in aipet-core tests/watchers.rs (`jira_errors_are_the_csharps`, `github_errors_are_the_csharps`), which this task doesn't touch. Those tests passed 19/19 alone, then the full gate was green.
- `dotnet test` was not run: no C#, hook or core change.
- Cross-target clippy was not run: the new code has no cfg paths.

Manual checks for the hands-on passes (Git Bash at the repository root; quit the .NET pet first):
1. `(cd rust && cargo run -p aipet)`, menu → Settings → Jira. The fields show jira.json, and the token box is empty, with "A token is saved. Leave this empty to keep it." when one is saved.
2. Test search, with a real site, email and token: "Searching…" then "Connected. …". With a wrong token: "Jira didn't accept the email/API token".
3. Save. The token box empties, the status says the Saved text, and an issue bubble appears within about 2 s. Remove saved token: its text shows, and the button goes.
4. GitHub page: typing a token ticks the box. Test connection gives "Connected as <login>. …". Then Save, Remove saved token, and Create a token (opens <host>/settings/personal-access-tokens/new).
5. `cargo run -p aipet --features demo`: both pages show, and Save and Remove are disabled.

Follow-up (outside Touches): settings/mod.rs `field`'s `#[allow(dead_code)]` can go. Notes: NOTES_DIR/task-20-settings-jira-github.md.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

Review fixes: 2486601 (Save and Forget cancel a running Test), 0093af4 (closing Settings drops the Jira and GitHub pages' unsaved token, edits, status and Test, through settings::closed).

stage: impl-review - ran (codex: NEEDS_WORK then SHIP)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 216c4a1a2f51ee1f672cda329bbd623a092903fc, 2486601, 0093af4
- Tests: baseline: green (cd rust && cargo test --workspace --offline && cargo clippy --workspace --all-targets --offline && cargo fmt --all -- --check, at 2d6f460), cd rust && cargo test --workspace --offline && cargo clippy --workspace --all-targets --offline -- -D warnings && cargo fmt --all -- --check (green at 216c4a1; receipt .flow/tmp/green-receipts/216c4a1a-unittest.json), first verify run: inherited timing flake in aipet-core tests/watchers.rs (jira_errors_are_the_csharps, github_errors_are_the_csharps: 500 ms client timeout under load, untouched by this task); 19/19 alone, then full gate green, cargo test -p aipet-ui --offline --test settings_services (5 passed; mutation checks: no token clear, no GitHub status reset, no refill each fail a test), dotnet test AiPet.slnx: not run (no C#, hook or core change in this task), integrated verify (Windows, work branch 4ff23c3 with tasks 19, 20 and 21 and their review fixes merged): cargo test --workspace --no-fail-fast -- --skip credential_manager_tokens_are_the_apps green, clippy -D warnings and fmt clean; at 436d693: dotnet build and test 161 passed, 53 skipped, C# hook tests on the Rust hook 23 passed, Rust server with the .NET hook green; the live doctor replay, which failed in the full gate under load (the antivirus flake), passed alone at 4ff23c3
- PRs:
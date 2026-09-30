---
satisfies: [R9]
---
# fn-1-migrate-aipet-from-net-to-rust.11 Jira and GitHub watchers and presets

## Description
Port the Jira and GitHub watchers and the presets, with the C#'s endpoints, intervals, error texts and the preset
token-safety rule.

**Size:** M
**Files:** `rust/crates/aipet-core/src/{jira,github,presets,http}.rs`, `rust/crates/aipet-core/tests/{watchers,presets}.rs`
**Touches:** [rust/crates/aipet-core/src/jira.rs, rust/crates/aipet-core/src/github.rs, rust/crates/aipet-core/src/presets.rs, rust/crates/aipet-core/src/http.rs, rust/crates/aipet-core/tests/watchers.rs, rust/crates/aipet-core/tests/presets.rs]

### Approach
- HTTP: `ureq` with rustls and a 20 s timeout, like the C#'s shared `HttpClient`.
- Jira (`src/AiPet.Core/Jira.cs`): `/rest/api/3/search/jql`, Basic auth from email and token, polling no more often than
  every 30 s, the `jira.json` settings, `AiPet:Jira` in the secret store, `SearchAsync` for the Test button, and new
  reviews firing an alert.
- GitHub (`src/AiPet.Core/GitHub.cs`): the GraphQL search for `review-requested:@me`, polling no more often than every
  60 s, Jira keys matched in the title, branch, body and commits, the host handling (both `http://` and `https://`
  stripped), `TestAsync`, and `AiPet:GitHub`.
- Loops run on threads with a stop flag. Events go through channels.
- Presets (`src/AiPet.Core/Presets.cs`): the same validation and messages. `MovesJira`/`MovesGitHub` drive forgetting the
  token.
- Tests against a local mock HTTP server: success, 401, 403, rate limits, timeouts and no network.

### Investigation targets
**Required:**
- `src/AiPet.Core/Jira.cs`, `src/AiPet.Core/GitHub.cs`, `src/AiPet.Core/Presets.cs`
- `tests/AiPet.Tests/PresetTests.cs`
**Optional:**
- `docs/ARCHITECTURE.md` §8 (Jira and GitHub)
## Acceptance
- [ ] Each `PresetTests` case is ported and passes.
- [ ] The mock-server tests show the same requests (URL, method, headers, body shape) and the same error texts as the C#.
- [ ] The minimum intervals are enforced. A new review fires an alert once.
## Done summary
Ported the Jira and GitHub watchers and the presets to aipet-core. `jira::search` and `GitHubWatcher`'s GraphQL send the C#'s requests: URL, method, headers and body, checked byte for byte against what the C# sent to a stub under .NET 10. They give the C#'s error texts. The loops run on threads with a stop flag and send `Changed`/`NewReviews` through a channel. `Preset` has the C#'s validation, messages and token-safety moves.

Acceptance:
- Each PresetTests case is ported and passes (tests/presets.rs: all 9 methods, the 12 refusal cases checked against the full message, the 5 Jira-move and 4 GitHub-move cases). Also checked: comments, trailing commas and a BOM are accepted, and a file that isn't JSON is refused at the line .NET 10 names.
- Mock-server tests (tests/watchers.rs, 17 tests) run against a local stub.
  - Jira request: GET /rest/api/3/search/jql?jql=<EscapeDataString>&fields=summary,status,updated&maxResults=25, with exactly Host, Authorization: Basic and Accept: application/json.
  - GitHub request: POST /api/graphql, with exactly Host, Bearer, User-Agent AiPet/1.0, Content-Type application/json; charset=utf-8 and Content-Length. The poll, lookup and Test bodies match .NET's System.Text.Json output byte for byte.
  - Error texts checked: success, 401, 403 (with and without errorMessages), rate limits (403 and 429), timeout ("didn't answer in time"), no network ("Couldn't reach …: <OS text> (127.0.0.1:port)", .NET's format), HTML or empty bodies, and the JsonElement errors.
  - Stub-server requests use `Http::local`, which refuses any host other than this computer. No test can reach the real Jira or GitHub, or read the user's config or tokens (MemorySecrets and a scratch folder).
- Minimum intervals: `interval()` gives max(30 s, PollSeconds) for Jira and max(60 s, PollSeconds) for GitHub; unit tests check it down to i32::MIN. Loop tests with PollSeconds=1 show the first poll only after 1.5 s (Jira) or 4 s (GitHub), and no second poll within 2.5 s after it.
- A new review alerts once: the first poll sends only Changed. A new key or URL sends NewReviews(n) once, and one seen before does not alert when it comes back. Save and ForgetToken reset GitHub's seen set; Save resets Jira's, as in the C#.

Choices to review:
- API: `JiraWatcher::new`/`GitHubWatcher::new(data_dir, Arc<dyn SecretStore>, Http, Sender<E: From<Event>>)`, plus `set_jira_keys`, `jira_changed` and `test`. `Http::new()` is HTTPS only: OS certificates and proxy, 20 s timeout, no ureq default headers, 50 redirects with the last answer kept. `Http::local(timeout)` is plain HTTP to this computer only. `poll()` is doc-hidden, for tests.
- .NET's own text after the C#'s prefix is reproduced for the common causes: an unreachable host or port, TLS, an empty or HTML body, an invalid URI host or port, the JsonElement type and key errors, and null references. Rarer JSON syntax errors keep serde_json's text.
- The reason phrase is the standard one; ureq drops the server's.
- A time without an offset reads as UTC; .NET uses local time.
- Key detection compares ASCII case-insensitively, with ASCII digits.
- On Windows, a failed Credential Manager write is ignored, like the C#. Elsewhere it comes back from `save`.
- The key cache uses a monotonic clock.
- C# nulls in Issue and PullRequest fields become "".
- `LastChecked` is not ported: nothing in the C# reads it.

baseline: red (cd rust && cargo test --workspace failed pre-edit on the inherited, timing-flaky aipet-ui test tests::a_poke_and_a_hop_are_smooth under load: it passed 3/3 alone and in the verify run). aipet-core, clippy -D warnings, fmt and `dotnet test` were green pre-edit.
Verify: both full gates green on 55b4f3c: the Rust chain (47 s) and `dotnet test` (160 passed, 42 skipped). Green receipts are in .flow/tmp/green-receipts/.
Follow-ups (outside Touches): rust/golden could get a `watchers` golden mode that replays the recorded .NET requests in CI. The recording harness was a scratch C# app. `sessions/describe.rs` and http.rs each carry .NET's simple_upper, and a later task could share it. Notes: NOTES_DIR/task-11-watchers.md.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

Review fix (9593292): Http::local refuses non-loopback hosts on every connection, redirects included; "Jira/GitHub returned ..." carries the server's own reason phrase; JSON syntax errors use .NET's Utf8JsonReader message and position, checked against 5,492 texts recorded from .NET 10.

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fixes 9593292)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 55b4f3c3c4737ab07c30b5188f9b4663342e99f1, 9593292
- Tests: cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check, dotnet test AiPet.slnx, cd rust && cargo clippy -p aipet-core --all-targets -- -D warnings, integrated verify (Windows, work branch 0a5517b with tasks 9 and 11 and their review fixes merged): cargo test --workspace -- --skip credential_manager_tokens_are_the_apps green, clippy --workspace --all-targets -D warnings and fmt --check clean; dotnet build AiPet.slnx -p:UseAppHost=false, dotnet test AiPet.slnx --no-build: 160 passed, 42 skipped; AIPET_TEST_HOOK=<Debug aipet-hook.dll> cargo test -p aipet-core --test server: 15 passed (the .NET hook against the Rust server); AIPET_TEST_HOOK=<rust hook> dotnet test --filter PluginHooksTests|RegistrationTests: 14 passed, 10 Unix-only skipped
- PRs:
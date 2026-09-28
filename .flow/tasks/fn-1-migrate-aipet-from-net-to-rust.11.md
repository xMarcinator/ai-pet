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
TBD

## Evidence
- Commits:
- Tests:
- PRs:

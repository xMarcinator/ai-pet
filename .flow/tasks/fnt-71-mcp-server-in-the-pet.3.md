---
satisfies: [R4, R5]
---
# fnt-71-mcp-server-in-the-pet.3 Claude status over HTTP hooks, ordered under parallel posts

## Description
Add `/hook/claude`: map Claude's hook body plus the two env headers into envelope v1 with receive time as `at`, push it on a bounded queue, answer 200 `{}`, and apply from one consumer through task 2's ingress handle (`AgentSessions::apply`, record and `changed()`, the same instances the pipe server uses). Move the exe's trimming to the server. Prove ordering with the C# goldens replayed under jitter.

**Size:** M
**Files:** `rust/crates/aipet-mcp/src/hook.rs` (new), `rust/crates/aipet-core/src/server/answer.rs`, `rust/crates/aipet-core/src/sessions/claude.rs`, `rust/crates/aipet-core/tests/**`
**Touches:** [rust/crates/aipet-mcp/**, rust/crates/aipet-core/src/server/**, rust/crates/aipet-core/src/sessions/**, rust/crates/aipet-core/tests/**]

### Approach
- Reuse `answer::event`'s apply-record-changed path (`aipet-core/src/server/answer.rs:22-80`); factor the part after parsing so both servers call it.
- Trimming: move `aipet-hook/src/event.rs`'s 256K cut, `tool_response` strip and `TOOL_INPUT_KEYS` fallback into shared code the hook route calls.
- Same Host/Origin checks as `/mcp`, no token (spec Decision Context).
- `transcript_path` used for titles only when under the Claude or Codex home (`aipet-ipc/src/paths.rs`).
- Ordering test: replay `aipet-core/tests/golden/sessions` and `golden/board` with each event's arrival shuffled within 500 ms and `at` = arrival; compare final states. Adjust `PAIR_WINDOW` (`sessions/claude.rs:300`) or stale rules only if the test demands it.

### Investigation targets
**Required:**
- `rust/crates/aipet-core/src/server/answer.rs:22-80`
- `rust/crates/aipet-core/src/sessions/mod.rs:120-250` - apply and the `at` fallback
- `rust/crates/aipet-core/src/sessions/claude.rs:255-340` - rank, stale, pairing
- `rust/crates/aipet-hook/src/event.rs:1-60` - trimming to move
**Optional:**
- `rust/crates/aipet-core/tests/server.rs:1-60` - the Env scratch pattern

### Key context
- From the spike (`rust/proofs/mcp-spike.md`): Claude doesn't run HTTP hooks on SessionStart. Keep the exe's command hook for that one event, or create the session from its first HTTP event; decide here and record it. `allowedEnvVars` headers arrive, and Stop's reply comes back before the turn ends.
- Return `{}` or empty: Claude treats a plain-text 2xx body as an error.
- R1's spike says whether Stop's HTTP hook completes before the turn ends; if not, the stale rule must protect "done".

## Acceptance
- [ ] A test posts every Claude event kind to `/hook/claude` and sees the session state the pipe path gives for the same input
- [ ] Malformed JSON and unknown events get 200 `{}` and a log line; a full queue drops with a log line
- [ ] The jittered golden replay reaches the C# goldens' final states, including PreToolUse/PermissionRequest pairs and "done" after Stop
- [ ] Entrypoint and host-session headers land in the envelope's `env`

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

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
Claude's status over HTTP hooks, on branch `fnt-71-mcp-server`.

- `/hook/claude` (aipet-mcp `hook.rs`): the body plus `X-AiPet-Entrypoint` / `X-AiPet-Host-Session` become envelope v1 as the exe made it (mended, no `tool_response`, strings cut to 256K, `tool_input` cut to its key fields past 4 MiB), with `env` from the headers (an unexpanded `${VAR}` is dropped), the arrival as `at`, no `pid`, and `"via":"http"`. A `transcript_path` outside `$CLAUDE_CONFIG_DIR`/`~/.claude` or Codex's home is removed. Always 200 `{}` (application/json); 403/413 from the guard as before.
- Queue (`events.rs`): bounded `sync_channel` (256 events, 16 MiB of lines) with `try_send`, one `aipet-mcp events` thread applying through the core's `Ingress` in arrival order (a panic is logged, the thread goes on). A full queue, or a body that isn't an event, is dropped with an `mcp: dropped an event: …` line, at most every 10 s with a count of the rest. An unknown event is applied and recorded `-> ignored`.
- Shared code: the exe's JSON tree and its trimming/envelope moved to `aipet_ipc::json` and `aipet_ipc::event` (`mended`, `trimmed`, `envelope`, `fit`, `READ_DEPTH`); the hook uses them unchanged in behaviour. New constants `CLAUDE_HOOK_PATH`, `CLAUDE_ENV_HEADERS`, `VIA`; `paths::claude_home()`.
- Ordering (R5): `at` = arrival can't tell a late event, so `claude::stale` gained one rule for `via:http` events only: within `STRAGGLERS` = 0.5 s of a turn's "done", or of the chat's end, an event that can't open a turn (anything but SessionStart, UserPromptSubmit, SessionEnd) is stale. The exe path and every golden are unchanged; PAIR_WINDOW unchanged.
- Jitter replay (`aipet-core/tests/sessions.rs`, `jitter`): the Claude golden's chats (random/*, ordering/*; 41 cases × 40 seeds) dispatched at their golden `at`, arriving up to 500 ms later (a turn's UserPromptSubmit/SessionStart stays after its chat's earlier events), applied with `at`=`now`=arrival and `via:http`. Chats and Board cards must equal the in-order replay, whose states equal the golden's last snapshot (fully, where the golden's steps are in dispatch order). Without the straggler rule 12 cases fail. Left out with reasons in the test: chats with a clock set back, the claude/* tables (single events 10 ms apart), and 4 ordering tests that pin sub-500 ms orders Claude itself keeps over HTTP by waiting for each hook's answer.

SessionStart decision: the pet creates the chat from its first HTTP event; no command hook stays. What SessionStart contributes (`claude.rs`): a new chat shown idle/"Ready", and a done chat reset to idle on a `compact` SessionStart. Idle chats aren't on the Board; the transcript title, cwd, where and host id come with every event; /clear and resume start from the next prompt (a new session id, or a chat reopened after its tombstone). The losses: the pet doesn't wake from sleep to idle when a session merely opens, and a manual /compact shows "Compacted" (done) for its 20 s instead of going idle at once. Task 7 registers HTTP hooks for the other 16 events of `aipet-hook/src/claude.rs`'s list (matcher `*` on PreToolUse, PostToolUse, PostToolUseFailure, PermissionRequest), URL `http://127.0.0.1:47615` + `CLAUDE_HOOK_PATH`, timeout 2, the two `CLAUDE_ENV_HEADERS` with `allowedEnvVars`, never `async`, and removes the exe's SessionStart registration with the rest.

For task 4: build the Codex envelope with `aipet_ipc::event::envelope("codex", SystemTime::now(), None, None, trimmed(payload))`, push `"via":"http"` if Codex should get arrival-time rules (Codex's `claude::stale`/`end` calls pass `false` today), and queue it with `state.events.push(envelope)`; `drop_event` logs a refusal. `report_event`'s `isError` for an invalid event must be decided before queueing (the queue applies asynchronously).

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 2cd4321e376a9f3e58cb9682a59bbc20b24bd9af, 9bbf3a48c06d13f8374f0d265edcda43fb1701ad, 0d5b3b2fd529bbbbd54b39095fc978a05acf7fe4
- Tests: cd rust && cargo test --offline --workspace
- PRs:
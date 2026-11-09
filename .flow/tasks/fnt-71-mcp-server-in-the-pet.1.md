---
satisfies: [R1]
---
# fnt-71-mcp-server-in-the-pet.1 Spike: Codex mcp_tool hooks and Claude HTTP hooks on real installs

## Description
Answer R1's questions with a throwaway server before anything is built on them (the spec's early proof point). Use a tiny script server (Python's http.server or a scratch rmcp example outside the workspace) that logs every request, and a scratch Codex and Claude config pointing at it. Record answers, versions of Codex and Claude Code, and the exact working hook JSON in a proof file.

**Size:** M
**Files:** `rust/proofs/mcp-spike.md` (new)
**Touches:** [rust/proofs/mcp-spike.md]

### Approach
- Codex: one `[mcp_servers.aipet-spike]` with `http_headers_helper`, and one `mcp_tool` hook per event whose `input` templates the payload fields AgentSessions needs (`session_id`, `turn_id`, `hook_event_name`, `tool_name`, `tool_use_id`, `tool_input`). Try the template forms the Claude docs use (`${tool_input.file_path}`) and a whole-payload form.
- Codex: start Codex with the server down, then start it, and see whether hooks reach it. Put the report tool in `disabled_tools` and see whether hooks still call it.
- Claude: an `http` hook for every event the exe registers today (`aipet-hook/src/claude.rs:37-56`), with `allowedEnvVars` for `CLAUDE_CODE_ENTRYPOINT` and `CLAUDE_CODE_HOST_SESSION_ID`, `timeout: 2`. Log arrival order and whether Stop arrives before the turn ends.
- Run on Linux. Windows Codex needs the owner present: record it as a hands-on item for task 8 if not done here.

### Investigation targets
**Required:**
- `rust/crates/aipet-hook/src/codex.rs:5-83` - today's Codex events, Windows list, trust keys
- `rust/crates/aipet-hook/src/event.rs:1-60` - the envelope fields the pet needs
- `rust/crates/aipet-core/src/sessions/turns.rs:1-110` - what Codex ordering reads

### Key context
- Never touch the owner's real `~/.codex` or `~/.claude`: use `CODEX_HOME` and `CLAUDE_CONFIG_DIR` scratch folders. Ask before downloading anything.
- A "no" on payload fields triggers R6's fallback; write that verdict at the top of the proof.

## Acceptance
- [ ] `rust/proofs/mcp-spike.md` answers each R1 question yes/no with the evidence (request logs) and the tool versions
- [ ] It contains the working Codex `mcp_tool` hook JSON and Claude `http` hook JSON, or the reason none works
- [ ] Its verdict line says whether task 4 builds `mcp_tool` hooks or the exe fallback

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

# Spike: Codex `mcp_tool` hooks and Claude HTTP hooks (fnt-71 task 1, R1)

**Verdict: task 4 builds the Codex `mcp_tool` hooks (R6's main path, not the exe fallback).** Codex templates every
payload field AgentSessions needs (`session_id`, `turn_id`, `hook_event_name`, `tool_name`, `tool_use_id`,
`tool_input`) into the `report_event` call, with `tool_input` kept as an object. Four limits go into the docs and the
generator (below): one placeholder set per event, no SessionEnd, `report_event` can't be hidden from the model, and no
reconnect after the pet was down at thread start. Claude's HTTP hooks replace the exe for every event except
**SessionStart, which Claude does not run as an HTTP hook**.

Run on Linux (Arch, 2026-11-09) with **Codex CLI 0.166.0** and **Claude Code 2.1.316**. Windows Codex was not run:
it is a hands-on item for task 8.

## Setup

- A Python stub (`http.server`) on `127.0.0.1:47801` logged every request. It answered MCP Streamable HTTP on `/mcp`
  (initialize, tools/list with `report_event`, tools/call) and `/hook/claude` with `200 {}`. It could delay the
  reply to Stop.
- Nothing in `~/.codex` or `~/.claude` was changed (their mtimes are unchanged), and no credentials were copied.
  Codex got `-c` overrides; Claude got `--settings <file>` and `--strict-mcp-config`.
- Codex trust: hooks from `-c` have the key `/<session-flags>/config.toml:<event>:<group>:<index>` and are
  untrusted. App-server `hooks/list` reports each key and its `currentHash`. Passing them back as
  `-c 'hooks.state={"<key>"={trusted_hash="<hash>"}, ...}'` made them trusted for that run. The state merges with
  the user's state, so the user's existing hooks stayed trusted. `--dangerously-bypass-hook-trust` was not used.
- `codex exec --json` shows no hook runs, and Codex's log database stayed empty. Codex runs 3-5 therefore drove one
  turn through `codex app-server` (thread/start, then turn/start), whose `hook/started` and `hook/completed`
  notifications carry each run's status and error text.
- Hooks set with `-c` are **added** to the user's hooks. They don't replace them. The user's AiPet command hooks
  also ran; the pet was not running.

## R1 answers

| Question | Answer | Evidence |
|---|---|---|
| Codex `mcp_tool` hooks pass session, turn, event, tool and tool-use fields into the call | **Yes** | Run 4 tools/call: `{"agent":"codex","event":{"session_id":"01a1c21e-1dd2-…","hook_event_name":"PreToolUse","turn_id":"01a1c21e-1e08-…","tool_name":"Bash","tool_use_id":"exec-c1d02ea0-…","tool_input":{"command":"cat word.txt"},"tool_input_command":"cat word.txt","mixed":"sid=01a1c21e-… tool=Bash"}}`. SessionStart, UserPromptSubmit, PreToolUse, PostToolUse and Stop all arrived in order and completed in 1-41 ms. |
| Codex reconnects to an MCP server that was down at its start | **No** (within a thread) | Run 5: thread started with the server down, so `mcpServer/startupStatus` was `failed` ("handshaking with MCP server failed"). The server came up 4.8 s later, and the turn ran 10 s after that with hooks firing. The server got **zero** requests. |
| A tool hidden from the model (`disabled_tools`) still runs from a hook | **No** | Run 5 with `disabled_tools=["report_event"]`: every hook failed with `tool 'report_event' is disabled for MCP server 'aipet-spike'`. Run 1 (exec, also disabled) had no tools/call either. |
| Claude's HTTP hook reaches the pet on SessionStart | **No** | Run 3 debug log: `Skipping HTTP hook http://127.0.0.1:47801/hook/claude — HTTP hooks are not supported for SessionStart`. No SessionStart request arrived in runs 2 or 3. |
| `allowedEnvVars` headers arrive | **Yes** | Every Claude request carried `X-AiPet-Entrypoint: claude-desktop` and `X-AiPet-Host-Session: spike-host-1` (the launching shell's values). |
| Stop's HTTP hook returns before the turn ends | **Yes** (Claude waits for it) | Run 2: the stub held Stop for 1.5 s. Stop arrived at …361.90 and was answered at …363.40. SessionEnd came at …363.62 and the process exited at …364.31. The turn ended only after Stop's reply. |

## Codex findings for task 4

- `input` must be a TOML table. A string such as `input="${tool_input}"` is a config error.
- `${field}` and `${a.b}` resolve from the hook payload. A whole-string placeholder keeps the value's JSON type
  (`tool_input` arrives as an object). Placeholders also work inside text (`"sid=${session_id}"`).
- **A placeholder missing from that event's payload fails the whole hook** (`hook input placeholder `${turn_id}` was
  not found` on SessionStart, `${source}` on PreToolUse). So the generator writes one input per event with only the
  fields that event has. There is no whole-payload placeholder: `${payload}` was not found.
- `SessionEnd MCP hooks are not supported` (warning, hook skipped). The Interrupt timeout is clamped to 3 s.
- `mcp_tool` hooks run `sync`, in 1-41 ms against the stub. `transcript_path` was `null` in these ephemeral threads.
- `report_event` must stay visible to the model, so its description has to tell the model not to call it.
- `http_headers_helper` is a string path to a command. Its stdout JSON object became headers on every request,
  including the bearer.
- Not exercised: PermissionRequest (no approvals in these runs), the compaction and sub-agent events, Interrupt, and
  Windows.

## Codex hook config that worked (run 4)

```toml
[mcp_servers.aipet-spike]
url = "http://127.0.0.1:47801/mcp"
http_headers_helper = "/path/to/helper.sh"   # prints {"Authorization":"Bearer …"}

[[hooks.PreToolUse]]
[[hooks.PreToolUse.hooks]]
type = "mcp_tool"
server = "aipet-spike"
tool = "report_event"
timeout = 5
input = { agent = "codex", event = { session_id = "${session_id}", hook_event_name = "${hook_event_name}", cwd = "${cwd}", model = "${model}", transcript_path = "${transcript_path}", turn_id = "${turn_id}", tool_name = "${tool_name}", tool_use_id = "${tool_use_id}", tool_input = "${tool_input}" } }
```

Each event's `event` table:

| Event | Fields |
|---|---|
| SessionStart | base fields + `source` |
| UserPromptSubmit | base fields + `turn_id`, `prompt` |
| PreToolUse, PermissionRequest, PostToolUse | base fields + `turn_id` and the tool fields |
| Stop | base fields + `turn_id` |
| PreCompact, PostCompact, SubagentStart, SubagentStop, Interrupt | base fields only (no field beyond them tried) |

The base fields are `session_id`, `hook_event_name`, `cwd`, `model` and `transcript_path`.

## Claude HTTP hook JSON that worked (runs 2-3)

Each of the 17 events in `aipet-hook/src/claude.rs` used this handler, with `"matcher": "*"` on the tool events:

```json
{"type": "http", "url": "http://127.0.0.1:47801/hook/claude", "timeout": 2,
 "headers": {"X-AiPet-Entrypoint": "${CLAUDE_CODE_ENTRYPOINT}", "X-AiPet-Host-Session": "${CLAUDE_CODE_HOST_SESSION_ID}"},
 "allowedEnvVars": ["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID"]}
```

UserPromptSubmit, PreToolUse, PostToolUse, Stop and SessionEnd arrived as Claude's hook input, unchanged. PostToolUse
included `tool_response`. **SessionStart needs another route**, which task 3 or 7 decides: keep the exe's command hook
for SessionStart only, or let the pet create the session from its first event.

## Runs

There were five Codex runs: two through `codex exec` and three through app-server. Codex run 3 found the
missing-placeholder rule. There were three Claude runs: run 1 failed on its arguments before any model call, because
`--mcp-config` consumed the prompt. Scratch files are under the session scratchpad's `mcp-spike/` folder.

---
satisfies: [R6]
---
# fnt-71-mcp-server-in-the-pet.4 Codex status over mcp_tool hooks (or the exe fallback)

## Description
Build what task 1's verdict says. Normally: a `report_event` tool that maps the templated Codex fields into envelope v1 and feeds task 3's queue and consumer, and the generated Codex hook file of `mcp_tool` hooks for the full event list on every OS. Fallback: keep `codex.json`'s command hooks and record it in the spec.

**Size:** M
**Files:** `rust/crates/aipet-mcp/src/report.rs` (new), `rust/crates/aipet-hook/src/codex.rs`, `rust/crates/aipet-hook/src/plugin_hooks.rs`, `plugins/aipet/hooks/codex.json`, `plugins/aipet/.codex-plugin/plugin.json`
**Touches:** [rust/crates/aipet-mcp/**, rust/crates/aipet-hook/src/codex.rs, rust/crates/aipet-hook/src/plugin_hooks.rs, plugins/aipet/hooks/codex.json, plugins/aipet/.codex-plugin/plugin.json]

### Approach
- Hook JSON exactly as `rust/proofs/mcp-spike.md` records it works.
- `report_event` validates the agent and event name against the Codex event list and rejects anything else with `isError`; it is hidden from the model the way the spike showed works (`disabled_tools` or the plugin's tool policy).
- Codex ordering keeps using turn and tool ids (`sessions/turns.rs`); `at` is receive time.
- The generator (`plugin_hooks.rs`) emits the new `codex.json` so `scripts/check-plugin.sh --hook` stays byte-exact.

### Investigation targets
**Required:**
- `rust/proofs/mcp-spike.md` - the verdict and working JSON
- `rust/crates/aipet-hook/src/codex.rs:5-83,303-322` - event lists and the frozen strings this replaces
- `rust/crates/aipet-hook/src/plugin_hooks.rs:1-30`
**Optional:**
- `rust/crates/aipet-core/src/sessions/turns.rs:1-110`

### Key context
- From the spike (`rust/proofs/mcp-spike.md`): build the `mcp_tool` hooks (not the exe fallback). A placeholder missing from an event's payload fails that hook, so generate one input table per event (the proof lists the fields); there is no whole-payload placeholder; Codex skips MCP hooks on SessionEnd; `report_event` can't be in `disabled_tools` (hooks then fail), so it stays visible to the model; and a thread started while the pet was down never reconnects, so its hooks are lost until the next thread (document it).
- Never rename `aipet`: the Codex trust keys contain `aipet@aipet`.

## Acceptance
- [ ] `report_event` is in task 2's conformance table
- [ ] A test calls `report_event` with each Codex event and sees the same session state the pipe path gives
- [ ] An invalid agent or event is `isError` and changes nothing
- [ ] `--print-plugin-hooks` matches the committed `plugins/aipet/hooks/codex.json` byte for byte
- [ ] Or, on the fallback verdict, the spec and this task record why Codex keeps the exe

## Done summary
Codex's status over `mcp_tool` hooks: the `report_event` tool, on branch `fnt-71-mcp-server` (R6's main path, per the spike's verdict; no exe fallback).

- `report_event` (aipet-mcp `report.rs`, `tools.rs`): `{agent: "codex", event: {...}}` becomes the envelope (v1) the exe made of Codex's input: no `tool_response`, strings cut to 256K, no `env`, no `pid`, the arrival as `at`, `"via":"http"`. Queued with `Events::push`, applied in arrival order through the core's `Ingress`. Answers `{}` (structuredContent plus the text block).
- Refused before queueing, as `isError` with the reason and an `mcp: dropped an event: report_event refused it: …` line: an agent other than `codex`, a `hook_event_name` Codex's chats don't take (`aipet_core::sessions::codex_takes`: the chats' 11 events plus SessionEnd), no or empty `session_id`, an event too deep for the C#'s reader. rmcp itself answers `isError` (not a JSON-RPC error) for arguments it can't read (an event that isn't an object, no event). A full queue is `isError` too.
- A `transcript_path` outside Claude's or Codex's folders is dropped (`hook::agent_file`, shared with `/hook/claude`): the tool is the model's to call too.
- Visible to the model (Codex refuses a hook's call to a disabled tool); its description tells the model not to call it. Annotations: not read-only, not destructive, not idempotent.
- Ordering: Codex waits for each `mcp_tool` hook (sync) and its chats order by turn and tool ids first, so `via:http` is only a marker for Codex: `codex::Event` doesn't take Claude's straggler rule.
- Arguments arrive as rmcp's serde_json map, so their keys come sorted (no `preserve_order`); the pet's JSON tree is parsed from that text. That only changes the call key of tool inputs without command/file_path, consistently for every event of the same path, so PreToolUse and PermissionRequest still pair.
- Tests: conformance row (`every_tool_conforms`, which now accepts an output schema without properties for `{}`); `every_codex_event_over_report_event_gives_the_pipes_state` (15 calls covering all 12 events, with each event's spike input fields, outcome and chat equal to the pipe's); `an_invalid_agent_or_event_is_an_error_and_changes_nothing`; unit tests in `report.rs`.

Not in this task, by the caller's direction: the plugin's `codex.json`, `plugin_hooks.rs` and `--install` (task 7). `--print-plugin-hooks claude|codex` still matches the committed hook files byte for byte.

For task 7, the Codex hooks: server `aipet`, tool `report_event`, `type = "mcp_tool"`, timeout 5 (Interrupt is clamped to 3), one `input` table per event with only the fields that event has. Base fields: session_id, hook_event_name, cwd, model, transcript_path. SessionStart: + source. UserPromptSubmit: + turn_id, prompt. PreToolUse, PostToolUse: + turn_id, tool_name, tool_use_id, tool_input. PermissionRequest: + turn_id, tool_name, tool_input (tool_use_id not verified). Stop: + turn_id. PreCompact, PostCompact, SubagentStart, SubagentStop, Interrupt: base only (the spike ran none of these, nor PermissionRequest). The chats would also read `trigger` (PostCompact: a manual /compact shows "Compacted"), `agent_type` (SubagentStart: "Delegating to …") and `agent_id` (a helper's events); all are unverified in Codex's payloads, and a missing one fails the hook, so check them on a real Codex before adding. No SessionEnd (Codex skips MCP hooks on it). Same list on Windows and Linux. Limits for the docs (task 8): a thread started while the pet was down never reconnects; `report_event` is visible to the model.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: b12610ea5f50c6ecc32bafef077e0a0ff57850fc
- Tests: cd rust && cargo test --offline --workspace
- PRs:
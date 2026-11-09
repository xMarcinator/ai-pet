---
satisfies: [R6]
---
# fnt-71-mcp-server-in-the-pet.4 Codex status over mcp_tool hooks (or the exe fallback)

## Description
Build what task 1's verdict says. Normally: a `report_event` tool that maps the templated Codex fields into envelope v1 and feeds the same queue as task 3, and the generated Codex hook file of `mcp_tool` hooks for the full event list on every OS. Fallback: keep `codex.json`'s command hooks and record it in the spec.

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
- Never rename `aipet`: the Codex trust keys contain `aipet@aipet`.

## Acceptance
- [ ] A test calls `report_event` with each Codex event and sees the same session state the pipe path gives
- [ ] An invalid agent or event is `isError` and changes nothing
- [ ] `--print-plugin-hooks` matches the committed `plugins/aipet/hooks/codex.json` byte for byte
- [ ] Or, on the fallback verdict, the spec and this task record why Codex keeps the exe

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

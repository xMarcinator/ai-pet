---
satisfies: [R12]
---
# fnt-71-mcp-server-in-the-pet.7 Install, uninstall, doctor and the plugin for the new hooks

## Description
Move `--install`, `--uninstall` and `--doctor` into `AiPet` and make them register the HTTP hooks and the MCP server (Claude) and the `mcp_tool` hooks plus `[mcp_servers.aipet]` (Codex), removing the exe registrations they made before. Ship the same in the `aipet` plugin with a header-helper for the token. Keep the exe and pipe server working for registrations not yet migrated.

**Size:** M
**Files:** `rust/crates/aipet-hook/src/{claude,codex,plugin_hooks}.rs`, `rust/crates/aipet/src/main.rs`, `plugins/aipet/**`, `scripts/check-plugin.sh`, `install.sh`, `install.ps1`, `rust/crates/aipet-core/src/cleanup.rs`, `rust/crates/aipet/src/updates.rs`
**Touches:** [rust/crates/aipet-hook/src/**, rust/crates/aipet/src/**, plugins/aipet/**, scripts/check-plugin.sh, install.sh, install.ps1, rust/crates/aipet-core/src/cleanup.rs]

### Approach
- Follow the existing install code (`aipet-hook/src/claude.rs:208-262`, `codex.rs` BEGIN/END block) and its golden tests (`aipet-hook/tests/golden/{registration,codex,doctor}`).
- Claude: `http` hooks with `timeout: 2`, `allowedEnvVars` for the two env vars, never `onFailure: block`; MCP entry `{type: http, url, headersHelper}`. Codex: branch on the verdict in `rust/proofs/mcp-spike.md`. On success, install task 4's `mcp_tool` hooks and remove the exe's command hooks. On the fallback, keep registering the exe's command hooks unchanged (frozen strings, no re-trust). Either way add `[mcp_servers.aipet]` with `http_headers_helper` for the tools.
- Port and URLs come from `aipet_ipc::protocol::MCP_PORT` only.
- The header helper is `AiPet --print-token-header` (reads the token file, prints the header, exits 0 even when the pet is down).
- Doctor asks `status` over HTTP and reports listener, token, registrations, and untrusted Codex hooks (warning).
- Uninstall cleanup in `cleanup.rs` and `updates.rs` removes both kinds.

### Investigation targets
**Required:**
- `rust/crates/aipet-hook/src/claude.rs:208-262`, `codex.rs:52-83,303-322`
- `rust/crates/aipet-hook/tests/registration.rs`, `tests/codex.rs`, `tests/doctor.rs`
- `plugins/aipet/.claude-plugin/plugin.json`, `plugins/aipet/.codex-plugin/plugin.json`
- `scripts/check-plugin.sh`
**Optional:**
- `install.sh:86-170`, `install.ps1:106-262`

### Key context
- fn-1.23 owns the release workflow; this task runs after it and only changes what it ships.
- On the success verdict Codex users re-trust once: the doctor must say so plainly.

## Acceptance
- [ ] Golden tests: install on a settings file with old exe hooks leaves only the new hooks and MCP entry; uninstall removes both kinds; invalid JSON or TOML is untouched with a message
- [ ] `scripts/check-plugin.sh --hook` passes against the generator
- [ ] `--doctor claude|codex` reports a running and a stopped pet correctly, and untrusted Codex hooks as a warning
- [ ] Hands-on on Linux: a real Claude Code session and a real Codex session light the pet through the new hooks

## Done summary
AiPet --install/--uninstall/--doctor claude|codex and --print-token-header, on aipet-hook made a library. Claude: HTTP hooks for 16 events (no SessionStart, per task 3) in settings.json, mcpServers.aipet (http, headersHelper = AiPet --print-token-header) in .claude.json. Codex: [mcp_servers.aipet] (http_headers_helper = <data>/mcp-header script) and mcp_tool report_event hooks for 11 events in config.toml. Old exe hooks removed by either install; uninstall removes both kinds; invalid JSON/TOML untouched with a message (tests/agent_install.rs). The C# goldens still pass: aipet-hook's own --install keeps the C# behaviour for one release, its --uninstall also drops the new kinds (the uninstaller's cleanup finds registrations naming AiPet or the helper). Doctor asks status over HTTP with the token; Codex trust via codex app-server is a warning. Plugin: HTTP hooks, mcp_tool hooks, .mcp.json and hooks/codex-mcp.json (header via the bundled aipet-hook), checked by check-plugin.sh against the generator. install.sh/install.ps1 use AiPet. NOT done here: the hands-on with a real Claude Code and Codex session lighting the pet (needs the owner's screen) moves to task 8's hands-on pass; install.ps1 and the Windows paths are untested (FNT-76); the Codex plugin's MCP file format and ${PLUGIN_ROOT} in http_headers_helper are unverified on a real Codex.

stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: bd8653f
- Tests: cargo test -p aipet-hook (goldens), cargo test -p aipet --test agent_install, scripts/check-plugin.sh --hook aipet-hook
- PRs:
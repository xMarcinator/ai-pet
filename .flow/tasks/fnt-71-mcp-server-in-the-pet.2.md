---
satisfies: [R2, R3, R11, R13]
---
# fnt-71-mcp-server-in-the-pet.2 Listener thread, token, both MCP eras and the status tool, measured against R18

## Description
Add the pet's HTTP listener: a new crate with its own thread and current-thread tokio runtime, rmcp's Streamable HTTP service at `/mcp` with Origin, Host and bearer checks, the token file, request limits, and the first tool (`status`). Measure calm CPU and RSS with it on before any further tool is built (second proof point).

**Size:** M
**Files:** `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/crates/aipet-mcp/**` (new), `rust/crates/aipet/src/main.rs`, `rust/crates/aipet-ipc/src/paths.rs`
**Touches:** [rust/Cargo.toml, rust/Cargo.lock, rust/crates/aipet-mcp/**, rust/crates/aipet/src/main.rs, rust/crates/aipet-ipc/src/paths.rs]

### Approach
- Declare rmcp 3.9.0 (`server`, `macros`, `transport-streamable-http-server`), axum 0.8 and tokio `net` in `[workspace.dependencies]`, as the repo does for every dep.
- Wire as rmcp's `examples/servers/src/counter_streamhttp.rs` and `simple_auth_streamhttp.rs` (tag rmcp-v3.9.0): `nest_service("/mcp", svc)` plus a `from_fn` bearer layer. Set `allowed_origins`/`enforce_origin_validation()`, `max_request_body_bytes` = `MAX_REQUEST` (`aipet-ipc/src/protocol.rs`), `json_response = true`, keep `legacy_session_mode`, `LocalSessionManager` with idle reaping. tower `ConcurrencyLimit(16)`.
- Token: 32 random bytes hex in `paths::data_dir()/mcp-token`, 0600 on Unix, user-only by default on Windows LocalAppData. Constant-time compare.
- Start the listener from `main.rs` beside the pipe server; port in use -> one log line, pet carries on. `AIPET_PORT` overrides the port for tests only.
- `status` reads the record lines the pipe server's ping returns (`aipet-core/src/server/record.rs`).
- Measure with `scripts/perf/pet-calm.sh` (current build vs this branch, alternating) and `scripts/perf/memory.sh`; add the numbers to `rust/proofs/memory.md` in task 8, keep raw output here.

### Investigation targets
**Required:**
- `rust/crates/aipet-ui/src/executor.rs:1-31` - the one-worker executor this must not share
- `rust/crates/aipet/src/main.rs:137` - `cap_malloc_arenas` and startup order
- `rust/crates/aipet-core/src/server/mod.rs:1-160` - how the pipe server starts and logs
- `rust/crates/aipet-ipc/src/paths.rs:29-57` - data dir helpers
**Optional:**
- `rust/proofs/memory.md` - method and baselines

### Key context
- rmcp's default `allowed_origins` is empty, which disables the Origin check: set it.
- Requests without an Origin header (CLI agents) must pass.
- If fnt-70's hakari workspace-hack exists by then, regenerate it.

## Acceptance
- [ ] Tests: no token or a wrong token gets 401; a bad Origin gets 403; a non-loopback Host gets 403; a 5 MiB body gets 413
- [ ] Tests: a 2026-07-28 stateless `tools/call status` and a 2025-11-25 `initialize` + `Mcp-Session-Id` session both succeed; an unknown session id gets 404
- [ ] A second pet on the same port logs once and keeps running without the listener
- [ ] Calm CPU and RSS with the listener on and one MCP client connected are recorded against the build without it, and meet R18

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

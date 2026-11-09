---
satisfies: [R2, R3, R11, R13, R15]
---
# fnt-71-mcp-server-in-the-pet.2 Listener thread, token, both MCP eras and the status tool, measured against R18

## Description
Add the pet's HTTP listener: a new crate with its own thread and current-thread tokio runtime, rmcp's Streamable HTTP service at `/mcp` with Origin, Host and bearer checks, the token file, request limits, and the first tool (`status`). Measure calm CPU and RSS with it on before any further tool is built (second proof point).

**Size:** M
**Files:** `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/crates/aipet-mcp/**` (new), `rust/crates/aipet/Cargo.toml`, `rust/crates/aipet/src/main.rs`, `rust/crates/aipet/src/core_thread.rs`, `rust/crates/aipet-core/src/server/{mod,answer,record}.rs`, `rust/crates/aipet-ipc/src/{paths,protocol}.rs`
**Touches:** [rust/Cargo.toml, rust/Cargo.lock, rust/crates/aipet-mcp/**, rust/crates/aipet/Cargo.toml, rust/crates/aipet/src/main.rs, rust/crates/aipet/src/core_thread.rs, rust/crates/aipet-core/src/server/**, rust/crates/aipet-ipc/src/**]

### Approach
- Declare rmcp 3.9.0 (`server`, `macros`, `transport-streamable-http-server`), axum 0.8 and tokio `net` in `[workspace.dependencies]`, as the repo does for every dep.
- Wire as rmcp's `examples/servers/src/counter_streamhttp.rs` and `simple_auth_streamhttp.rs` (tag rmcp-v3.9.0): `nest_service("/mcp", svc)` plus a `from_fn` bearer layer. Set `allowed_origins`/`enforce_origin_validation()`, `max_request_body_bytes` = `MAX_REQUEST` (`aipet-ipc/src/protocol.rs`), `json_response = true`, keep `legacy_session_mode`, `LocalSessionManager` with idle reaping. tower `ConcurrencyLimit(16)`.
- Add `aipet-mcp` to `rust/crates/aipet/Cargo.toml` `[dependencies]`.
- Port: `pub const MCP_PORT: u16 = 47615` in `aipet-ipc/src/protocol.rs`, the one constant every later task uses.
- Ingress: today `core_thread::start` builds `AgentSessions` and the `changed` sender, and the record lives privately in the pipe server's `Shared` (`aipet-core/src/server/mod.rs:75-111,217`). Lift the record and the apply-record-changed step (`server/answer.rs:22-80`) into one public ingress handle that `core_thread::start` creates, and make the pipe server consume that handle instead of its own copy. Pass the same handle to the listener, so task 3's consumer and the pipe server feed one sessions instance and one record. Task 2 only wires it; `status` reads the record through it. Equivalence: `aipet-core/tests/server.rs` and the sessions/board goldens pass unchanged.
- Token: 32 random bytes hex in `paths::data_dir()/mcp-token`. Create it atomically (temp file in the same folder, then rename), never following a symlink at the target. On Unix set 0600 and check the owner on every start. On Windows set an explicit ACL that grants only the current user (inherited ACLs do not count when `AIPET_DATA_DIR` points somewhere permissive). A file that is unreadable, not 64 hex chars, or wrongly owned is replaced, with one log line. Constant-time compare. 401 logging at most once a minute, never the token itself.
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
- [ ] RSS with the listener on and an agent connected is at most 5 MB above the listener off, paired in one run; it settles back within 60 s after a cap-size hook body (R15)
- [ ] Tests: no token or a wrong token gets 401; a bad Origin gets 403; a non-loopback Host gets 403; a 5 MiB body gets 413
- [ ] Tests: a 2026-07-28 stateless `tools/call status` and a 2025-11-25 `initialize` + `Mcp-Session-Id` session both succeed; an unknown session id gets 404
- [ ] A second pet on the same port logs once and keeps running without the listener
- [ ] Tests: a corrupt token file and a wrongly permissioned one are replaced and logged; repeated bad-token requests log once per minute
- [ ] A table-driven MCP conformance test checks every registered tool's `outputSchema`, `structuredContent`, matching JSON text block, annotations and listing order, plus unknown-tool and malformed calls (JSON-RPC errors) against business failures (`isError`); tasks 4-6 add their tools to the table
- [ ] Calm CPU and RSS with the listener on and one MCP client connected are recorded against the build without it, and meet R18

## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

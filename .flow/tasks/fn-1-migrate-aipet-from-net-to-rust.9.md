---
satisfies: [R6, R7]
---
# fn-1-migrate-aipet-from-net-to-rust.9 Rust hook server for Unix and Windows, with cross-runtime tests

## Description
Port `HookServer`: the pet's listener with the C#'s trust boundary. It runs on threads, with no async runtime. Tests
run the Rust hook, and the C# hook, against it.

**Size:** M
**Files:** `rust/crates/aipet-core/src/server/{mod,unix,windows,answer,record}.rs`, `rust/crates/aipet-core/tests/server.rs`, `.github/workflows/cross-runtime.yml` (the reverse direction)
**Touches:** [rust/crates/aipet-core/src/server/**, rust/crates/aipet-core/tests/server.rs, .github/workflows/cross-runtime.yml]

### Approach
- Unix (`src/AiPet.Core/HookServer.cs:54-90, 354-400`):
  - `RefuseLiveServer` (connect first), then remove a stale socket file, `bind`, `chmod 0600`, and only then `listen`.
    Use `socket2` or `libc` to separate bind from listen; `std`'s `UnixListener::bind` also listens.
  - Accept on 8 threads.
  - Peer credentials: `SO_PEERCRED` through `libc` on Linux, `getpeereid` elsewhere. `std`'s `peer_cred` wasn't stable
    on Rust 1.89 (the workspace is on 1.98 now: check it before choosing).
- Windows (`:330-351`):
  - `CreateNamedPipeW` with a security descriptor (owner = the current user's SID, network denied),
    `FILE_FLAG_FIRST_PIPE_INSTANCE` on the first instance, and 8 blocking `ConnectNamedPipe` listeners.
  - Cut silent clients off with `CancelIoEx`.
- Shared:
  - A thread per connection, capped at 64. `Answer` (`:266-299`): version check, then ping or event, else refuse.
  - `Record`: timestamp format, the 30-line ring, `hook-events.log` halved at 64 KB (`:305-325`).
  - `Stop` wakes the listeners by connecting to them (`:92-116`).
- Tests: the Rust hook ↔ the Rust server end to end; floods; oversized and too-deep requests; wrong versions; silent
  clients; the socket mode; a stale socket; refusing a live server.
- Cross-runtime: add a job to `cross-runtime.yml` that runs the C# hook (`dotnet aipet-hook.dll`) against the Rust
  server. Task 2's hook is used for the Rust ↔ Rust tests.

### Investigation targets
**Required:**
- `src/AiPet.Core/HookServer.cs:1-406`
- `tests/AiPet.Tests/HookServerTests.cs`, `HookServerUnixTests.cs`, `StarvedPoolTests.cs` — the cases to port
- `docs/ARCHITECTURE.md` §6.5, §10 (threads)
**Optional:**
- `rust/notes/native.md` — the Windows security APIs from the spike

### Key context
- Don't use `PipeOptions.CurrentUserOnly`-style checks that compare the token owner. They break between elevated and
  normal processes of the same user (dotnet/runtime#123903).
**From task 7's review (2026-09-29):** `AgentSessions::apply` takes an `Envelope`: the parsed fields plus the raw
request line, which pairing needs to key a whole tool input as System.Text.Json writes it. The server calls
`Envelope::parse(line)` and passes the result to `apply`, not a parsed `Map`.

## Acceptance
- [ ] Rust tests cover every case of `HookServerTests`, `HookServerUnixTests` and `StarvedPoolTests`, on Linux, and on
      Windows where they apply.
- [ ] The C# hook works against the Rust server in CI.
- [ ] The socket is 0600 before `listen`, and a peer with another uid is refused (Linux test using a second uid where
      CI allows, else a unit test on the credential check).
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

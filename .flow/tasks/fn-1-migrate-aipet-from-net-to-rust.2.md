---
satisfies: [R1, R2, R7]
---
# fn-1-migrate-aipet-from-net-to-rust.2 Rust hook event path, trace log and panic safety, run against the C# black-box suites

## Description
The `aipet-hook` binary's event path (`--agent claude|codex`): read stdin, build the envelope, send, and exit 0
silently. This is the early proof point: the C#'s own black-box hook suites run against this binary talking to the
.NET pet's server.

**Size:** M
**Files:** `rust/crates/aipet-hook/src/{main,event,trace}.rs`, `rust/crates/aipet-hook/tests/event.rs`, `tests/AiPet.Tests/TestEnv.cs`, `.github/workflows/cross-runtime.yml` (new)
**Touches:** [rust/crates/aipet-hook/src/main.rs, rust/crates/aipet-hook/src/event.rs, rust/crates/aipet-hook/src/trace.rs, rust/crates/aipet-hook/tests/event.rs, tests/AiPet.Tests/TestEnv.cs, .github/workflows/cross-runtime.yml]

### Approach
- Port `Program.Main`, `ReadPayload`, `Mended`, `Trimmed`/`Cap`/`Cut`, `Request`, `Sent` and `Trace`
  (`src/AiPet.Hook/Program.cs:23-229`), and the Claude/Codex envelopes (`ClaudeHook.cs`, `CodexHook.cs`).
- Take `at` on the first line of `main`, before reading stdin. Read stdin on a thread with the 2 s deadline.
- Panic safety: install a silent panic hook, wrap the body in `catch_unwind`, and end with `process::exit(0)`. Never
  write to stdout or stderr on this path.
- The request-size fallback keeps the C#'s budget: the limit minus 32 bytes, with the 22-byte `sent` suffix. Measure the
  bytes the Rust serializer actually writes.
- Trace log (`Program.cs:200-220`), hardened beyond the C# on purpose:
  - The file is `$TMPDIR/aipet-hook-<uid>.log` on Unix, `%TMP%\aipet-hook.log` on Windows.
  - Open atomically with `O_NOFOLLOW | O_CREAT | O_APPEND`, mode 0600, so a symlink is refused by the open itself.
  - Then `fstat` the same descriptor: a regular file, owned by the effective uid, with mode exactly 0600. Also check
    that `/proc/self/fd/<n>` points at the expected name. Any mismatch: close, write nothing.
  - Rotation at 64 KB runs only after those checks pass. It unlinks only if the path's inode still equals the open
    file's (compare `lstat` with `fstat`), so a swapped path is never deleted.
- `TestEnv`: with `AIPET_TEST_HOOK=<path>` set, `StartHook` runs that binary directly instead of
  `dotnet hook/aipet-hook.dll` (`tests/AiPet.Tests/TestEnv.cs:60-117`).
- A new workflow, `cross-runtime.yml` (Linux and Windows), runs these classes with `AIPET_TEST_HOOK` pointing at the
  Rust hook: `HookContractTests`, `EndToEndTests`, `HookServerTests`, `NoPetTests`, `HookServerUnixTests` and
  `StarvedPoolTests`. Task 9 adds the reverse direction to the same file.

### Investigation targets
**Required:**
- `src/AiPet.Hook/Program.cs:1-229` — the event path and trace log
- `src/AiPet.Hook/ClaudeHook.cs`, `src/AiPet.Hook/CodexHook.cs` — envelopes
- `tests/AiPet.Tests/HookContractTests.cs` — the contract as tests
- `tests/AiPet.Tests/TestEnv.cs:60-117` — how the tests run the hook
**Optional:**
- `docs/ARCHITECTURE.md` §6.2 (the contract), §6.9 (logs)

### Key context
- System.Text.Json can parse lone escaped surrogates but can't round-trip them, which is why `Mended` exists.
  serde_json rejects them outright, so mend before parsing.
- A missing pet must leave no file behind. Don't touch the trace log until the pet has answered.
## Acceptance
- [ ] The six C# classes above pass with `AIPET_TEST_HOOK=<rust hook>` on Linux and Windows CI.
- [ ] Rust unit tests cover surrogate mending, string caps, `tool_input` trimming at the limit, the `sent` suffix, and
      the Claude env forwarding.
- [ ] Trace-log tests on Unix, each meaning no write: a symlink, a file owned by another uid (as root in a CI
      container, or with a fixture file owned by `nobody`), a foreign mode, a wrong fd target, and a path swapped
      between the check and the rotation. Rotation at 64 KB works on a valid file.
- [ ] A forced panic in the wrapped body still exits 0 with empty stdout and stderr (a unit test on the wrapper).
## Done summary
TBD

## Evidence
- Commits:
- Tests:
- PRs:

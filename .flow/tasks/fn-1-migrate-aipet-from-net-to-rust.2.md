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
Finished the Rust hook's event path from the paused WIP (3d914e6), which holds the event path, the hardened trace log, the panic wrapper and AIPET_TEST_HOOK in TestEnv. 8435fa6 adds `.github/workflows/cross-runtime.yml`. On Windows and Linux it builds the Rust hook and runs the C#'s eight event-path suites against it. It fails when nothing passed. On Linux it also runs the trace-log tests as root. The commit also gates a test import that was unused on macOS, and moves stdin decoding to `as_chunks`, which clippy 1.98 flagged.

This is the first Windows run outside compilation. The six spec classes, plus HookSocketTests and HookRequestSizeTests, pass against the Rust hook with the .NET HookServer: 19 passed, 11 skipped as Unix-only. A live-HookServer probe shows the trace log at `%TMP%\aipet-hook.log` in the C#'s line format. Linux and macOS weren't re-run here (no targets installed). The paused run verified Linux, and this run's only Unix-code change is the test import. `cross-runtime.yml` hasn't run in CI yet, so the AC's "Linux and Windows CI" waits on a push.

For the conductor:
- The WIP commit changed files outside the declared Touches: `rust/crates/aipet-hook/Cargo.toml`, the new `src/json.rs`, `aipet-ipc/src/endpoint.rs` (`user_name` made pub), and `tests/AiPet.Tests/HookContractTests.cs` (HookRun uses `HookStartInfo`). 8435fa6 stays inside the Touches.
- Evidence commits are the first-parent, non-merge commits since base 88b528f. That leaves out the merge b531bab and the work-branch commits it brought in.
- Local rust is 1.98.1, CI pins 1.89.0. Under 1.98.1, `clippy -D warnings` flags inherited code in aipet-sprite `avatar.rs` (4) and aipet-ui `sprite.rs` (1). That is a follow-up outside this task.
- The worktree's `avatars/hood-green.json` was checked out with CRLF before the merge brought `.gitattributes`. I re-checked it out as LF; there is no diff.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

stage: impl-review - ran (codex: NEEDS_WORK then SHIP; fix d19afd7)
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 3d914e692db654363b3861760b7496fd4d9f682d, 8435fa6fa238b5e0c10f32160c60a129e5aa7d2f, d19afd7
- Tests: baseline: green via handoff at 88b528f (paused run, Linux: cargo test/clippy -D warnings/fmt green; dotnet build then test --no-build 234 passed, 1 skipped), baseline (resume, Windows, HEAD b531bab): cargo test --workspace green after re-checking-out a stale CRLF avatars/hood-green.json (no diff); cargo fmt --check green; cargo clippy -D warnings red only for 3 lints new in local rust 1.98.1 (CI pins 1.89.0): aipet-sprite avatar.rs x4 and aipet-ui sprite.rs x1 (inherited), aipet-hook event.rs x1 (fixed in 8435fa6); dotnet test AiPet.slnx -p:UseAppHost=false: 160 passed, 42 skipped, 0 failed, cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check  (Windows, rust 1.98.1: rc 0, 108 tests passed, clippy warnings only in aipet-sprite/aipet-ui), cargo clippy -p aipet-hook -p aipet-ipc --all-targets --locked -- -D warnings  (Windows, rust 1.98.1: clean), dotnet test AiPet.slnx -p:UseAppHost=false  (Windows: 160 passed, 42 skipped, 0 failed), AIPET_TEST_HOOK=rust/target/debug/aipet-hook.exe dotnet test tests/AiPet.Tests --no-build --filter <HookContractTests|HookSocketTests|HookRequestSizeTests|EndToEndTests|HookServerTests|NoPetTests|HookServerUnixTests|StarvedPoolTests>  (Windows: 19 passed, 11 skipped as Unix-only, 0 failed; a missing AIPET_TEST_HOOK path fails with "the hook isn't built at"), scratch probe, .NET HookServer + both hooks with non-JSON and duplicate-member stdin (Windows): both exit 0 silently; aipet-hook.log in %TMP% has the C#'s line format (HH:mm:ss user= pipe=), messages worded by each runtime, cross-runtime.yml test step dry-run locally in bash against the debug hook: "passed: 19", step rc 0; the jq selection of the hook's bin test executable checked, not run locally: Linux/macOS (no Linux or Apple target installed); cross-runtime.yml has not run in CI (nothing pushed), integrated verify (Windows, work branch f91a758): cargo test --workspace, clippy --workspace --all-targets -D warnings, fmt --check green; dotnet build -p:UseAppHost=false + dotnet test AiPet.slnx: 160 passed, 42 skipped, 0 failed; AIPET_TEST_HOOK=rust/target/debug/aipet-hook.exe on the 8 cross-runtime classes: 19 passed, 11 skipped (Unix-only), 0 failed, still pending: Linux/macOS compile and run, and cross-runtime.yml in CI (the work branch isn't pushed)
- PRs:
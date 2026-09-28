# fn-1-migrate-aipet-from-net-to-rust.2 - PAUSED (work in progress)

Stopped on the conductor's pause request. Task status stays `in_progress`. Nothing reviewed, no `flowctl done`.

- Workspace: /home/marcinator/Documents/projects/ai-pet-waves/task-2, branch wave/fn-1.2
- Base commit: 88b528fb14c628ff0f13d0381fa8a0652ac63454 (also in the workspace's .flow/tmp/base_commit, which is gitignored)
- WIP commit: 3d914e6 ("wip: Rust hook event path, trace log and panic safety (paused)")

## Baseline (before any edit)
- `baseline: green via handoff (green (verified at 03b1404d by fn-1-migrate-aipet-from-net-to-rust.1))` for cargo test.
- cargo fmt --check and cargo clippy --workspace --all-targets --locked -D warnings: green.
- dotnet: a bare `dotnet test AiPet.slnx` fails 5 Resource/Velopack tests because it doesn't build AiPet.UI first
  (an inherited quirk of the Quick command, not a code failure). `dotnet build AiPet.slnx` then
  `dotnet test AiPet.slnx --no-build`: 234 passed, 1 skipped, 0 failed.

## Done (in the WIP commit)
- `rust/crates/aipet-hook/src/json.rs` (new): a JSON tree that keeps member order and number text, parsed with
  System.Text.Json's rules (depth 64, strict grammar, duplicate names allowed at parse time and detected when read).
- `src/event.rs`: the event path ported from Program.cs/ClaudeHook.cs/CodexHook.cs. Covers stdin on a thread within
  2 s; UTF-8/16/32 BOM decoding as StreamReader does; Mended; non-object -> {}; Trimmed/Cap/Cut (UTF-16 units, not
  splitting pairs); duplicate member names fail as the C# throws (`?` at top level); Request with the 32-byte `sent`
  room, measured in the bytes Rust writes; Sent; the budget; and the traces.
- `src/trace.rs`: the trace log. Unix: O_NOFOLLOW|O_NONBLOCK|O_CREAT|O_APPEND, 0600. Then fstat for a regular file,
  nlink==1, owner == euid and mode exactly 0600, plus the /proc/self/fd target (the canonical dir plus the name). It
  rotates past 64 KB only when the lstat dev/ino equals the fstat's, then makes a new file with O_EXCL.
  Windows: as the C# (TMP, then TEMP, rotation, append).
  Two guards go beyond the spec: nlink==1 (hard-link attack) and O_NONBLOCK (a FIFO would block the open).
- `src/main.rs`: `at` is taken on the first line of main. A silent panic hook, catch_unwind and process::exit(0), plus
  a compile_error if panic=abort.
- `aipet-ipc/src/endpoint.rs`: `user_name()` made `pub` (Windows) for the trace's `user=`.
- `aipet-hook/Cargo.toml`: windows-sys gains the `Win32_System_SystemInformation` feature (GetLocalTime). Cargo.lock
  is unchanged.
- `tests/AiPet.Tests/TestEnv.cs`: `TestHook` (AIPET_TEST_HOOK) and `HookStartInfo()`. StartHook uses it, and so does
  HookRun.Start in HookContractTests.cs.
- Tests: 23 unit tests (json, event, trace and the panic wrapper, including a subprocess check that a panic exits 0
  with empty stdout and stderr, mutation-checked). `tests/event.rs` has 2 integration tests: no pet leaves no trace,
  and a fake Unix pet gets the envelope.

## Verified locally (Linux)
- `cargo test -p aipet-hook`: 25/25 pass (23 unit + 2 integration). The root-only owner test skips unless euid 0.
- `cargo clippy --workspace --all-targets --locked -D warnings` and `cargo fmt --check`: clean (Linux).
- `cargo clippy -p aipet-hook -p aipet-ipc --all-targets --target x86_64-pc-windows-msvc -D warnings`: clean.
- The C# suites against the Rust release hook (AIPET_TEST_HOOK=rust/target/release/aipet-hook): all 32 tests pass.
  That is HookContractTests, HookSocketTests, HookRequestSizeTests, EndToEndTests, HookServerTests, NoPetTests,
  HookServerUnixTests and StarvedPoolTests. A missing AIPET_TEST_HOOK path fails loudly ("the hook isn't built at").
- A probe of 25 stdin edge cases against both hooks gave the same outcomes. Only string escapes differ: Rust writes
  non-ASCII as UTF-8 where STJ writes some as \uXXXX.

## Left to do
1. FAILING: `cargo clippy -p aipet-hook --all-targets --target aarch64-apple-darwin -D warnings` has an unused import
   in trace.rs's test module on macOS. `checked`/`opened` are only used by the Linux-only fd-target test, so gate that
   import with cfg(target_os = "linux"). CI's macOS job runs `cargo check --all-targets` without -D warnings, so it
   would only warn, but fix it.
2. NOT WRITTEN: `.github/workflows/cross-runtime.yml` (Linux + Windows):
   - Set up Rust 1.89.0 with rust-cache, and .NET 10.
   - On Windows, add a Defender exclusion for rust/target inside a try.
   - `cargo build -p aipet-hook --release --locked`, then `dotnet build AiPet.slnx -c Release`.
   - Run `dotnet test tests/AiPet.Tests -c Release --no-build` with the filter
     `FullyQualifiedName~AiPet.Tests.<Class>.|...` over the 8 classes above (the 6 in the spec plus HookSocketTests
     and HookRequestSizeTests), with `AIPET_TEST_HOOK=${{ github.workspace }}/rust/target/release/aipet-hook(.exe)`.
   - On Linux, run the trace tests as root: `cargo test -p aipet-hook --release --locked --no-run
     --message-format=json`, pick with jq the executable where kind==["bin"] and profile.test is true, then
     `sudo "$exe" trace:: --test-threads=1`. This runs `a_file_another_user_owns_is_left_alone`.
3. Verify block (`flowctl gate classify --base 88b528f...`, full gates, receipts), the evidence and summary handover
   finalisation, then the conductor's review.
4. Windows runtime behaviour (trace path/clock/user, the pipe) is only compile/clippy-checked. The Windows side of
   the AC ("pass on Linux and Windows CI") needs the new workflow to run once pushed.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate; task paused before Verify)

---
satisfies: [R1]
---
# fn-1-migrate-aipet-from-net-to-rust.1 Workspace foundations: the shared aipet-ipc crate and the crate and generator skeletons

## Description
Turn the spike workspace into the product workspace, and add `aipet-ipc`: the wire contract and data paths the hook and
the pet share. Also lay down the skeletons every later task fills in:
- `aipet-hook` and `aipet-core`, with every module stubbed and every planned dependency declared;
- the golden generator, split into one file per mode.

That way the hook, core and golden tasks each touch only their own files and can run in parallel. CI starts running the
Rust workspace here, so every later task is gated by it.

**Size:** M
**Files:** `rust/Cargo.toml`, `rust/crates/aipet-ipc/Cargo.toml`, `rust/crates/aipet-ipc/src/{lib,protocol,endpoint,paths,connect}.rs`, `rust/crates/aipet-hook/{Cargo.toml,src/*.rs}` (stubs), `rust/crates/aipet-core/{Cargo.toml,src/**}` (stubs), `rust/golden/{Program.cs,Golden.csproj,Sprite.cs}`, `.github/workflows/ci.yml`
**Touches:** [rust/Cargo.toml, rust/Cargo.lock, rust/crates/aipet-ipc/**, rust/crates/aipet-hook/Cargo.toml, rust/crates/aipet-hook/src/**, rust/crates/aipet-core/Cargo.toml, rust/crates/aipet-core/src/lib.rs, rust/golden/**, .github/workflows/ci.yml]

### Approach
- Skeletons, as empty modules with a doc comment each, declared in their crate root:
  - `aipet-hook`: `main`, `event`, `trace`, `install`, `claude`, `plugin_hooks`, `json_out`, `codex`, `toml_text`,
    `doctor`.
  - `aipet-core`: `sessions/{claude,codex,turns,describe,titles}`, `server/{unix,windows,answer,record}`,
    `secrets/{windows,linux,file}`, `config`, `log`, `cleanup`, `update_status`, `http`, `jira`, `github`,
    `presets`, `codex_watcher`, `board`.
  - Declare the dependencies these need now, so later tasks don't each rewrite the manifests and the lock file: `libc`,
    `socket2`, the `windows-sys` features, `serde`/`serde_json`, `ureq` with rustls, and `winresource` (or
    `embed-resource`) as the hook's build dependency. Tasks 6, 9 and 13 rely on this and don't touch `Cargo.lock`.
- Golden generator:
  - `Program.cs` becomes a dispatcher over modes: `sprite`, and stubs for `registration`, `doctor`, `sessions`,
    `data` and `board`. Each mode lives in its own file; the existing sprite code moves to `Sprite.cs`.
  - `Golden.csproj` links the hook sources the modes need, as the test project does:
    `src/AiPet.Hook/{Install,CodexConfig,PluginHooks}.cs` (`tests/AiPet.Tests/AiPet.Tests.csproj:33-36`).
  - Where a C# file doesn't compile outside its project (`Doctor.cs`), that mode runs the built C# hook as a
    subprocess instead.
- `protocol`: constants, field names, `ToolInputKeys`, `ClaudeEnv`, and the `UnixTime` format, from `src/AiPet.Core/Ipc.cs:27-50`.
- `endpoint`: the same name logic as `Ipc.Endpoint` (`Ipc.cs:53-58`), including `AIPET_PIPE`. Windows: the pipe name,
  with the session via `ProcessIdToSessionId`. Unix: the socket path, with the euid.
- `paths`: `DataDir`, `Config`, `Log`, `HookEventsLog` and `CodexHome` from `src/AiPet.Core/Paths.cs`, including
  `AIPET_DATA_DIR` and `CODEX_HOME`.
- `connect`: port `Ipc.Connect(busyMs, out busy)` (`Ipc.cs:60+`). On Windows, `WaitNamedPipeW` then `CreateFileW`. On
  Unix, run `connect()` on its own thread with a deadline, as the C# does, because it can block without limit. Also a
  line reader with the reply timeout, and `Ask`.
- Use `windows-sys` (already in the lock file at 0.52) and `libc`. No async runtime.
- CI: add a `rust` job (ubuntu-22.04 and windows-2022: `cargo fmt --check`, `clippy -D warnings`, `test`) and
  `cargo check` on macos-14. Put the cargo registry and target directory in `Swatinem/rust-cache`, after the toolchain
  step.

### Investigation targets
**Required:**
- `src/AiPet.Core/Ipc.cs:1-212` — the whole contract
- `src/AiPet.Core/Paths.cs` — data paths
- `docs/ARCHITECTURE.md` §6.5 (the socket) — the documented contract
**Optional:**
- `rust/Cargo.toml` — workspace and dependencies as the spike left them
- `.github/workflows/ci.yml:24-80` — the existing build job

### Key context
- `interprocess`'s Windows peer info is only a PID. The pet's server (task 9) does its own security, so this crate needs
  only client connects.
## Acceptance
- [ ] Unit tests: each constant equals the C#'s value.
- [ ] The endpoint and data paths equal the C#'s for the same environment on Linux and Windows, including the
      `AIPET_PIPE`, `AIPET_DATA_DIR` and `CODEX_HOME` overrides.
- [ ] Connecting to a missing, busy or silent endpoint returns within the C#'s budgets.
- [ ] The skeleton crates build. The golden generator's `sprite` mode still writes a byte-identical `frames.json`.
- [ ] The new CI job passes on Linux and Windows; the macOS check passes.
## Done summary
Added the aipet-ipc crate (Ipc.cs/Paths.cs port: protocol constants and .NET double format, endpoint with AIPET_PIPE, data paths and .NET special folders with AIPET_DATA_DIR/CODEX_HOME/XDG_DATA_HOME, and Windows/Unix connect with the C#'s deadlines plus Ask and line readers), stubbed aipet-hook and aipet-core crates with every planned dependency locked, split the golden generator into per-mode files (sprite unchanged byte for byte, a new ipc mode for the C# cross-check, stubs for the rest), and a Rust CI job (Linux/Windows fmt+clippy+test, macOS check).

Tests: constants are read from Ipc.cs's source; endpoint and paths are compared with the C# golden `ipc` mode across 14 Linux scenarios (also as uid 0 in a user namespace for the XDG_RUNTIME_DIR/data-folder branches) and 9 Windows scenarios in CI; missing/stale/busy/silent/foreign endpoints are tested against their budgets. Windows runtime behaviour (named pipes, short-name expansion, known folders) is compile- and clippy-checked locally but only runs in the new CI job, which has not run yet (nothing was pushed).

Follow-ups noted: ureq also carries platform-verifier and win-system-proxy so task 11 can match HttpClient's OS trust store and proxy without touching Cargo.lock; paths::home()/local_app_data() are public because the hook's registration and doctor read the same .NET special folders.

stage: impl-review - ran [2026-09-28..2026-09-28] codex SHIP on first pass
stage: plan-sync - skipped(config: planSync.enabled != true)
## Evidence
- Commits: 03b1404d0af2c262c39f6d65afb117c416b3b990
- Tests: baseline: green (cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check; dotnet test AiPet.slnx: 234 passed, 1 skipped), cd rust && cargo test --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check, dotnet test AiPet.slnx, AIPET_GOLDEN=rust/golden/bin/Release/net10.0/aipet-golden.dll cargo test --workspace --locked (C# cross-check of endpoint and paths), unshare -r env AIPET_GOLDEN=... <aipet-ipc test binary> (endpoint rules without /run/user/<uid>), cargo clippy --workspace --all-targets --locked -- -D warnings, cargo clippy -p aipet-ipc -p aipet-hook -p aipet-sprite -p aipet-ui -p aipet-desktop -p aipet-spike -p aipet-wayland --all-targets --target x86_64-pc-windows-msvc -- -D warnings, cargo check -p aipet-ipc -p aipet-hook --all-targets --target aarch64-apple-darwin, dotnet run --project rust/golden -c Release (sprite: frames.json sha256 d2eed203... unchanged, git diff empty)
- PRs:
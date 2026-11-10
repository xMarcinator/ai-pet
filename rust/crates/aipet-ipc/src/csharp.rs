//! The Rust held against the C#: every constant of the C#'s `AiPet.Core/Ipc.cs`, frozen here as the last C# had
//! them (commit bf3696ea4a, Ipc.cs:27-50), which the .NET pet and hook of earlier releases still speak.
//!
//! Until the cutover this read Ipc.cs itself, and also compared the endpoint, the data paths and the user's folders
//! with what the C# found in the same environments (the golden generator's `ipc` mode). The paths' rules are held by
//! endpoint.rs's and paths.rs's own tests.

use std::collections::HashMap;

use crate::protocol::*;

#[test]
fn every_constant_equals_ipc_cs() {
    let ms = |d: std::time::Duration| d.as_millis() as i64;

    // public const int Version = 1; MaxRequest = 4 << 20, TimeoutMs = 2000, BufferSize = 64 << 10;
    // ConnectMs = 2500, HookBudgetMs = 4500, StdinMs = 2000; MaxString = 256 << 10; MaxDepth = 128
    let csharp: HashMap<&str, i64> = [
        ("Version", 1),
        ("MaxRequest", 4 << 20),
        ("TimeoutMs", 2000),
        ("BufferSize", 64 << 10),
        ("ConnectMs", 2500),
        ("HookBudgetMs", 4500),
        ("StdinMs", 2000),
        ("MaxString", 256 << 10),
        ("MaxDepth", 128),
    ]
    .into();
    let rust: HashMap<&str, i64> = [
        ("Version", i64::from(VERSION)),
        ("MaxRequest", MAX_REQUEST as i64),
        ("TimeoutMs", ms(TIMEOUT)),
        ("BufferSize", BUFFER_SIZE as i64),
        ("ConnectMs", ms(CONNECT)),
        ("HookBudgetMs", ms(HOOK_BUDGET)),
        ("StdinMs", ms(STDIN)),
        ("MaxString", MAX_STRING as i64),
        ("MaxDepth", MAX_DEPTH as i64),
    ]
    .into();
    assert_eq!(rust, csharp);

    // public const string V = "v", Kind = "type", ...
    let csharp: HashMap<&str, &str> = [
        ("V", "v"),
        ("Kind", "type"),
        ("Agent", "agent"),
        ("At", "at"),
        ("Sent", "sent"),
        ("Pid", "pid"),
        ("Env", "env"),
        ("Payload", "payload"),
        ("Ok", "ok"),
        ("Outcome", "outcome"),
        ("Error", "error"),
        ("App", "app"),
        ("Recent", "recent"),
    ]
    .into();
    let rust: HashMap<&str, &str> = [
        ("V", V),
        ("Kind", KIND),
        ("Agent", AGENT),
        ("At", AT),
        ("Sent", SENT),
        ("Pid", PID),
        ("Env", ENV),
        ("Payload", PAYLOAD),
        ("Ok", OK),
        ("Outcome", OUTCOME),
        ("Error", ERROR),
        ("App", APP),
        ("Recent", RECENT),
    ]
    .into();
    assert_eq!(rust, csharp);

    // public static readonly string[] ToolInputKeys = { ... }, ClaudeEnv = { ... }
    assert_eq!(TOOL_INPUT_KEYS, ["file_path", "notebook_path", "skill", "command"]);
    assert_eq!(CLAUDE_ENV, ["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID"]);
}

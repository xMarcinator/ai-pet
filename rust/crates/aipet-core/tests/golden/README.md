# Golden corpora: frozen

These files are outputs of the C#'s own AiPet.Core, which the Rust core's tests replay byte for byte (and the
times bit for bit). The golden generator that wrote them, and the C# it ran, were removed at the cutover to Rust (spec
fn-1-migrate-aipet-from-net-to-rust, task 25). They can't be written again: they are the contract as it stands, and
nothing edits them by hand.

The C# is at commit `bf3696ea4a` (`git show bf3696ea4a:rust/golden/Program.cs` lists the generator's modes; the C#
projects are under `src/` there). Each file's `about` still says how it was written then.

| Folder | Written by | Replayed by |
|---|---|---|
| `sessions/` | the `sessions` mode: AgentSessions' outcomes, hook-events.log lines and snapshots | `tests/sessions.rs` |
| `board/` | the `board` mode: the Board's bubbles, and the Codex log watcher's polls | `tests/board.rs` |
| `data/` | the `data` mode: the data files read and written, secrets.json, the hook cleanup, the update texts, the log's time separators | `tests/data.rs`, `src/cleanup.rs` |

Until the cutover CI also had the C# read what the Rust writes (the data files, a log line, a Credential Manager
token): that direction has no corpus, and went with the C#.

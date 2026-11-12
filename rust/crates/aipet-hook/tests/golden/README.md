# Golden corpora: frozen

These files are outputs of the C# hook's own code, which the Rust hook's tests replay byte for byte. The golden
generator that wrote them, and the C# it ran, were removed at the cutover to Rust (spec
fn-1-migrate-aipet-from-net-to-rust, task 25). They can't be written again: they are the contract as it stands, and
nothing edits them by hand.

The C# is at commit `bf3696ea4a` (`git show bf3696ea4a:rust/golden/Program.cs` lists the generator's modes; the C#
projects are under `src/` there). Each file's `about` still says how it was written then.

| File | Written by | Replayed by |
|---|---|---|
| `registration/claude.json`, `registration/json.json` | the `registration` mode, on Windows | `tests/registration.rs`, `src/json_out.rs` |
| `codex/codex.json` | the `registration` mode, on Windows | `tests/codex.rs`, `src/codex.rs` |
| `doctor/doctor.json` | the `doctor` mode, through the built C# hook, on Windows | `tests/doctor.rs` |
| `registration/linux/claude.json`, `codex/linux/codex.json`, `doctor/linux/doctor.json` | the same modes on Linux, at the cutover (2026-11-10, .NET 10.0.114, from `bf3696ea4a`) | the same tests, on Linux |

Until the cutover CI also ran the generator on each runner's OS and replayed what it wrote; the Linux files hold
those runs now. Neither doctor corpus has the scenarios that write the organisation's managed Claude settings: the
C# ran them only on CI runners, live.

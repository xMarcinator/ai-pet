# Picking up flow-next on another machine (state of 2026-11-07)

The work branch is `fn-1-migrate-aipet-from-net-to-rust` (draft PR https://github.com/xMarcinator/ai-pet/pull/1).
Two flow-next specs are on it:

| Spec | State |
|---|---|
| `fn-1-migrate-aipet-from-net-to-rust`: the .NET → Rust migration | 24 of 30 tasks done; task 22 closed on Hyprland; next is task 28 (performance gate) |
| `fn-2-layered-sprite-and-vector-pets`: layered sprite, vector, Aseprite and Rive pets | 15 tasks, R1–R22; the spikes (13, 15) are done, task 14 is blocked (Rive deferred); work starts with task 1 |
| `fn-3-linux-desktops-beyond-hyprland-hands-on`: task 22's other desktops | 5 tasks, all todo; each needs a person at that desktop |

The overall picture, what's next and the decisions are in [docs/HANDOFF.md](../../docs/HANDOFF.md) under "Where the
work stands".

## 1. Restore flowctl's task state

flowctl keeps each task's live status in `.git/flow-state/`, which git never pushes. `state/` here is a copy of it
taken on 2026-11-07: the 24 done fn-1 tasks, fn-2's two done spikes and its blocked task 14, with their evidence.
In a fresh clone, on the work branch, copy it back before running flow-next.

In Git Bash, Linux or macOS:

```bash
mkdir -p .git/flow-state/tasks && cp .flow/paused/state/*.state.json .git/flow-state/tasks/
```

Check it: `flowctl show fn-1-migrate-aipet-from-net-to-rust.22` reads `done`, and `.28` reads `todo`. (flowctl is
bundled with the flow-next plugin: `~/.claude/plugins/cache/flow-next/flow-next/<version>/scripts/flowctl`. This
session used 9.5.1; fn-1's earlier tasks ran under 4.12.0 with the rolling-frontier work skill.)

## 2. What's next in fn-1

| Task | Needs |
|---|---|
| 22 Linux hands-on parity pass and the Hyprland layer rules | a person at a Linux machine (Hyprland); like task 27's Windows pass (`rust/proofs/windows-hands-on.md`) |
| 28 Performance gate | after 22 and 27; it also owns E-x9 (dropped frames while a stack spreads, from task 27) |
| 23 Ship the Rust pet (milestone 2) | after 22 and 28 |
| 24 Test migration and the parity ledger, then 25 Cutover, then 26 Docs and 29 Memory audit | in that order |

Run them with `/flow-next:work-rolling fn-1-migrate-aipet-from-net-to-rust` (or `/flow-next:work`). Each task is
reviewed with Codex: `flowctl codex impl-review <task> --base <base> --receipt <path> --spec codex:gpt-5.6-sol:high`.
In flow-next 9.5.1 the review backend isn't configured (`flowctl review-backend` reads `ASK`), so pass the review
mode explicitly or set it.

## 3. fn-2

It depends on the whole of fn-1. Its two spikes run before their implementations: task 15 (Aseprite) before task 11,
and task 13 (Rive) before task 14. The approved reference drawings it converts are in `avatars/reference/`; see
[docs/HANDOFF.md](../../docs/HANDOFF.md).

## History

`handover-2.md` and `handover-14.md` are from the first pause (2026-09-28). Both tasks have since been done, so they're
kept only as history.

# Work in progress (handoff)

This file tracks where the work stands, so it can continue on another machine. Delete it once everything below is done.

## Where the work stands (2026-11-10)

Everything is on the branch `fn-1-migrate-aipet-from-net-to-rust` (draft PR https://github.com/xMarcinator/ai-pet/pull/1). To pick it up:

1. **Restore flowctl's task state** first: [.flow/paused/README.md](../.flow/paused/README.md). Git doesn't carry it, and without it the done tasks read as todo.
2. **Set the commit identity in the clone** (see Decisions, last item).

### The migration (flow-next spec `fn-1-migrate-aipet-from-net-to-rust`)

- **Done:** 28 of 30 tasks: the Rust hook, registration and doctor; the core, watchers and Board; the hook server; the pet app with its Settings, updates, placement, bubble actions and platform services; the Windows proofs and hands-on pass (task 27, `rust/proofs/windows-hands-on.md`); the Linux hands-on pass (task 22); the performance gate (task 28, Linux); shipping the Rust pet (task 23, milestone 2); the parity ledger (task 24); and the cutover (task 25).
- **Left:** task 29 (the memory audit; Windows still to do with the owner) and task 26 (these docs, on the branch `fn-1-docs`, cut from `fn-1-cutover`).
- **The cutover (task 25)** is on the branch `fn-1-cutover`, cut from `fn-2-layered-sprite-and-vector-pets`: it removes the C# (the last commit with it is `bf3696ea4a`) and merges only after milestone 2 (0.2.0) is released from fn-2. Milestone 3 is released from main after that merge. On that branch, and from milestone 2 on, the shipped pet and hook are the Rust ones; build them with `cd rust && cargo build --release -p aipet -p aipet-hook` (README, "Building from source").

### The pets' new look (flow-next spec `fn-2-layered-sprite-and-vector-pets`)

Planned after fn-1: 15 tasks, R1–R22. It replaces today's code-drawn pet with **layered sprite packs**. A pet is layers (body, eyes, arms, feet, props, headphones, effects), each with animations picked by the pet's behaviour. Pack kinds:
- pixel and painted sheets, including native Aseprite files and grid sheets;
- vector parts drawn with iced's canvas;
- Rive files.

Parts carry their own positions and motion as data (R18), and a pack-making agent skill is planned (R19). Spikes come first for Aseprite (task 15) and Rive (task 13). The spec has the details and decisions.

**Approved reference drawings, for fn-2's vector packs:** `avatars/reference/`.
- **Pets:** Hood (green), Hamster and Sprout, each made by a Python generator (`hood_gen.py`, `hamster_gen.py`, `sprout_gen.py`; run `python avatars/reference/<name>_gen.py` from anywhere, no packages needed).
- **Shared states:** `pet_states.py` holds every state the pet supports today (from `rust/crates/aipet-sprite/src/pet.rs`): moods, idle acts, reactions and second frames, plus the shared effects, magnifier, headphones, whole-body motion, shadow and mirroring.
- **Outputs per pet:** `<pet>.svg`, `<pet>-working.svg` (with experimental typing), `<pet>-preview.html` (the pixel original beside the vector, then every state in 130×120 boxes on three backgrounds) and `<pet>-sheet.svg`.
- **Pixel originals:** `hood-pixel.png`, `hood-pixel-working.png`, `hamster.png`, `sprout-pixel.png`.
- **Review:** Codex rated all three READY on 2026-11-06.
- **The owner's design decisions:**
  - **Hood:** round hood with no peak, slim, short robe, tall glowing eyes, round hands resting in front of the robe that float out to wave, no feet, the laptop at the lower right (one hand holds it, one types) with only a tight glow on `>_`.
  - **Hamster:** true to its pixel reference, after JLandbo's hamster-pet (https://github.com/JLandbo/hamster-pet, used with its author's permission). Since fnt-72 its shading is a darker orange down the right and bottom right, replacing the back in the head's yellow; paws are nubs on the belly, `lens` is round glasses, and it has its own blush, speed lines and the new states' art.
  - **Sprout:** flat body with the outline as its only edge, and no blush.
  - **Pip:** the repo-root `pocket-sprout.html` is the owner's Pip design that fn-2 task 5 builds from.

### Tools and services

- **Reviews:** Codex CLI, model `gpt-5.6-sol`, high effort, with an explicit `--receipt` and no timeout override. If Codex reports the workspace is out of credits, the owner refills it.
- **Progress tracker:** the user-level flow-next-tracker MCP server (`http://127.0.0.1:3210/mcp`). It was unreachable from 2026-11-05, so the board hasn't been told about fn-2, the reference drawings or anything since task 27. Publish fn-2 (`planning_started`, `sync_task`, `planning_finished`) and fn-1's current state once it's back.
- **On Windows,** start the pet detached, so it outlives the shell that started it: `cmd //c start "" "<repo>\rust\target\release\AiPet.exe"` from Git Bash.

## State of this commit

The .NET app's state at release 0.1.0, with what was tested where, is this section as it stood at commit `bf3696ea4a`, the last with the C#. The Rust pet and hook replace it; what is proven of them is in [rust/proofs/](../rust/proofs/) and [rust/PARITY.md](../rust/PARITY.md), and the architecture in [ARCHITECTURE.md](ARCHITECTURE.md).

**Still unproven, whichever runtime:**
- **install.ps1** (there's no pwsh here): removing the old `Run` values, keeping `GITHUB_TOKEN` from the pet, and the fallback without Git Bash.
- **The deploy-key check** in the release's check job, tested only with a stand-in ssh.
- **Codex and Claude behaviour the ordering rules assume.** That a manual `/compact` has its own turn and no Stop, that `turn_aborted` carries `turn_id`, and that Claude's PermissionRequest has the same command or file path as its PreToolUse. Real traffic hasn't shown these yet.
- **The Linux desktops beyond Hyprland** (KDE, Sway, GNOME, X11): flow-next spec `fn-3-linux-desktops-beyond-hyprland-hands-on`.

## Next steps

1. **Release 0.1.0 is out** (2026-09-26, all six jobs green on the first run). It proved the deploy key and the plugin repository, the cross-container Linux hooks (at most `GLIBC_2.16`, under the 2.27 floor), the exes' version resources (`AiPet.exe`, `aipet-hook.exe`, 0.1.0), Velopack's pack, the plugin push and the marketplace pins; CI on windows-2022 passes too. Still untried: installing and updating the Windows app, and the plugins, from a machine other than the developer's (the repositories are private, so the installers need `GITHUB_TOKEN` until they're public).
2. **Small leftovers:**
   - Add the test that `packaging/linux/install.sh` creates a new data folder with mode 0700 and leaves an existing one's mode alone, in the testkit's installer tests (`rust/crates/aipet-testkit/tests/installer.rs`).
   - release.yml's header says the check job checks that "the plugin files are consistent" ([release.yml:6](../.github/workflows/release.yml)). It checks the manifests and marketplaces, and the hook files only as JSON; the plugin job compares the hook files with what the released Linux hook prints.
   - The Windows job's `vpk download github` fetches the latest release for the delta package; releasing a version that isn't newer than it (a hotfix number after a newer release) makes `vpk pack` refuse. Download it only when the new version is newer.
   - Try "Import defaults…" (the file picker) and the ABOUT card in the GUI.
   - If branch protection on main requires ci.yml's old "Plugin files" check, replace it with "Build (windows-2022)" and "Build (ubuntu-22.04)".
3. **A native Wayland front end for the pet: done, by moving to Rust.** On Hyprland the .NET pet's transparent parts took clicks, because Hyprland ignores X11 input shapes, and Avalonia had no layer-shell. A research report (2026-09-26) advised staying on .NET and adding a GTK4 layer-shell backend; the iced spike ([rust/SPIKE.md](../rust/SPIKE.md)) showed a Rust front end could do it instead, and became the migration (Decisions, first item). The Rust pet is a layer surface with an exact input region on compositors with layer-shell, and an X11 window elsewhere (ARCHITECTURE §4).
4. **Before going public:** squash the history, and report the false positives to Microsoft and AhnLab.
5. **Then the next-meeting bubble** (Outlook calendar, optionally Teams' local API for live meeting state), planned and built with flow-next.

## Decisions

- **AiPet is written in Rust (flow-next spec `fn-1-migrate-aipet-from-net-to-rust`).** This supersedes the decision of 2026-09-26 to stay on .NET and add a GTK4 layer-shell backend to the Avalonia app. The iced spike showed that Rust has a front end that does what Avalonia couldn't on Wayland (a layer surface with an exact input region, placement and always-on-top without window rules), so the hook, the core and the pet were ported, kept byte for byte where users or agents can see it (`rust/PARITY.md`), and the C# was removed at the cutover (it is at commit `bf3696ea4a`).
  - The Windows antivirus workaround is retired with it: local .NET builds used `-p:UseAppHost=false` and ran as `dotnet <dll>` so that no fresh `.exe` was made. Cargo always makes an `.exe`, so a freshly built pet or hook may be held for a scan for a while; publishing is still left to GitHub Actions, and nothing here needs PowerShell to build.
- **Codex on Windows keeps its command hooks,** even though Codex runs them through PowerShell (that's Codex's choice).
  - No MCP server and no watcher-only mode.
  - Don't change the registered command strings: Codex would ask users to trust the hooks again.
  - `--doctor codex --probe` stays as it is for direct hooks.
- **Claude's hooks use exec form** (`command` + `args`, no shell). The plugin uses `sh` with `"shell": "bash"`, so Claude never falls back to PowerShell.
- **No autostart at sign-in, and hooks never start the pet.** Users start the pet themselves. The hook never writes to stdout on the hook path, always exits 0, and writes and starts nothing when the pet isn't running.
- **Never rename the marketplace or plugin `aipet`.** `aipet@aipet` is part of every Codex trust key. Codex hook definitions are frozen after the first release.
- **The plugin repository `xMarcinator/ai-pet-plugin` is separate,** and only CI writes to it.
- **Velopack's packId is `AiPetApp`, never `AiPet`:** `%LOCALAPPDATA%\AiPet` is the data folder. Velopack's auto-apply stays off, because Velopack's startup comes before the single instance, where a second launch would have `Update.exe` stop the running pet. `updates::start` installs a pending update instead (`rust/crates/aipet/src/updates.rs`).
- **Hyprland rules are shipped as files** (`packaging/linux/hyprland/aipet.lua` and `aipet.conf`: the pet's layer surface without animation, and Settings without Hyprland's frame) that users include themselves. The installers print the include line and never edit the config. The uninstall removes that line again, after a backup.
- **On Wayland compositors with layer-shell the pet is a layer surface; elsewhere an X11 window** (`AIPET_BACKEND` overrides the choice). The .NET pet's X11-era decisions (no override-redirect window, never `no_focus`, fitting the window to what it shows, overlay popups) went with it: the layer surface's input region is exact, and the desktop shell's window is the full 380x600 with an input shape.
- **No keys sent to other apps.** The .NET pet's Stop button pressed Escape in the agent's app; the Rust pet has an Open button that opens the chat instead.
- **The Linux pet and hook are built for glibc 2.27,** with cargo-zigbuild, which links them against zig's glibc 2.27 stubs, and the release fails if either needs anything newer. Linked on a runner, they would need the runner's much newer glibc, and older distributions would refuse them (the hook on every event).
- **Commits use this identity:** `xMarcinator <25668644+xMarcinator@users.noreply.github.com>`, set in the repository-local git config. A fresh clone doesn't have it, so in each clone run `git config user.name xMarcinator && git config user.email 25668644+xMarcinator@users.noreply.github.com` before committing (61a47b6 went out under a global identity by mistake).

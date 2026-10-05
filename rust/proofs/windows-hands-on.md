# Windows hands-on pass: the Rust pet with real data

The checklist for task fn-1-migrate-aipet-from-net-to-rust.27, part 2: the checks that need a person at the screen.
Part 1 (no GUI started) made the fixes below and the builds, and wrote this list. Each check names the command or
action, what to look for, and a Result column to fill in: **Pass**, **Fail** (with what was seen), or **Not run**
(with why). A failure is fixed in this task, or becomes a new task under the spec that tasks 27 and 23 depend on.

## Windows 10 is not tested

The user has no Windows 10 machine, so this pass checks Windows 11 only (the test laptop: Windows 11 Pro, Intel UHD
plus NVIDIA RTX 2000 Ada, a 2560 × 1440 landscape main monitor and a portrait one to its right). Nothing in the pet
asks for a Windows 11 API: `GetDpiForWindow` and per-monitor DPI v2 exist since Windows 10 1607 and 1703, and Segoe
UI ships with both. These checks are the ones most likely to behave differently on Windows 10, and should be the
first run if a Windows 10 machine turns up:

- **DPI and monitors:** Windows 10's per-monitor scaling and its `WM_DPICHANGED` timing during a move differ in
  detail from Windows 11's. Checks C1–C7 (drag, the held scale change, the menu at 150 %).
- **The taskbar, Alt+Tab and Task View:** Windows 10 has its own shell (and Timeline in Task View).
  `WS_EX_TOOLWINDOW` should keep the pet out of all of them, but it is a different shell. Check B4.
- **Transparency and click-through:** the pet is a plain wgpu window over GL (no WebView, no `UpdateLayeredWindow`).
  Its transparency rests on DWM blur-behind and the GPU driver's GL or Vulkan presentation, which depend on the
  Windows 10 DWM and older drivers. Click-through rests on `SetWindowRgn` plus `WS_EX_TRANSPARENT | WS_EX_LAYERED`.
  Checks B1–B3 and E6.
- **Fonts:** Windows 10 has Segoe UI but not Segoe UI Variable. The pet asks for Segoe UI only, so the text should
  match, but the font check (B5) is cheap to repeat.
- **Antivirus:** Windows 10's Defender versions and SmartScreen may react differently to the unsigned exes (A1–A3).

## What part 1 changed (wave/fn-1.27)

- **The scale change waits for the drop** (`aipet-desktop/src/native/win32.rs`). While the primary button is held,
  the pet window's subclass holds `WM_DPICHANGED` back from winit. At the drop (the release, a lost focus, or the
  button found up on the 33 ms hit-test timer) winit gets the window's DPI, with the window scaled about the pointer
  so the grabbed spot stays under it. This is task 13's chosen fix for the scale flipping at the edge between a
  150 % and a 100 % monitor. Unit tests: `native::sys::tests::a_scale_change_waits_for_the_drop_and_only_the_last_is_applied_once`
  and `at_the_drop_the_window_rescales_about_the_pointer`.
- **The menu's rows are centred** (`aipet-ui/src/menu.rs`): each item's label and check mark sit in the middle of its
  32 px row, not 6–7 px above it. This applies on Linux too.
- **Segoe UI's line height on Windows** (`aipet-ui/src/style.rs`, `view.rs`): `style::LINE_SPACING` is
  (2210 + 514) / 2048 ≈ 1.330 on Windows and 1.362 (Noto Sans) elsewhere, and the bubbles' `TITLE_Y` and `DETAIL_Y`
  derive from it. Unit test: `view::tests::a_bubbles_lines_are_the_ui_fonts_and_centred`.
- **`style::status_colour` is gone** (task 17's follow-up): a state the Board's table lacks gets the C#'s
  `0xFF888888`.

## Before starting

Run everything from Git Bash at the repository root (never PowerShell). The builds were made by part 1 in the task's
workspace, `C:/Users/MJE/Documents/projects/ai-pet-waves/task-27`; after integration, rebuild them in the checkout
used for the pass with the same commands.

| Build | Command | Output |
|---|---|---|
| Debug pet | `(cd rust && cargo build -p aipet)` | `rust/target/debug/AiPet.exe` |
| Demo pet | `(cd rust && cargo build -p aipet --features demo --target-dir target/demo)` | `rust/target/demo/debug/AiPet.exe` |
| Release pet | `(cd rust && cargo build --release -p aipet)` | `rust/target/release/AiPet.exe` |
| Release hook | `(cd rust && cargo build --release -p aipet-hook)` | `rust/target/release/aipet-hook.exe` |
| .NET pet | `dotnet build AiPet.slnx -p:UseAppHost=false` | `src/AiPet.UI/bin/Debug/net10.0/AiPet.dll`, run as `dotnet src/AiPet.UI/bin/Debug/net10.0/AiPet.dll` |

The demo has its own target folder so that it doesn't replace the plain debug build's `AiPet.exe`.

- Quit the installed .NET pet (`$LOCALAPPDATA/AiPetApp/current/AiPet.exe`) before every check except the
  never-two-pets ones. The Rust pet quits at once while another pet runs.
- The pet uses the real data folder (`$LOCALAPPDATA/AiPet`: config.json, jira.json, github.json, aipet.log) and the
  real tokens. Back up `config.json` first: `cp "$LOCALAPPDATA/AiPet/config.json" ~/aipet-config.backup.json`, and
  restore it at the end.
- `AIPET_DEBUG=1` logs pointer, drag, scale, menu and region events to stderr. Keep a log per run:
  `AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-<check>.log`.
- Every run ends through the menu (right-click the sprite, then Quit), or `timeout -k 5 <seconds>` in front of the
  command.
- Each check's screenshot, when one is asked for, goes to `rust/proofs/windows/` as `27-<check>-<what>.png`.

## A. Antivirus (record for the false-positive reports)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| A1 | The release pet | Note the time and the SHA-256 (`sha256sum rust/target/release/AiPet.exe`), then start it once from Explorer and quit. | Any reaction from the antivirus (Trend Micro on this machine) or Defender/SmartScreen: a block, a quarantine, a scan delay, a prompt. Note the product, its version, the detection name and the time. | **Pass**: no reaction. SHA-256 at 10:56: `c25db67b2c8a189450abe54eb374b64fb403867bc4260c046636a47ec739d802`. |
| A2 | The release hook | `sha256sum rust/target/release/aipet-hook.exe`, then `rust/target/release/aipet-hook.exe --doctor; echo $?` | As A1. `--doctor` only reads. | **Pass**: no reaction. SHA-256: `5da792ef42c47cb7a556bca423c5d0ffc30a5eb770f14defd58c8243dea3a4e2`. |
| A3 | The debug and demo pets | Start `rust/target/debug/AiPet.exe` and `rust/target/demo/debug/AiPet.exe` once each, then quit. | As A1. A rebuild can make file operations fail with "Access to the path is denied" for a few minutes: note it, wait, and retry. | **Pass**: no reaction. Debug SHA-256 `703b9764b3f54edec89470bc1a35a6860f7a3af12bb54e42058b9b2fcbd800b0`, demo `0976026b27fe9e51e6759d18907da1fdfc86288cabfb12e69fa07929aa3a1364`. |
| A4 | Reports | For every detection in A1–A3, report the file as a false positive to its vendor (the spec's handling; no code signing). | The report's reference number, here. | **Not needed**: no detections. |

## B. Start, the window and the font

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| B1 | Groundwork start (task 15, check 5) | `AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-B1.log` with no other pet, over an empty, maximised Notepad. | The pet shows in under about a second (GL is the default), bottom-right of the work area, above the taskbar (task 16, check 1). Transparent everywhere it doesn't draw: Notepad's white shows through the bubbles' corners, shadows, the glow and the ground shadow. `grep GPU run-B1.log` names GL on the Intel GPU. | **Pass**. |
| B2 | Transparency with Vulkan (task 13) | `AIPET_DEBUG=1 WGPU_BACKEND=vulkan rust/target/debug/AiPet.exe 2> run-B2.log` | As B1, transparent. The probe line comes before the window shows (the moved `AIPET_DEBUG` probe: no press without a position). | **Pass**: Vulkan on the Intel GPU, shown at 7.4 s. |
| B3 | Transparency on the NVIDIA GPU (task 13) | `AIPET_DEBUG=1 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high rust/target/debug/AiPet.exe 2> run-B3.log` | `grep "GPU for the pet" run-B3.log` names the RTX 2000. The pet is transparent, or record what it shows. | **Pass**: the NVIDIA GPU, PreMultiplied alpha. |
| B4 | Out of Alt+Tab, Task View and the taskbar | With the pet running, press Alt+Tab, then Win+Tab, and look at the taskbar. | The pet shows in none of them. | **Pass**: not in Alt+Tab, Task View or the taskbar. |
| B5 | Font | Open Settings (right-click the sprite → Settings…). | Segoe UI throughout, semibold headings, regular hints; the bubbles' two lines sit in the middle of each bubble (the 1.330 line height). Screenshot. | **Pass**. |
| B6 | Release console (task 15, check 7) | Start `rust/target/release/AiPet.exe` from Explorer. | No console window opens. Quit it. | **Pass**. |
| B7 | Topmost, and "Always on top" again (task 13) | Drag Notepad over the pet; untick Always on top in the menu, click Notepad; tick it again, click Notepad. | Ticked: the pet stays in front. Unticked: Notepad covers it. Ticked again: once Notepad is clicked, the pet is back in front within 2 s. | **Pass** for the behaviour; the menu's check mark for Always on top was wrong (D-x3, under D3). |

## C. Poke, drag and DPI

Set the main monitor to 150 % for C3–C7 (Settings → System → Display), and back to 100 % afterwards. Run each with
`AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-C<n>.log`.

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| C1 | Poke and drag at 100 % | Click the sprite without moving, three times (once with the pointer resting on it first); then drag slowly and quickly, and drop. | Each click bounces (`poked`). The grabbed spot stays under the pointer; the drop lands with its squash, no jump. | **Pass**. |
| C2 | Onto the other monitor at 100 % (not shown by task 13) | Both monitors at 100 %. Drag the pet onto the portrait monitor, let go there, then drag it back. | It follows across, lands on the other monitor under the pointer, and comes back the same way. Screenshot after the drop on the second monitor. | **Pass**. |
| C3 | Drag at 150 % on one monitor | Main monitor at 150 %. Drag and drop on it. | As C1, at the larger size, crisp. | **Pass** (at 150 %). |
| C4 | **Across 150 % and 100 %, side by side (the fix)** | Drag the pet slowly back and forth across the edge between the 150 % main monitor and the 100 % portrait one, pausing on the edge; drop once on each side and once right on the edge. | During the drag the pet keeps its size and doesn't flicker or jump (the log shows no `scale factor` line until the drop). At the drop it takes the other monitor's size once, with the grabbed spot under the pointer, or nudged fully onto that monitor near the edge; no flip-flopping afterwards. | **Pass** (the fix): the scale holds during the drag, and the new one applies at the drop. |
| C5 | Across 150 % and 100 %, stacked (never tried) | Arrange the portrait monitor above (or below) the main one in Display settings, and repeat C4 through the top edge. | As C4. Restore the arrangement afterwards. | **Not run** (optional). |
| C6 | Lost release during a drag (task 13; unit-tested only) | Start a drag, and while holding the button: press Win (Start opens), then let go. Repeat with Alt+Tab, and with Win+D. Do each once on the 150 % monitor near the edge, so a scale change is held. | The pet stops following when the focus goes (`drag cancelled`), with no poke and no landing; any held scale change is applied then, not at the next click. | **Pass**. |
| C7 | Swapped buttons and a lost capture (SPIKE.md 12) | Swap the mouse buttons (Settings → Bluetooth & devices → Mouse), drag with the right one; swap back. Then start a drag and let another app take the capture (e.g. a notification click). | The swapped drag behaves as C1. A lost capture ends the drag within about 0.1 s of letting go. | **Not run** (optional). |
| C8 | Clicks around the sprite (SPIKE.md 12) | Click about 5, 10 and 20 px beside the sprite, and 10 and 25 px beside a bubble. | Within about 10 px of the sprite and 14–18 px of a bubble (the glow and shadows) the pet takes the click; farther ones reach Notepad. | **Pass**. |
| C9 | No flicker or clipping | Watch the pet hop and the bubbles slide, fold and spread for a minute, at 100 % and 150 %. | No flicker, flash, clipped glow, shadow or bubble edge. | **Pass** (at 100 % and 150 %). |

## D. The menu

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| D1 | Rows centred (the fix) | Right-click the sprite at 100 %. | The menu opens inline above the pet. Each label and check mark sits in the middle of its row. Screenshot (compare with `windows/01-menu-offset.png`). | **Pass**. |
| D2 | The menu at 150 % (not shown by task 13) | With the pet on the 150 % monitor (after a drop there), right-click the sprite. | Whole, crisp, not cut off, rows centred. Screenshot. | **Pass** (at 150 %). |
| D3 | Items | Use each: Show bubbles, Always on top, Avatars…, Settings…, Quit. | Show bubbles toggles the bubbles; Avatars… and Settings… open Settings on that page (asking again brings it to the front); Quit ends the pet. | **Fail (D-x3)**: Always on top's check mark followed Show bubbles instead of its own setting. The settings themselves were right; the check marks were canvas meshes, which came out stale and doubled in the GL window (see E-x4). Fixed in 30d6a61 (icons drawn as pictures). **Re-check.** |
| D4 | Closing (SPIKE.md 4 and 10) | Open the menu; left-click and right-click the pet; click elsewhere; click a bubble outside the menu; click the menu's padding and separator. Right-click within 0.4 s of a drop. | A click on the pet only closes the menu (no poke, drag or new menu); a click elsewhere closes it; a bubble click closes it and still acts; padding and separator keep it open; after a drop the menu is placed right. Near the screen's top it opens below the sprite; at the edges it stays on screen. | **Pass**. |

## E. Bubbles, buttons and click-through

Have a working Claude-app chat, a Jira issue with a PR, a GitHub review, Spotify playing, and an error bubble
(for example a wrong GitHub token, restored afterwards). Run `AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-E.log`.

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| E1 | Real chats (task 15, check 1) | Work in a Claude Code chat and in a Codex chat. | Their bubbles, the mood and the laptop or lens follow. A dismissed bubble stays away until the chat does something new. | **Pass**. |
| E2 | Buttons by kind (task 17, checks 2–3) | Point at each kind of bubble, then rest on each button for half a second. | Its buttons and the dismiss X appear, only on the front bubble of a folded stack. Each tooltip shows. No Stop button anywhere. Screenshots of the hovered bubbles. | **Partial**: chat and music bubbles checked; review bubbles not run (none existed). Findings, fixed in 30d6a61, to **re-check**: **E-x1** the dismiss badge was a bare X on unhovered bubbles and a white circle on the hovered one, and spread stacks showed it on every bubble (it must be the C#'s 22 px #2E2E31 circle with a 1 px #66FFFFFF edge and a white 1.6 px X, #4A4A4F hovered, on the hovered bubble only); **E-x4** every icon (dismiss X, Open, media buttons) was drawn twice, the second copy offset, at 100 % and 150 %; **E-x6** a hovered bubble got narrower, where the C#'s keeps its width or grows and shortens its title. |
| E3 | Button clicks (task 17, check 4) | Click each button. | Jira and the PR open in the browser; Previous, Play/Pause and Next drive Spotify; the chat's Open opens the chat or brings its app forward. | **Pass**, but Open is slow to bring Claude forward. **Re-check** with `AIPET_DEBUG=1`: the `aipet: open:` lines time finding the window, bringing it forward and opening a link. |
| E4 | Bubble clicks (task 17, check 5) | Click a folded stack, then a bubble body; then an error bubble. | A folded stack spreads first, then the click opens the item. An error bubble opens its Settings page. | No result reported. |
| E5 | Dismiss and hover (SPIKE.md 5–6) | Hover a bubble and click its X. | The wave, eyes and close button react at once; the bubble leaves and returns when its chat does something new. | **Pass**. |
| E6 | Click-through, GL and Vulkan (not shown by task 13) | With the default (GL), click Notepad beside the pet and in a gap between bubbles. Repeat under `WGPU_BACKEND=vulkan`. | Each click reaches Notepad, with both backends. | **Pass** (GL and Vulkan). |

## F. The agents' apps, Spotify and links (task 18)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| F1 | Claude to the front | Leave the Claude app open behind another window (once minimised), and click a Claude chat bubble that has no deep link. | Claude comes to the front, restored if minimised. | No result reported. |
| F2 | No Claude window | Close Claude and click again. | `claude://` opens Claude; aipet.log says `no claude window found; opening claude://`. | **Pass**. |
| F3 | Codex | With ChatGPT open, click a Codex chat bubble without a deep link; then close ChatGPT and click again. | ChatGPT comes forward. Closed: nothing opens, and aipet.log says `no codex window found`. | **Pass**. |
| F4 | Spotify | Play Spotify and turn on Settings → Listen along. Pause in Spotify. | Within a second the music bubble shows the song with "♫ Artist"; paused: "Paused · Artist". | **Pass**. |
| F5 | Media keys | Run another player too, then use Previous, Play/Pause and Next; click the music bubble. | Only Spotify reacts. Clicking the bubble brings Spotify forward. | **Pass**. |
| F6 | Links and folders | Open a ticket, a pull request and an error link; Settings → Open data folder; Avatars → Open folder. | Links open in the default browser; the folders open in Explorer. | **Pass**. |

## G. Settings

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| G1 | General and Avatars screenshots (task 19, check 1) | Menu → Settings… (General), then menu → Avatars…. | Both pages as the C#'s. Screenshot of each. | **Pass**. |
| G2 | Switches persist (task 19, check 2; task 15, check 5) | Toggle Show chat bubbles, Always on top and Listen along, then `cat "$LOCALAPPDATA/AiPet/config.json"`. Quit and start again. | `Pills`, `OnTop` and `Music` changed; after the restart the switches are as left. | **Pass**. |
| G3 | Import defaults (task 19, check 3) | Write `{"version":1,"jira":{"site":"other.atlassian.net"}}` to a file outside the data folder, Import defaults… and pick it. Then a file containing `not json`. Then cancel the picker. Restore jira.json and the token afterwards. | "Imported Jira (site). It points Jira at other.atlassian.net, so the token you saved…" (when a token was saved), and the Jira page shows the new site. `not json`: "Nothing was imported. The file isn't valid JSON (line 1)." Cancel leaves the line. The pet keeps animating while the picker is open. | **Pass**. The import removed the saved Jira token, because the test preset moved the site: intended. |
| G4 | Avatars (task 19, check 4) | Open folder; copy `avatars/hood-green.json` into `<data>/avatars`, press Reload, click the new tile. | The folder is made and opens; "Hood (green)" appears with its dot and tooltip, and clicking it puts it on and recolours Settings. | **Pass**. |
| G5 | Jira page (task 20, checks 1–3) | Settings → Jira. Test search with the real site, email and token, then with a wrong token. Save. Remove saved token. Restore the token afterwards. | Fields from jira.json, token box empty ("A token is saved. Leave this empty to keep it."). "Searching…" then "Connected. …"; wrong: "Jira didn't accept the email/API token". Save empties the box, shows the Saved text, and an issue bubble appears within about 2 s. Remove shows its text and the button goes. | **Pass**. |
| G6 | GitHub page (task 20, check 4) | Settings → GitHub. Type a token, Test connection, Save, Remove saved token, Create a token. Restore the token afterwards. | Typing ticks the box; "Connected as <login>. …"; Save and Remove as G5; Create a token opens `<host>/settings/personal-access-tokens/new`. | **Pass**. |
| G7 | Updates in a dev build (task 21, check 1) | `AIPET_OPEN_SETTINGS=1 rust/target/debug/AiPet.exe` | General → updates reads "This copy doesn't update itself. To update, install AiPet again." with no button, and no `$LOCALAPPDATA/velopack` log appears. | **Pass**. |
| G8 | Velopack uninstall hook in a dev build (task 21, check 2) | `rust/target/debug/AiPet.exe --veloapp-uninstall 0.3.0; echo $?` | Prints 0, and the pet doesn't start. Nothing changes (no hook next to a dev build). | No result reported. |

## H. Placement and config.json (task 16)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| H1 | Left where it was | Drag the pet, quit, start again. | The pet is where it was left. | **Pass**. |
| H2 | Reset position | Settings → General → Position: Reset. | Back above the taskbar, on the monitor it is on. | **Pass**. |
| H3 | An old Toolbar config | Quit; edit config.json to `"Toolbar":true` with no `WindowHeight`; start the Rust pet, note where it is, quit; start the .NET pet. | Both pets put it in the same place. | **Not run** (the legacy Toolbar config). |
| H4 | Off-screen | Quit; set `"Left":99999`; start. | The pet resets to the corner. | **Pass**. |
| H5 | Untouched config | `stat -c %y "$LOCALAPPDATA/AiPet/config.json"`, start and quit without a change, `stat` again. | The modified time is the same. | **Pass**. |
| H6 | A place on the other monitor | Main monitor at 150 %, drop the pet on the 100 % monitor, quit, start. | It opens on the second monitor where it was left. | **Pass** with both monitors at 100 %; the 150 % variant **not run**. |

## I. Never two pets (task 15, check 2)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| I1 | .NET first | Start the installed .NET pet, then `rust/target/debug/AiPet.exe; echo $?`. | Prints 0 at once, and no second pet shows. | **Pass**, against the branch's .NET build (`dotnet src/AiPet.UI/bin/Debug/net10.0/AiPet.dll`): the old installed .NET pet in `%LOCALAPPDATA%\AiPetApp` is a leftover that doesn't start. |
| I2 | Rust first | Start `rust/target/debug/AiPet.exe &`, then the installed .NET pet (and `dotnet src/AiPet.UI/bin/Debug/net10.0/AiPet.dll`). | No .NET pet shows. | **Pass**, against the branch's .NET build. |
| I3 | Same moment | `"$LOCALAPPDATA/AiPetApp/current/AiPet.exe" & rust/target/debug/AiPet.exe &` (three times). | One pet each time. | **Pass**, against the branch's .NET build, in two rounds: one won by each pet. |

## J. The demo (task 15, check 4; tasks 17, 19, 20)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| J1 | The script | `rust/target/demo/debug/AiPet.exe` (it may run next to a real pet). | The 40 s script plays; Settings → General has Pet mood and Listen along. | **Pass**. |
| J2 | Demo bubbles | Hover and click the demo's bubbles. | The same buttons as E2; the review opens example.atlassian.net. | **Pass**. |
| J3 | Demo Settings | Avatars, Jira and GitHub pages. | Avatars' Reload and Open folder are greyed; Jira and GitHub show, with Save and Remove disabled. | **Fail (J-x8)**: Reload and Open folder weren't greyed out, though they do nothing in the demo. Fixed in 30d6a61 (the disabled look of a Fluent button). **Re-check.** |

## Results summary

- **Run:** 2026-10-05, Windows 11, by the user with the conductor, on the part 1 builds (wave/fn-1.27 at 3738dc3).
  The main monitor is 2560 × 1440 with a portrait monitor to its right. 150 % was used for C3, C4, C9 and D2.
  Windows 10 not tested (no machine).
- **Passed:** A1–A3 (no antivirus reaction to any of the four exes, so A4's reports aren't needed), B1–B7, C1–C4,
  C6, C8, C9, D1, D2, D4, E1, E3, E5, E6, F2–F6, G1–G7, H1, H2, H4, H5, H6 at 100 %, I1–I3 and J1–J2. E2 passed in
  part (see below).
- **Not run:** C5 and C7 (optional), H3 (the legacy Toolbar config), E2 for review bubbles (none existed), and the
  150 % variant of H6. E4, F1 and G8 have no result reported.
- **Found, and fixed in 30d6a61** (this task; a re-check by hand is next):
  - **E-x1, E-x4, D-x3:** the dismiss badge, the round buttons' icons and the menu's check marks came out doubled
    and stale. They were canvas meshes. The settings and the logic were right, and offscreen renders of the same view
    were right too. Every icon is now a picture rasterised from the same path data.
  - **E-x6:** a hovered bubble got narrower. It now keeps its width, or grows, and its text gets the room its
    buttons leave, as the C#'s does.
  - **J-x8:** the demo's Reload and Open folder now look disabled.
  - **E3:** Open is slow to bring Claude forward. `AIPET_DEBUG=1` now logs the time each part of Open takes.

### Re-check after 30d6a61

Rebuild (see [Before starting](#before-starting)), then:

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| R1 | E-x1: the dismiss badge | `AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-R.log`. Hover a bubble, then its badge, then spread a stack and hover each of its bubbles. | A 22 px dark circle (#2E2E31) with a faint 1 px white edge and a white X, on the hovered bubble's top-left corner only, lighter (#4A4A4F) while the pointer is on it. No bare X and no white circle anywhere. In a spread stack, only the hovered bubble has one. | |
| R2 | E-x4: single icons | Hover a Jira bubble with a PR, a working Claude-app chat and the music bubble, at 100 % and at 150 %. | Each icon is drawn once, crisp, in the middle of its button: the dismiss X, Open, the pull request, Previous, Play/Pause and Next. | |
| R3 | E-x6: width on hover | Hover a bubble with a long title, then one with a short title and buttons (the music bubble). | The long one keeps its width and its title is cut shorter beside the buttons. The short one grows by its buttons. Neither gets narrower. | |
| R4 | D-x3: the menu's check marks | Open the menu with each combination of Show bubbles and Always on top (toggle them from the menu). | Each row's check mark shows its own setting, once, with no extra mark. | |
| R5 | J-x8: the demo's buttons | `rust/target/demo/debug/AiPet.exe`, then Settings → Avatars. | Reload and Open folder are greyed out and do nothing. | |
| R6 | E3: Open's timing | With `AIPET_DEBUG=1`, click Open on a working Claude-app chat (Claude open behind another window), then once with Claude minimised. `grep "aipet: open:" run-R.log` | The lines give how long finding Claude's window took and how long until Claude was in front (and its waits of 15 ms), or how long opening the link took. Note the numbers and how slow it feels. | |

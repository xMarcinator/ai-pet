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
| A1 | The release pet | Note the time and the SHA-256 (`sha256sum rust/target/release/AiPet.exe`), then start it once from Explorer and quit. | Any reaction from the antivirus (Trend Micro on this machine) or Defender/SmartScreen: a block, a quarantine, a scan delay, a prompt. Note the product, its version, the detection name and the time. | |
| A2 | The release hook | `sha256sum rust/target/release/aipet-hook.exe`, then `rust/target/release/aipet-hook.exe --doctor; echo $?` | As A1. `--doctor` only reads. | |
| A3 | The debug and demo pets | Start `rust/target/debug/AiPet.exe` and `rust/target/demo/debug/AiPet.exe` once each, then quit. | As A1. A rebuild can make file operations fail with "Access to the path is denied" for a few minutes: note it, wait, and retry. | |
| A4 | Reports | For every detection in A1–A3, report the file as a false positive to its vendor (the spec's handling; no code signing). | The report's reference number, here. | |

## B. Start, the window and the font

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| B1 | Groundwork start (task 15, check 5) | `AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-B1.log` with no other pet, over an empty, maximised Notepad. | The pet shows in under about a second (GL is the default), bottom-right of the work area, above the taskbar (task 16, check 1). Transparent everywhere it doesn't draw: Notepad's white shows through the bubbles' corners, shadows, the glow and the ground shadow. `grep GPU run-B1.log` names GL on the Intel GPU. | |
| B2 | Transparency with Vulkan (task 13) | `AIPET_DEBUG=1 WGPU_BACKEND=vulkan rust/target/debug/AiPet.exe 2> run-B2.log` | As B1, transparent. The probe line comes before the window shows (the moved `AIPET_DEBUG` probe: no press without a position). | |
| B3 | Transparency on the NVIDIA GPU (task 13) | `AIPET_DEBUG=1 WGPU_BACKEND=vulkan WGPU_POWER_PREF=high rust/target/debug/AiPet.exe 2> run-B3.log` | `grep "GPU for the pet" run-B3.log` names the RTX 2000. The pet is transparent, or record what it shows. | |
| B4 | Out of Alt+Tab, Task View and the taskbar | With the pet running, press Alt+Tab, then Win+Tab, and look at the taskbar. | The pet shows in none of them. | |
| B5 | Font | Open Settings (right-click the sprite → Settings…). | Segoe UI throughout, semibold headings, regular hints; the bubbles' two lines sit in the middle of each bubble (the 1.330 line height). Screenshot. | |
| B6 | Release console (task 15, check 7) | Start `rust/target/release/AiPet.exe` from Explorer. | No console window opens. Quit it. | |
| B7 | Topmost, and "Always on top" again (task 13) | Drag Notepad over the pet; untick Always on top in the menu, click Notepad; tick it again, click Notepad. | Ticked: the pet stays in front. Unticked: Notepad covers it. Ticked again: once Notepad is clicked, the pet is back in front within 2 s. | |

## C. Poke, drag and DPI

Set the main monitor to 150 % for C3–C7 (Settings → System → Display), and back to 100 % afterwards. Run each with
`AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-C<n>.log`.

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| C1 | Poke and drag at 100 % | Click the sprite without moving, three times (once with the pointer resting on it first); then drag slowly and quickly, and drop. | Each click bounces (`poked`). The grabbed spot stays under the pointer; the drop lands with its squash, no jump. | |
| C2 | Onto the other monitor at 100 % (not shown by task 13) | Both monitors at 100 %. Drag the pet onto the portrait monitor, let go there, then drag it back. | It follows across, lands on the other monitor under the pointer, and comes back the same way. Screenshot after the drop on the second monitor. | |
| C3 | Drag at 150 % on one monitor | Main monitor at 150 %. Drag and drop on it. | As C1, at the larger size, crisp. | |
| C4 | **Across 150 % and 100 %, side by side (the fix)** | Drag the pet slowly back and forth across the edge between the 150 % main monitor and the 100 % portrait one, pausing on the edge; drop once on each side and once right on the edge. | During the drag the pet keeps its size and doesn't flicker or jump (the log shows no `scale factor` line until the drop). At the drop it takes the other monitor's size once, with the grabbed spot under the pointer, or nudged fully onto that monitor near the edge; no flip-flopping afterwards. | |
| C5 | Across 150 % and 100 %, stacked (never tried) | Arrange the portrait monitor above (or below) the main one in Display settings, and repeat C4 through the top edge. | As C4. Restore the arrangement afterwards. | |
| C6 | Lost release during a drag (task 13; unit-tested only) | Start a drag, and while holding the button: press Win (Start opens), then let go. Repeat with Alt+Tab, and with Win+D. Do each once on the 150 % monitor near the edge, so a scale change is held. | The pet stops following when the focus goes (`drag cancelled`), with no poke and no landing; any held scale change is applied then, not at the next click. | |
| C7 | Swapped buttons and a lost capture (SPIKE.md 12) | Swap the mouse buttons (Settings → Bluetooth & devices → Mouse), drag with the right one; swap back. Then start a drag and let another app take the capture (e.g. a notification click). | The swapped drag behaves as C1. A lost capture ends the drag within about 0.1 s of letting go. | |
| C8 | Clicks around the sprite (SPIKE.md 12) | Click about 5, 10 and 20 px beside the sprite, and 10 and 25 px beside a bubble. | Within about 10 px of the sprite and 14–18 px of a bubble (the glow and shadows) the pet takes the click; farther ones reach Notepad. | |
| C9 | No flicker or clipping | Watch the pet hop and the bubbles slide, fold and spread for a minute, at 100 % and 150 %. | No flicker, flash, clipped glow, shadow or bubble edge. | |

## D. The menu

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| D1 | Rows centred (the fix) | Right-click the sprite at 100 %. | The menu opens inline above the pet. Each label and check mark sits in the middle of its row. Screenshot (compare with `windows/01-menu-offset.png`). | |
| D2 | The menu at 150 % (not shown by task 13) | With the pet on the 150 % monitor (after a drop there), right-click the sprite. | Whole, crisp, not cut off, rows centred. Screenshot. | |
| D3 | Items | Use each: Show bubbles, Always on top, Avatars…, Settings…, Quit. | Show bubbles toggles the bubbles; Avatars… and Settings… open Settings on that page (asking again brings it to the front); Quit ends the pet. | |
| D4 | Closing (SPIKE.md 4 and 10) | Open the menu; left-click and right-click the pet; click elsewhere; click a bubble outside the menu; click the menu's padding and separator. Right-click within 0.4 s of a drop. | A click on the pet only closes the menu (no poke, drag or new menu); a click elsewhere closes it; a bubble click closes it and still acts; padding and separator keep it open; after a drop the menu is placed right. Near the screen's top it opens below the sprite; at the edges it stays on screen. | |

## E. Bubbles, buttons and click-through

Have a working Claude-app chat, a Jira issue with a PR, a GitHub review, Spotify playing, and an error bubble
(for example a wrong GitHub token, restored afterwards). Run `AIPET_DEBUG=1 rust/target/debug/AiPet.exe 2> run-E.log`.

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| E1 | Real chats (task 15, check 1) | Work in a Claude Code chat and in a Codex chat. | Their bubbles, the mood and the laptop or lens follow. A dismissed bubble stays away until the chat does something new. | |
| E2 | Buttons by kind (task 17, checks 2–3) | Point at each kind of bubble, then rest on each button for half a second. | Its buttons and the dismiss X appear, only on the front bubble of a folded stack. Each tooltip shows. No Stop button anywhere. Screenshots of the hovered bubbles. | |
| E3 | Button clicks (task 17, check 4) | Click each button. | Jira and the PR open in the browser; Previous, Play/Pause and Next drive Spotify; the chat's Open opens the chat or brings its app forward. | |
| E4 | Bubble clicks (task 17, check 5) | Click a folded stack, then a bubble body; then an error bubble. | A folded stack spreads first, then the click opens the item. An error bubble opens its Settings page. | |
| E5 | Dismiss and hover (SPIKE.md 5–6) | Hover a bubble and click its X. | The wave, eyes and close button react at once; the bubble leaves and returns when its chat does something new. | |
| E6 | Click-through, GL and Vulkan (not shown by task 13) | With the default (GL), click Notepad beside the pet and in a gap between bubbles. Repeat under `WGPU_BACKEND=vulkan`. | Each click reaches Notepad, with both backends. | |

## F. The agents' apps, Spotify and links (task 18)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| F1 | Claude to the front | Leave the Claude app open behind another window (once minimised), and click a Claude chat bubble that has no deep link. | Claude comes to the front, restored if minimised. | |
| F2 | No Claude window | Close Claude and click again. | `claude://` opens Claude; aipet.log says `no claude window found; opening claude://`. | |
| F3 | Codex | With ChatGPT open, click a Codex chat bubble without a deep link; then close ChatGPT and click again. | ChatGPT comes forward. Closed: nothing opens, and aipet.log says `no codex window found`. | |
| F4 | Spotify | Play Spotify and turn on Settings → Listen along. Pause in Spotify. | Within a second the music bubble shows the song with "♫ Artist"; paused: "Paused · Artist". | |
| F5 | Media keys | Run another player too, then use Previous, Play/Pause and Next; click the music bubble. | Only Spotify reacts. Clicking the bubble brings Spotify forward. | |
| F6 | Links and folders | Open a ticket, a pull request and an error link; Settings → Open data folder; Avatars → Open folder. | Links open in the default browser; the folders open in Explorer. | |

## G. Settings

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| G1 | General and Avatars screenshots (task 19, check 1) | Menu → Settings… (General), then menu → Avatars…. | Both pages as the C#'s. Screenshot of each. | |
| G2 | Switches persist (task 19, check 2; task 15, check 5) | Toggle Show chat bubbles, Always on top and Listen along, then `cat "$LOCALAPPDATA/AiPet/config.json"`. Quit and start again. | `Pills`, `OnTop` and `Music` changed; after the restart the switches are as left. | |
| G3 | Import defaults (task 19, check 3) | Write `{"version":1,"jira":{"site":"other.atlassian.net"}}` to a file outside the data folder, Import defaults… and pick it. Then a file containing `not json`. Then cancel the picker. Restore jira.json and the token afterwards. | "Imported Jira (site). It points Jira at other.atlassian.net, so the token you saved…" (when a token was saved), and the Jira page shows the new site. `not json`: "Nothing was imported. The file isn't valid JSON (line 1)." Cancel leaves the line. The pet keeps animating while the picker is open. | |
| G4 | Avatars (task 19, check 4) | Open folder; copy `avatars/hood-green.json` into `<data>/avatars`, press Reload, click the new tile. | The folder is made and opens; "Hood (green)" appears with its dot and tooltip, and clicking it puts it on and recolours Settings. | |
| G5 | Jira page (task 20, checks 1–3) | Settings → Jira. Test search with the real site, email and token, then with a wrong token. Save. Remove saved token. Restore the token afterwards. | Fields from jira.json, token box empty ("A token is saved. Leave this empty to keep it."). "Searching…" then "Connected. …"; wrong: "Jira didn't accept the email/API token". Save empties the box, shows the Saved text, and an issue bubble appears within about 2 s. Remove shows its text and the button goes. | |
| G6 | GitHub page (task 20, check 4) | Settings → GitHub. Type a token, Test connection, Save, Remove saved token, Create a token. Restore the token afterwards. | Typing ticks the box; "Connected as <login>. …"; Save and Remove as G5; Create a token opens `<host>/settings/personal-access-tokens/new`. | |
| G7 | Updates in a dev build (task 21, check 1) | `AIPET_OPEN_SETTINGS=1 rust/target/debug/AiPet.exe` | General → updates reads "This copy doesn't update itself. To update, install AiPet again." with no button, and no `$LOCALAPPDATA/velopack` log appears. | |
| G8 | Velopack uninstall hook in a dev build (task 21, check 2) | `rust/target/debug/AiPet.exe --veloapp-uninstall 0.3.0; echo $?` | Prints 0, and the pet doesn't start. Nothing changes (no hook next to a dev build). | |

## H. Placement and config.json (task 16)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| H1 | Left where it was | Drag the pet, quit, start again. | The pet is where it was left. | |
| H2 | Reset position | Settings → General → Position: Reset. | Back above the taskbar, on the monitor it is on. | |
| H3 | An old Toolbar config | Quit; edit config.json to `"Toolbar":true` with no `WindowHeight`; start the Rust pet, note where it is, quit; start the .NET pet. | Both pets put it in the same place. | |
| H4 | Off-screen | Quit; set `"Left":99999`; start. | The pet resets to the corner. | |
| H5 | Untouched config | `stat -c %y "$LOCALAPPDATA/AiPet/config.json"`, start and quit without a change, `stat` again. | The modified time is the same. | |
| H6 | A place on the other monitor | Main monitor at 150 %, drop the pet on the 100 % monitor, quit, start. | It opens on the second monitor where it was left. | |

## I. Never two pets (task 15, check 2)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| I1 | .NET first | Start the installed .NET pet, then `rust/target/debug/AiPet.exe; echo $?`. | Prints 0 at once, and no second pet shows. | |
| I2 | Rust first | Start `rust/target/debug/AiPet.exe &`, then the installed .NET pet (and `dotnet src/AiPet.UI/bin/Debug/net10.0/AiPet.dll`). | No .NET pet shows. | |
| I3 | Same moment | `"$LOCALAPPDATA/AiPetApp/current/AiPet.exe" & rust/target/debug/AiPet.exe &` (three times). | One pet each time. | |

## J. The demo (task 15, check 4; tasks 17, 19, 20)

| # | Check | Command or action | Look for | Result |
|---|---|---|---|---|
| J1 | The script | `rust/target/demo/debug/AiPet.exe` (it may run next to a real pet). | The 40 s script plays; Settings → General has Pet mood and Listen along. | |
| J2 | Demo bubbles | Hover and click the demo's bubbles. | The same buttons as E2; the review opens example.atlassian.net. | |
| J3 | Demo Settings | Avatars, Jira and GitHub pages. | Avatars' Reload and Open folder are greyed; Jira and GitHub show, with Save and Remove disabled. | |

## Results summary

To fill in at the end: the date, the build (`git rev-parse --short HEAD`), the monitors and scales used, and every
failure with the task that fixes it.

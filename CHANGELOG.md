# Changelog

All notable changes to AiPet are listed here. Versions follow [Semantic Versioning](https://semver.org/). The
app, `aipet-hook` and the plugin share one version number.

## [Unreleased] - milestone 3

The end of AiPet's move to Rust: the .NET code is gone from the repository. The pet, the hook and the plugin work as
in the milestone 2 release, and an installed copy of any earlier release updates to this one as before.

### Removed
- The C# projects (`src/AiPet.Core`, `src/AiPet.UI`, `src/AiPet.Hook`), their tests (`tests/AiPet.Tests`), the
  solution (`AiPet.slnx`), `Directory.Build.props` and `build/Win32Resources.targets`. Every C# test has Rust tests
  that hold its cases (`rust/PARITY.md`); `cargo test --workspace` in `rust/` runs them all.
- The golden generators (`rust/golden`). The corpora they wrote from the C# stay under `rust/crates/*/tests/golden/`
  as frozen fixtures; corpora the C# wrote on Linux at the cutover replace the replays CI made of it there.
- `tools/IconGen`, which drew the icons with the C# pet's renderer. The icons it made stay committed.
- CI's .NET build and tests, the hook start-up comparison with the NativeAOT hook, the cross-runtime workflow
  (`cross-runtime.yml`: each runtime's hook against the other's pet), the Velopack proof's .NET-to-Rust jobs and their
  harness, and the test variables `AIPET_TEST_HOOK` and `AIPET_GOLDEN`. Only the release's Windows job installs .NET,
  for Velopack's `vpk`.

### Changed
- CI checks that nothing outside this changelog names what was removed (`scripts/check-removed-paths.sh`), and runs
  the hook's trace log tests and the pet's peer check as root on Linux, which the cross-runtime workflow did.
- THIRD-PARTY-NOTICES.md is generated whole by cargo-about from the crates the Windows and Linux release builds
  compile in, each with its licence text (ring's and kamadak-exif's source files were taken for licence texts
  before), and names the icon font iced compiles in. CI checks that the committed file is the generator's
  (`scripts/check-notices.sh`).
- The docs describe the Rust code: the architecture (the crates, the two shells, click-through, the threads), the
  README's build section with each distribution's prerequisites, the handoff's decisions (the move to Rust
  supersedes "stay on .NET"), and the plugin and Windows packaging READMEs. The spike's notes and proofs stay, as
  records the open plans still cite; the architecture's Appendix A lists them.

## [Unreleased]

AiPet's move to Rust: the hook is written in Rust, and the packages ship the Rust pet in place of the .NET one
(milestone 2). It draws its pets from layered packs, listed under Added. A release of the milestone 1 hook with the
.NET pet, if one is cut first, can be updated from and rolled back to without losing settings, tokens or hooks.

### Added
- Pets are layered packs: a folder with a `pack.json` and its art, in the data folder's `avatars/`. Each layer
  follows one part of the pet (its mood, eyes, arms, feet, prop, headphones or effects), so every pet keeps
  eye-follow, idle acts, blinking, the laptop and the magnifier, headphones and its effects. A pack's art is pixel-art
  sheets (crisp at every scale), smooth painted sheets, or vector shapes in the manifest, with parts that move, look
  around and animate. [docs/MAKING-A-PET.md](docs/MAKING-A-PET.md) explains how to make one, and `avatars/samples/`
  has small examples. Rive packs aren't supported yet.
- Packs can come straight from Aseprite: the `.aseprite` file itself, or a `json-array` export with its PNG. Tags
  name the animations and slices place the anchor and the parts. Plain sprite sheets can be described as a grid of
  frames.
- `AiPet --check-pack <folder or theme file>` loads a pack or a theme file as the pet would and prints `OK` or what
  is wrong, without starting the pet.
- Packs load completely or not at all, within limits on file sizes, image sizes, frames and numbers, and only from
  inside their own folder. Settings → Avatars lists each one that didn't load with the reason.
- Six new built-in pets: Pip (a vector pet) and Pip (painted), drawn from it; Hood (vector), Hamster and Sprout
  (vector); and Hamster (pixel). Sprout and Hood are baked from the pixel pets and look and behave as before.
- Themes: a pet can come in several colours (the Hood in Blue or Green, Pip in Peach or Lilac) or, for a painted
  pet, several sets of sheets. Settings → Avatars shows the worn pet's themes, and New theme makes your own from any
  pet with named colours, saved as a file in `avatars/themes/` that Edit and Delete change or remove. The theme you
  pick is saved as `AvatarTheme` in `config.json`.
- Styles: a pet can come in several styles (pixel, vector or painted) under one tile. A pet folder holds a
  subfolder per style, or a pack joins a pet with its `pet` key and names its style with `style`. Settings → Avatars
  shows the worn pet's styles, and the one you pick is saved as `AvatarStyle` in `config.json`.
- New moods in the Rust pet: `news` (it holds up an envelope for 6 s when a review comes in), `sad` (tears, when a
  chat's turn fails, with its own red "Failed" bubble), `tired` (sleepy eyes and a yawn, late at night or after a
  chat of 2 hours), a golden glow that opens every done, and a heart when you poke it. Every built-in pet draws them;
  a pack that doesn't shows them as before (attention, idle, sparkles). The pack format has the new values `sad`,
  `tired`, `news`, `sleepy`, `envelope`, `tears`, `aura` and `heart`.
- The Hamster wears round glasses to read, has its own blush and speed lines, and new shading. Its design comes from
  JLandbo's [hamster-pet](https://github.com/JLandbo/hamster-pet), used with its author's permission.
- Settings → General has Pet size (75 to 200 %) and Bubble size (80 to 120 %), saved as `PetSize` and `BubbleSize`.
- The pet holds still when the system asks for reduced motion, or when `AIPET_REDUCE_MOTION=1` is set: no bobbing,
  squash or idle acts, though its expressions still change.

### Changed
- The pet is the Rust one, in the Windows installer and zip and the Linux packages. It installs to the same folders,
  keeps its settings, logs, tokens and avatars where they were, and an installed copy updates itself as before: the
  update from a .NET release keeps them, and so do the hooks it registered. The packages are much smaller, since
  they carry no .NET runtime.
- On Windows, `AiPet.exe` keeps its icon and version details, and needs no Visual C++ runtime. It draws through
  Vulkan, which leaves no stale copies of the pet behind when it jumps or changes size; it takes a few seconds to
  appear. `WGPU_BACKEND=gl` picks GL instead.
- Linux: the pet runs on glibc 2.27 and newer, like the hook, and needs no runtime of its own.
- Building from source needs only Rust: `build.sh`, `build.ps1` and `install-from-source` build the pet and the hook
  with cargo.
- `aipet-hook` is the Rust one, in the plugin and in the Windows and Linux packages. Its command line, what it sends
  the pet, and what it registers are the same, so hooks registered by an earlier version keep working, and Codex
  doesn't ask you to trust them again.
- On Windows, `aipet-hook.exe` carries the same icon and version details as before, and needs no Visual C++ runtime.
  The Linux hooks still run on glibc 2.27 and newer.
- Linux: two pets started at the same moment no longer both run, also when one of them was started from a snap or
  over SSH. The pet holds a lock next to its socket (`aipet.lock`) while it runs, which the Rust pet will hold too.
  When it can't take that lock (something else is at its path, say), the pet doesn't start, and `aipet.log` says why.
- Settings → Avatars shows one tile per pet. The green Hood is now the Hood's Green theme, and a `config.json` that
  names "Hood (green)" wears it. The avatar `.json` files of earlier versions still load, `avatars/hood-green.json`
  among them.

## [0.1.0] - 2026-09-25

The first release. The first sections describe what it contains. The last two list what changed from the
earlier builds, which were installed from source.

### The pet
- A floating pixel pet that mirrors your coding-agent chats. It thinks, types, searches, waves when a chat needs
  you, naps when everything is quiet, runs when you drag it, waves when you hover and reacts to a click.
- Two built-in avatars (Sprout and Hood), plus your own avatars as JSON files in the data folder's `avatars/`. A
  file the pet can't draw is skipped.
- Right-click the pet for Show chats, Always on top, Avatars…, Settings… and Quit.
- Windows 10/11 x64 and Linux x64/arm64 on X11 or XWayland, Hyprland included. macOS isn't supported yet.

### Chats
- One status bubble per chat for Claude Code (CLI, desktop app, VS Code, Agent SDK) and Codex (CLI,
  `codex exec`, ChatGPT desktop app). The bubbles stack like cards, and a click spreads a stack out.
- Three stacks, from the pet up: what's playing, your chats, and reviews.
- Chats that need you (permissions, questions, MCP input) jump to the front with an amber glow. Bubbles also
  say when a chat is compacting, delegating to a helper, or stopped by a rate limit or an expired sign-in.
- Each bubble's border shows the app the chat runs in: the Claude app, the ChatGPT app, VS Code or a terminal.
- Clicking a bubble opens that chat in its desktop app, or brings the app forward when there's no link to it.
- Busy chats in the Claude or ChatGPT desktop app have a stop button. It presses Escape in the app only when the
  pet can be sure it's the right chat and the app is really in front. Otherwise it opens the chat for you to stop
  it there.
- The pet also reads Codex's own session files, so desktop chats show up, and interrupted turns are noticed,
  even when hooks don't fire.

### Hooks and plugin
- `aipet-hook` only observes. It hands each event to the running pet over a socket that only your user can
  open, prints nothing to the agent and always exits 0. While the pet is closed it does nothing at all.
- One plugin, `aipet` in the `aipet` marketplace, for both Claude Code and Codex. It ships the hook for Windows
  x64, Linux x64 and Linux arm64. On Windows, Claude Code runs it through Git Bash (Git for Windows).
- The Linux hook needs glibc 2.27 or newer (Ubuntu 18.04 and later, for example). Where there's no hook for the
  system (musl, say), the plugin does nothing and reports no error.
- The one-line installers add the plugin to Claude Code and Codex, whichever is installed.
- `aipet-hook --install claude|codex` and `--uninstall claude|codex` register the hook without the plugin, for
  example for the Codex IDE extension. While the plugin is enabled, `--install` changes nothing and says so. The
  one exception is Claude Code on Windows without Git Bash, where the plugin can't run: the hook is registered,
  and you're told to run `--uninstall claude` once Git for Windows is installed. The config files it rewrites
  keep their permissions.
- `aipet-hook --doctor claude|codex` checks the setup, asks the pet whether it's running and shows the last
  events it got. It changes nothing. It knows the plugin, and warns when the plugin and a registration would
  report every event twice.
- `aipet-hook --print-plugin-hooks claude|codex` prints the plugin's hook files as the hook defines them.

### Reviews (optional)
- Jira Cloud issues from a JQL search. The default search is your own open issues. A Jira bubble shows the
  issue's status and its pull request, if it has one. The stack's header counts what's waiting on you.
- GitHub pull requests that request your review, matched to their Jira issue by key. github.com and GitHub
  Enterprise Server are supported.
- Both stay off until you set them up in Settings with your own token. Tokens are kept in Windows Credential
  Manager or the Linux keyring, and Settings can remove them again.
- "Import defaults…" in Settings reads a preset file your team shares: the Jira site and search, and the GitHub
  host, organisations and Jira projects. A preset holds no tokens and no email. When it points Jira or GitHub at
  a new address, the token you saved for the old one is removed.

### Settings, installers and updates
- Settings window with General, Avatars, Jira and GitHub pages. General shows the version and where its update
  stands.
- Spotify listen-along (Windows), or any MPRIS player on Linux.
- Windows: a per-user installer and a portable zip. The installed app looks for a new stable release 3 minutes
  after it starts and then every 6 hours, and downloads it in the background. The update is installed when you
  choose Quit or "Restart to update", or else at the next start. The portable zip doesn't update itself.
- Uninstalling on Windows also removes the hooks that `aipet-hook --install` registered with that install's
  `aipet-hook.exe`. The plugin stays.
- Linux: a tarball per architecture with an `install.sh`. `install.sh --uninstall` removes the app, its menu
  entry and icons, the hooks registered with its `aipet-hook`, and its Hyprland rules. Your settings and logs
  stay.
- Hyprland: the Linux tarball includes window rules (`hyprland/aipet.lua` for Hyprland 0.55 and later,
  `aipet.conf` for `hyprland.conf`). They float and pin the pet, and drop the border, rounding, blur, shadow and
  animation around its transparent window. The installer copies them into the data folder and prints the line
  that includes them; it never edits your config. The uninstall removes that line again.
- Linux: the pet's window is only as big as what it shows, because some compositors (Hyprland) let no click
  through a transparent window. Around it everything can be clicked; inside it, the transparent parts still take
  the mouse on those compositors. `AIPET_NOFIT=1` keeps the window at its full size.
- Linux: menus and tooltips are drawn inside the pet's window, so no compositor can put the pet above them, and
  moving the mouse onto a menu doesn't close it.

### Changed from earlier builds
- The toolbar under the pet is gone, with new chat, voice typing, the chevron and the badge. Show or hide the
  bubbles from the menu or Settings.
- What's playing has its own stack directly above the pet.
- The Avatar submenu became "Avatars…", which opens Settings on the Avatars page. The avatars folder is opened
  from there.
- Only one stack is spread out at a time.
- The stop button shows only for desktop-app chats. On Linux, Escape is pressed only once `xdotool` says the
  app's window is in front.
- The pet doesn't start at sign-in, and hooks never start it. The installers remove the sign-in entries older
  versions made.
- Clicking a Codex chat no longer runs `codex app`, which opened a stray chat.
- Starting AiPet while a pet runs as administrator now does nothing, instead of failing.
- On Linux, the keyring's fallback file (`secrets.json`) is readable by you only from the moment it's written,
  and the installer creates a new data folder for you only.

### Fixed
- Codex: a prompt event that arrives late no longer replaces a newer state, such as a pending permission request.
- Codex: when the same command runs again within seconds, its permission request goes to the right call, so the
  chat keeps asking.
- Codex: a manual `/compact` shows "Compacted" instead of leaving the chat thinking.
- Codex: an interrupted, failed or compacted turn that the session files report is no longer overruled by an
  older hook report.
- Codex: after the clock is set back, new turns are no longer ignored.
- Claude Code: a chat keeps asking for permission when the tool call's start arrives after the request.
- Error bubbles in the reviews stack are no longer cut by its four-bubble limit, and aren't counted as waiting.
- The pet answers hooks on threads of its own, so a busy pet no longer makes hooks wait or lose events.
- On Linux, the hook gives up after a moment when the pet doesn't accept its connection. Its log is
  `aipet-hook-<uid>.log` in the temp folder, readable by you only.

[0.1.0]: https://github.com/xMarcinator/ai-pet/releases/tag/v0.1.0

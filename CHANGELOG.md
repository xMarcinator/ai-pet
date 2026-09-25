# Changelog

All notable changes to AiPet are listed here. Versions follow [Semantic Versioning](https://semver.org/). The
app, `aipet-hook` and the plugin share one version number.

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

# AiPet

A floating desktop pet that shows what your coding-agent chats are doing. It works with Claude Code (the CLI, the
desktop app, VS Code and the Agent SDK) and Codex (the CLI, `codex exec` and the ChatGPT desktop app). The pet
thinks, types and searches while a chat works, waves when a chat needs you, and naps when everything is quiet.

- One status bubble per chat, above the pet. A chat that needs you (a permission, a question) moves to the front
  and glows amber.
- Clicking a bubble opens that chat in its desktop app.
- If you want them: a bubble for each Jira issue your search finds and each GitHub pull request waiting on your
  review, and one for the track playing in Spotify.

AiPet runs on Windows 10/11 x64 and on Linux x64 or arm64 (X11, or XWayland under Wayland). macOS isn't supported
yet.

## Install

### Windows

In PowerShell:

```powershell
irm https://raw.githubusercontent.com/xMarcinator/ai-pet/main/install.ps1 | iex
```

Or download a package from the [latest release](https://github.com/xMarcinator/ai-pet/releases/latest):

- `AiPet-Setup-<version>-win-x64.exe`, the installer. The one-liner runs the same installer.
- `AiPet-<version>-win-x64.zip`, the portable app. Unzip it anywhere and run `AiPet.exe`. It doesn't update
  itself and makes no shortcuts.

Neither package adds the plugins: see [plugins/aipet/README.md](plugins/aipet/README.md).

The Windows files aren't code-signed, so SmartScreen may warn about them. Choose **More info**, then **Run anyway**.

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/xMarcinator/ai-pet/main/install.sh | sh
```

Or download `AiPet-<version>-linux-x64.tar.gz` (or `-linux-arm64`) from the
[latest release](https://github.com/xMarcinator/ai-pet/releases/latest), extract it and run `./install.sh` in the
extracted folder (`--no-start` installs without starting the pet). It installs the app but not the plugins: see
[plugins/aipet/README.md](plugins/aipet/README.md).

### What the installers do

The one-liners ([install.ps1](install.ps1), [install.sh](install.sh)):

1. Download the latest release and check it against the release's `SHA256SUMS`.
2. Install the app for your user, without admin rights:
   - Windows: in `%LOCALAPPDATA%\AiPetApp`, with a Start menu entry and an entry in Settings > Apps.
   - Linux: in `~/.local/share/AiPet/app` (`$XDG_DATA_HOME/AiPet/app` if that is set), with an app menu entry and
     icons.
3. Add the `aipet` marketplace and install the `aipet` plugin in Claude Code and in Codex, whichever of them is on
   your `PATH`. The plugin runs AiPet's hook on each chat event.
4. Start the pet.

Running a one-liner again updates the app and the plugins.

The installers don't make AiPet start at sign-in, and they remove the sign-in entry older versions made. Start the
pet from the Start menu or the app menu. The hooks never start it: while it's closed they do nothing, and a chat
shows up again with its next event once the pet runs.

Settings for the one-liners are environment variables, for example `$env:AIPET_NO_START = '1'` in PowerShell, or
`curl ... | AIPET_NO_START=1 sh`:

| Variable | What it does |
|---|---|
| `AIPET_VERSION=0.1.0` | Installs that release of the app instead of the latest. The plugins still come from the latest release, and on Windows the installed app still updates itself to the latest one. |
| `AIPET_NO_PLUGINS=1` | Installs only the app. |
| `AIPET_NO_START=1` | Doesn't start the pet afterwards. |
| `GITHUB_TOKEN=...` | Only while the repositories are private: a token that can read them. The top of each script says how to fetch the script with it. |

**Codex runs only hooks you trust.** After installing, run `codex`, open `/hooks` and trust the `aipet@aipet`
hooks (or choose *Review hooks* when Codex asks at startup). Then restart the ChatGPT app if it's open. Until
then, the pet sees Codex chats only through Codex's own session files.

**Claude Code** picks up the plugin in new sessions. In an open session, run `/reload-plugins`.

**Windows and Git Bash.** Claude Code runs the plugin's hook through Git Bash, which comes with
[Git for Windows](https://gitforwindows.org/). Without Git Bash, `install.ps1` registers the hook in
`~/.claude/settings.json` instead (`aipet-hook --install claude`), which needs no shell. Install Git for Windows
and run the one-liner again to switch to the plugin. Claude Code and Codex also need git to fetch plugins.

## Linux notes

- **glibc 2.27 or newer.** The release checks that `aipet-hook` needs no newer glibc, so Ubuntu 18.04, Debian 10,
  RHEL 8 and later work. Distributions without glibc (musl ones, such as Alpine) aren't supported.
- **Optional tools.** The installer lists the ones that are missing.
  - `playerctl` (package `playerctl`): the music bubble for Spotify and other MPRIS players. Without it the pet
    asks D-Bus with `dbus-send`.
  - `secret-tool` (package `libsecret-tools`): keeps the Jira and GitHub tokens in your desktop keyring. Without
    it they go into a file only you can read (see [Privacy](#privacy)).
  - `xdotool`: the stop button needs it to bring the agent's app forward and press Escape. `wmctrl` can bring an
    app forward, but then the pet never presses a key.
- The pet is an X11 window, so on Wayland it runs through XWayland. Apps that run as native Wayland windows can't
  be found or brought forward.

### Hyprland

Hyprland frames, rounds, blurs and animates every window. On the pet's transparent window that draws a frame around
the whole rectangle, and animated moves make dragging overshoot. AiPet ships window rules for this. The installers
copy them to `~/.local/share/AiPet/hyprland/` and, when they run under Hyprland, print the line that includes them.
They never edit your config.

For `~/.config/hypr/hyprland.lua` (Hyprland 0.55 and later):

```lua
pcall(dofile, os.getenv("HOME") .. "/.local/share/AiPet/hyprland/aipet.lua")
```

`pcall` skips the file once AiPet is uninstalled, instead of breaking the config.

For `~/.config/hypr/hyprland.conf` (Hyprland 0.53 and 0.54):

```
source = ~/.local/share/AiPet/hyprland/aipet.conf
```

For Hyprland 0.52 and earlier, the end of `aipet.conf` has the same rules as `windowrulev2` lines.

The rules:
- The pet's window floats and is pinned, so it stays on its monitor whichever workspace is shown. It has no border,
  rounding, blur, shadow or animations.
- The Settings window has no border, rounding, blur or shadow.

Don't add `no_focus` to these rules: an XWayland window with it gets no mouse input at all.

Hyprland ignores the X11 input shape, so every pixel of the pet's window would take the mouse. AiPet therefore
keeps its window only as big as what it shows. Inside that box, the transparent parts around the pet and between
the bubbles still take the mouse. `AIPET_NOFIT=1` keeps the window at its full size.

Uninstalling AiPet removes the rules and the line that includes them. The config is backed up first, to
`<config>.aipet-<time>.bak`.

## Using it

### The pet

The pet shows the mood of all your chats together: asleep when nothing is going on, thinking, working, waving when
a chat needs you, or done. Drag it and it runs; it remembers where you put it. Hover over it and it waves; click
it and it reacts.

### Bubbles

Bubbles come in up to three stacks above the pet: Jira and GitHub on top (headed "N waiting on you"), your chats in
the middle, and music just above the pet. A stack with more than one bubble lies on top of itself like cards: click
it to spread it out, then click a bubble to open it. Only one stack is spread out at a time, and it stacks again
shortly after the mouse leaves the pet.

- A chat's bubble says what the chat is doing ("Thinking", the tool it runs, "Needs your input", "Compacting the
  conversation", "Hit a rate limit", ...). Its title is the first line of your latest prompt.
- The border shows the app the chat runs in: coral for the Claude app, green for the ChatGPT app, blue for VS Code,
  grey for a terminal. The bubble names the app too (the Claude app only when chats from other apps are showing).
- Clicking a chat opens it in its desktop app (`claude://claude.ai/epitaxy/<chat id>` for the Claude app,
  `codex://threads/<thread id>` for the ChatGPT app). For a chat in a terminal, or when the chat's id isn't known,
  the app is brought forward instead.
- Clicking a Jira or GitHub bubble opens the issue or pull request. Clicking the music bubble brings the player
  forward.

Hover over the front bubble (or any bubble of a spread stack) for its buttons:
- ✕ dismisses the bubble.
- ■ stops a chat in a desktop app. The pet brings the app forward and presses Escape, which stops the chat the app
  has open. It only presses Escape when that must be this chat: the only chat of that agent the pet knows about,
  with the app really in front, and not in the pet's first 15 minutes (it only knows the chats it has heard from
  since it started). Otherwise it only brings the app forward, so you can stop the chat yourself. Only a chat in a
  desktop app has this button, while it works.
- Jira and GitHub bubbles have buttons that open the Jira issue and the pull request, when the pet knows them.
- The music bubble has previous, play/pause and next.

The pet never sends anything to a chat.

### Menu

Right-click the pet for its menu:
- **Show chats**: hides or shows the bubbles.
- **Always on top**
- **Avatars…**: opens Settings on the Avatars page.
- **Settings…**
- **Quit**: closes the pet. On Windows, an update that is ready is installed then.

### Settings

- **General**
  - Show chat bubbles, Always on top, and "Listen along with Spotify" (or your player): the music bubble, and the
    pet bops along.
  - Position: puts the pet back in the bottom-right corner of the screen.
  - Data folder: where AiPet keeps its settings and logs.
  - Team defaults: **Import defaults…** reads a preset file (below).
  - About: the version, and whether this copy updates itself. The installed Windows app shows the state of its
    updates here, with **Check for updates**.
- **Avatars**: the built-in avatars, and your own. For your own, put an avatar `.json` file (a copy of a built-in
  one with your own colours and shape) in the avatars folder and click **Reload**.
- **Jira**: your Jira Cloud site, your email, an API token and a JQL search. The default search finds your own open
  issues (assigned to you and not done). For reviews, use something like
  `Reviewer = currentUser() AND status = Review ORDER BY updated DESC`.
- **GitHub**: the host (github.com, or a GitHub Enterprise Server host), an access token, and the organisations to
  search for the pull request of a Jira issue. A pull request that mentions a Jira key (in its title, branch,
  description or commits) joins that issue's bubble. GitHub stays off until you save a token.

**Presets.** A team can share its Jira and GitHub defaults as a JSON file, for **Import defaults…**:

```json
{
  "version": 1,
  "jira": { "site": "your-company.atlassian.net", "jql": "assignee = currentUser() AND statusCategory != Done", "enabled": true },
  "github": { "host": "github.com", "orgs": ["your-org"], "jiraProjects": ["ABC"], "enabled": true }
}
```

Every section and field is optional. `jiraProjects` lists the Jira project keys to recognise in pull requests
(Settings has no field for it). A preset never holds a token or an email: AiPet ignores such fields even when a file
has them, and yours stay as they were. If a preset points Jira or GitHub at another address than the saved one,
AiPet removes the token saved for it, so that it isn't sent to an address you never typed in. Paste the token again
if you trust the new address.

### Updates

The app installed on Windows updates itself from the GitHub releases. It checks a few minutes after it starts and
then every few hours, and downloads a new version in the background. The update is installed when you quit the pet,
or at once with **Restart to update** in Settings > General. An update that was downloaded but never installed (the
computer was shut down, say) is installed the next time the pet starts.

The portable zip and Linux don't update themselves. On Linux, run the one-liner again.

## Troubleshooting

`aipet-hook --doctor claude` and `aipet-hook --doctor codex` check that the agent will really run AiPet's hook. They
change nothing. They check the plugin or the registration, policies your organisation set that block hooks
(Claude), which hooks Codex trusts, and hook commands that can't work. They ask the pet whether it's running and show
the last events it got. When the hook is registered directly, `--doctor claude` also runs it once with a test event.
For Codex, add `--probe` to run it through the shell Codex uses (PowerShell on Windows).

The hook is at:
- Windows: `%LOCALAPPDATA%\AiPetApp\current\aipet-hook.exe`
- Linux: `~/.local/share/AiPet/app/aipet-hook`

The logs:

| File | What |
|---|---|
| `aipet.log` in the data folder | The pet's own log. |
| `hook-events.log` in the data folder | One line per hook event the pet got: the agent, the event, the chat, and what the pet made of it. No prompt text. |
| `%TEMP%\aipet-hook.log` (Windows), `aipet-hook-<uid>.log` in `$TMPDIR` or `/tmp` (Linux) | The hook's problems, such as an event the pet didn't take. Written only while the pet is running. |
| `%TEMP%\aipet-setup.log` | The Windows installer's log, when `install.ps1` ran it. |

The data folder is `%LOCALAPPDATA%\AiPet` on Windows and `~/.local/share/AiPet` on Linux. Settings > General > Data
folder opens it.

On Linux, `AIPET_SOFTWARE=1` draws without OpenGL, for virtual GPUs (such as WSLg) where OpenGL output doesn't show.

## Privacy

AiPet runs on your computer and sends your chats nowhere.

**The hook.** Claude Code and Codex run `aipet-hook` on each chat event and give it the event on stdin. The event
holds, for example, the chat's id, the event's name, the tool the chat runs with its input, and the text of a prompt
you submit. For Claude, the hook also reads two environment variables that say where the chat runs
(`CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_HOST_SESSION_ID`). It drops the tool's output, cuts long text, and hands the
event to the running pet over a named pipe (Windows) or a Unix socket (Linux) that only your user can open. It sends
nothing anywhere else and prints nothing back to the agent. When the pet isn't running, the hook writes nothing and
keeps nothing.

**The pet** keeps the chats in memory only, and forgets them when it closes. It also reads Codex's own session files
(`~/.codex/session_index.jsonl` and `~/.codex/sessions/`), and never writes to them. In its data folder it stores:
- `config.json`: the pet's position, avatar and switches.
- `jira.json` and `github.json`: your Jira and GitHub settings (site, email, search, host, organisations), without
  the tokens.
- `aipet.log` and `hook-events.log`. The event log has no prompt text.
- `avatars/`: your own avatars.

**Tokens.** The Jira and GitHub tokens are only ones you paste into Settings, and Settings can remove them again.
They're kept in Windows Credential Manager, or on Linux in your keyring through `secret-tool`. Without a keyring,
Linux keeps them in `secrets.json` in the data folder, a file only you can read (mode `0600`). AiPet reads no other
credentials on the machine.

**Network.** The pet contacts:
- your Jira site and GitHub, only once you turn them on in Settings and save a token;
- GitHub's releases, to check for updates, only in the app installed on Windows. The check sends no token. The
  portable zip and Linux never check.

The installers download the release and the plugins from GitHub.

## Uninstall

- **Windows:** Settings > Apps > AiPet > Uninstall. This removes `%LOCALAPPDATA%\AiPetApp`, and any hooks
  registered directly with the installed `aipet-hook.exe` (the Git Bash fallback above).
- **Linux:** run `~/.local/share/AiPet/app/uninstall.sh`, or `./install.sh --uninstall` in the extracted package.
  This removes the app, its menu entry and icons, the hooks registered with its `aipet-hook`, and the Hyprland rules
  with the line that includes them.
- **A build from source** (Linux): `scripts/install-from-source.sh --uninstall`. It removes the hooks it registered,
  then does what the package's uninstall does.

Your settings and logs stay in the data folder: delete it to remove them too. The plugins are separate:
`claude plugin uninstall aipet@aipet` and `codex plugin remove aipet@aipet`.

## Building from source

Needs the [.NET 10 SDK](https://dotnet.microsoft.com/download).

```bash
dotnet build AiPet.slnx
dotnet test AiPet.slnx
dotnet src/AiPet.UI/bin/Debug/net10.0/AiPet.dll     # runs the pet
```

**Windows developers:** antivirus reacts to PowerShell and to freshly built `.exe` files. Build and test with
`-p:UseAppHost=false` (`dotnet build AiPet.slnx -p:UseAppHost=false`), so that no `.exe` is made, and run programs
as `dotnet <dll>`. Leave publishing and NativeAOT to GitHub Actions: `build.ps1` and
`scripts/install-from-source.ps1` publish `.exe` files, including a NativeAOT hook.

The scripts:

| Script | What |
|---|---|
| [build.sh](build.sh), [build.ps1](build.ps1) | Publish the app and the hook into `artifacts/<rid>/`, copy the hook into `plugins/aipet/native/<rid>/`, and make a local marketplace in `artifacts/marketplace/` for trying the plugin before a release. `build.sh` builds this machine's platform, or the ones named; `build.ps1` builds win-x64 and linux-x64, or `-Only <rid>`. |
| [scripts/install-from-source.sh](scripts/install-from-source.sh), [scripts/install-from-source.ps1](scripts/install-from-source.ps1) | Install that build: the app with a menu or Start menu entry, and the hook, registered directly with Claude Code and Codex (`--no-hooks` / `-NoHooks` skips that). They rebuild first when the build is missing or older than the source. |
| [scripts/check-plugin.sh](scripts/check-plugin.sh) | Checks the plugin's manifests and the marketplaces, and with `--hook <command>` that the plugin's hook files are exactly what `aipet-hook --print-plugin-hooks` prints. |

[.github/workflows/release.yml](.github/workflows/release.yml) builds the releases. CI
([.github/workflows/ci.yml](.github/workflows/ci.yml)) builds and tests on Windows and Linux.

The tests in [tests/AiPet.Tests](tests/AiPet.Tests) run the hook as the agents do (as `dotnet aipet-hook.dll`)
against the pet's listener on a temporary pipe or socket. They never touch your pet, settings or data. The tests of
the shell scripts run only on Linux.

### Hooks without the plugins

`aipet-hook --install claude` and `aipet-hook --install codex` register the hook directly, in
`~/.claude/settings.json` and in `~/.codex/config.toml` (or `hooks.json`, if your other Codex hooks are there).
`aipet-hook --uninstall claude|codex` removes AiPet's hooks again, and nothing else.

- Use either the plugins or these registrations, not both: with both, every event reaches the pet twice. When the
  plugin is enabled and can run, `--install` changes nothing and says so.
- Run the `aipet-hook` executable itself. Under `dotnet aipet-hook.dll`, `--install` refuses, since it would
  register `dotnet`.
- Codex asks you to trust these hooks too. Changing them means trusting them again.
- Each config is backed up before a change, as `<file>.aipet-<time>.bak` (the newest three are kept). Codex's is
  edited as text: your other hooks and their trust entries stay as they are.

### More

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it all fits together, from the pet's animation to how the hooks
  feed it.
- [plugins/aipet/README.md](plugins/aipet/README.md): the plugin for Claude Code and Codex.
- [packaging/windows/README.md](packaging/windows/README.md): the Windows packages and updates.

| Path | What |
|---|---|
| `src/AiPet.UI/` | The app (Avalonia, .NET 10). Platform code is in `Platform/` (Windows, Linux, a macOS stub). |
| `src/AiPet.Core/` | The pet's animation, avatars, the chats and the hooks' listener, and the Jira, GitHub and Codex watchers. |
| `src/AiPet.Hook/` | `aipet-hook`, which the agents run on each event, with `--install`, `--uninstall`, `--doctor` and `--print-plugin-hooks`. |
| `plugins/aipet/` | The plugin for Claude Code and Codex. |
| `packaging/` | The Linux package's installer, menu entry, icons and Hyprland rules; notes on the Windows packages. |
| `tests/AiPet.Tests/` | The tests. |

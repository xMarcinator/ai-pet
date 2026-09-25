# AiPet

A floating desktop pet that mirrors what your coding-agent chats are doing (Claude Code, and Codex in the
ChatGPT desktop app): a pixel buddy that thinks, types, searches, waves when a chat needs you, and naps
when everything is quiet. Windows and Linux (X11/XWayland); macOS is planned but not supported yet.

- One status bubble per chat, stacked like cards; click to spread them out, click a bubble to open that chat
  in its desktop app.
- Chats that need you (permissions, questions) jump to the front with an amber glow.
- Each chat's border shows the app it runs in: coral for the Claude app, green for the ChatGPT app, blue for
  Claude in VS Code, grey for a terminal (Claude Code or Codex CLI). The app is also named on the bubble when
  chats from more than one app are showing, and in its tooltip.
- Bubble actions on hover: dismiss (✕), stop (■), and open the Jira issue / pull request for reviews.
- A separate stack for reviews waiting on you: Jira issues where you're the reviewer, and GitHub pull
  requests requesting your review (matched to their Jira issue by key, e.g. ACS-1234).
- Toolbar: new chat (opens `claude://code/new`), voice typing on Windows (Win+H), hide/show bubbles.
  Changeable avatars; Spotify listen-along. Right-click the pet for Settings (General, Avatars, Jira, GitHub).
- Drag it and it runs; hover and it waves; click it for a boop.

## Layout

| Path | What |
|---|---|
| `src/AiPet.UI/` | The app (Avalonia, .NET 10). Platform code in `Platform/` (Windows, Linux, macOS stub). |
| `src/AiPet.Core/` | State board, pet animation, avatars, the hooks' socket server and the chats it keeps, Jira and GitHub watchers. |
| `src/AiPet.Hook/` | `aipet-hook`, the small program the agents run on each hook event. |
| `plugins/claude-code/` | Claude Code plugin bundling the hook per platform (`bin/<rid>/`, picked by `aipet-hook.sh`). |
| `build.ps1`, `install.ps1`, `install.sh` | Build for win-x64/linux-x64; install on Windows/Linux. |
| `legacy/`, `web/` | Earlier prototypes and retired parts (old Reviews window, Codex plugin), not built or installed. |

Runtime files live in `%LOCALAPPDATA%\AiPet` (Linux: `~/.local/share/AiPet`): `config.json`, `jira.json`,
`github.json`, `aipet.log`, `hook-events.log` (one line per hook event the pet got), `avatars/`. The chats'
states live only in the running pet: the hooks hand each event to it over a local socket that only you can
open (a named pipe on Windows, `/run/user/<uid>/aipet.sock` on Linux).

For how it all fits together, from the pet's animation to how the hooks feed it, see
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Build and install

```powershell
pwsh ./install.ps1            # Windows
```

```bash
./install.sh                  # Linux
```

The installers rebuild first whenever the source is newer than the last build (`-Rebuild` / `--rebuild`
forces it), and start the pet. It doesn't start at sign-in and nothing else opens it: start it from the Start
menu / app menu. While it's closed the hooks do nothing at all, and chats show up again with their next event
once it runs.

If your Claude hooks come from the plugin (`plugins/claude-code`) rather than from `--install claude`, run
`build.ps1` for both platforms after updating AiPet, then update the plugin in Claude Code, so its bundled
hooks speak the pet's current protocol. An older copy never reaches the pet, and still opens it by itself.

## Hooks

The hook only observes: it reads the event, hands it to the running pet, and prints nothing back to the agent,
so it can't be used to put words in a chat. It always exits 0. When the pet isn't running it returns at once,
having written nothing and started nothing (on Linux the .NET runtime still makes its own short-lived files in
the temp folder). Most events run in the background and can finish out of order, so the pet places each by the
time the agent started it; a late, older event is dropped.

**Claude Code.** `aipet-hook --install claude` registers it in `~/.claude/settings.json` for SessionStart,
UserPromptSubmit, PreToolUse, PostToolUse(Failure), PermissionRequest, PermissionDenied, Notification,
Elicitation(Result), Pre/PostCompact, SubagentStart/Stop, Stop(Failure) and SessionEnd. Claude runs the exe
directly. Besides the usual states, a bubble shows "Compacting the conversation", "Delegating to a helper
(Explore)", "Needs your input" (an MCP server or a background chat waiting on you) and what stopped a turn
("Hit a rate limit", "Needs you to sign in again", ...). All run in the background except Stop, which runs in line so the final
"done" state lands last. The hook passes on whether the chat runs in the desktop app, a terminal or an IDE,
and the desktop app's id for it (for the deep link below).

**Codex** (CLI, `codex exec`, and the ChatGPT desktop app). `aipet-hook --install codex` appends a marked
block to `~/.codex/config.toml` (or `hooks.json`, if that's where your other hooks are) for SessionStart,
UserPromptSubmit, PreToolUse, PermissionRequest, Stop, Pre/PostCompact and SubagentStart/Stop, plus
PostToolUse, Interrupt and SessionEnd on Linux/macOS. Notes from how Codex 0.156 works:
- Codex runs hook commands through PowerShell on Windows (`sh -c` elsewhere), so the command is the exe path
  without quotes (a quoted program path is a PowerShell error, exit code 1). On Windows each run starts
  PowerShell (1-4 s), which is why every AiPet hook runs in the background and the events Codex can do
  without are left out there.
- **Codex only runs hooks you've trusted.** After installing, run `codex` in a terminal, choose *Review hooks*
  and trust the AiPet ones (or use `/hooks`), then restart the ChatGPT app. Changing the hooks means trusting
  them again; re-installing an unchanged registration keeps the trust.
- The config is edited as text: your other hooks and their trust entries stay byte-for-byte the same, and AiPet's
  hooks always go after them so their positions (which trust is tied to) don't move. `--uninstall` removes
  AiPet's hooks and their trust entries and nothing else. A backup is kept (`config.toml.aipet-<time>.bak`).
- Hooks started by the desktop app run inside its app package; they reach the pet over the same pipe (not yet
  tried inside the package: the doctor's list of the pet's recent events shows whether they arrive).
- Independently of hooks, the pet reads Codex's own session files (`~/.codex/sessions`,
  `session_index.jsonl`), so desktop chats still show up (and interrupted turns are noticed) when hooks
  aren't trusted or don't fire. When both report a chat, the newest report wins.

**Checking it works.** `aipet-hook --doctor claude` and `aipet-hook --doctor codex` change nothing. They check
the registration, organisation policies that block your hooks (Claude), which hooks Codex trusts (asked from
Codex itself) and hook commands that can't work (like a quoted path), ask the pet whether it's running, and
show the last events it got. `--doctor claude` also runs the hook once with a test event and checks the pet
got it; for Codex add `--probe` to run it through the shell Codex uses (PowerShell on Windows).

## Opening a chat

Clicking a chat bubble opens that chat in its desktop app: `claude://claude.ai/epitaxy/<chat id>` for the
Claude app, `codex://threads/<thread id>` for the ChatGPT app. For chats in a terminal (or when the id isn't
known) the app is brought forward instead.

## Stopping a chat

The pet never sends anything to a chat: there's no reply feature, and a new chat from the toolbar only opens
prefilled in Claude for you to send.

The stop button (■) brings the agent's desktop app forward and presses Escape, which stops the chat the app
has open. Because the apps can't open a specific chat, Escape is only pressed when that must be the right one
(a desktop chat, and the only chat of that agent the pet knows about) and only while the app is really in
front; otherwise the app is just brought forward so you can stop it yourself. The pet only knows the chats it
has heard from since it started, so for its first 15 minutes it never presses Escape. Chats in a terminal have
no stop button.

## Credentials

Jira and GitHub tokens are only ever ones you paste into Settings, which can also remove them again. They're kept in Windows
Credential Manager or the Linux keyring (`secret-tool`; a `0600` file only if no keyring is available).
No other credentials on the machine are read. GitHub reviews stay off until you save a token.

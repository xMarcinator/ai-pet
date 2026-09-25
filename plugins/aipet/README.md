# AiPet plugin for Claude Code and Codex

This plugin connects your Claude Code and Codex chats to the [AiPet](https://github.com/xMarcinator/ai-pet)
desktop pet. One folder serves both agents. On each chat event the agent runs `aipet-hook`, which hands the event
to the running pet over a socket that only your user can open. The hook only observes: it prints nothing to the
agent and always exits 0. While the pet is closed it does nothing at all, and it never starts the pet.

The plugin needs the AiPet app, which is installed separately. The one-line installers install the app and add
this plugin to both agents: see the [main README](https://github.com/xMarcinator/ai-pet#readme). What the hook
reads, and where it sends it, is in the README's [Privacy](https://github.com/xMarcinator/ai-pet#privacy) section.

## Install

Claude Code:

```bash
claude plugin marketplace add xMarcinator/ai-pet
claude plugin install aipet@aipet
```

New sessions pick it up. In an open session, run `/reload-plugins`.

Codex:

```bash
codex plugin marketplace add xMarcinator/ai-pet
codex plugin add aipet@aipet
```

Codex runs only hooks you trust. Run `codex`, open `/hooks` (or choose *Review hooks* when Codex asks at startup)
and trust the `aipet@aipet` hooks. Then restart the ChatGPT desktop app if it's open.

To update: `claude plugin marketplace update aipet`, then `claude plugin update aipet@aipet`. For Codex,
`codex plugin marketplace upgrade aipet`, then `codex plugin add aipet@aipet`. Running a one-line installer again
does the same.

**Use either this plugin or `aipet-hook --install claude|codex`, not both.** With both, every event reaches the pet
twice. When the plugin is enabled and can run, `--install` registers nothing and says so. The one-line installers
remove an older direct registration once the plugin is in.

## Requirements

- Windows x64, or Linux x64 or arm64 with glibc 2.27 or newer.
- **Windows: Claude Code needs Git Bash.** The plugin's hook command is `sh`, with `"shell": "bash"`, so Claude Code
  runs it in Git Bash, which comes with [Git for Windows](https://gitforwindows.org/). Without Git Bash, register the
  hook with `aipet-hook --install claude` instead of using the plugin. The Windows one-line installer does that.
- Codex on Windows runs the hook through PowerShell, which starts `native\win-x64\aipet-hook.exe` directly. Elsewhere
  it runs the launcher through `sh`.

The launcher, `native/aipet-hook.sh`, picks the hook binary for the OS and CPU: `win-x64` on Windows (Git Bash),
`linux-x64` or `linux-arm64` on Linux. Anything it can't run is a silent no-op that exits 0, so the plugin never
disturbs a chat:
- another OS (macOS isn't supported yet) or CPU;
- a Linux without the glibc loader (a musl one, such as Alpine, or a 32-bit userland);
- a missing binary, or one that can't load (a glibc older than the one it was built for, or an `.exe` that
  antivirus blocks).

## What's inside

| Path | What |
|---|---|
| `.claude-plugin/plugin.json`, `hooks/hooks.json` | The Claude Code plugin: 17 events. |
| `.codex-plugin/plugin.json`, `hooks/codex.json` | The Codex plugin: 9 events. |
| `native/aipet-hook.sh` | The launcher that picks the hook binary (Linux, and Git Bash on Windows). |
| `native/<rid>/` | The hook binaries: `win-x64`, `linux-x64`, `linux-arm64`. |

## For maintainers

- The binaries in `native/<rid>/` aren't in the main repository. The release workflow builds them (NativeAOT) and
  commits them, with this folder, to [ai-pet-plugin](https://github.com/xMarcinator/ai-pet-plugin), tagged
  `v<version>`. Only the release workflow writes to that repository. It then pins both marketplace files
  (`.claude-plugin/marketplace.json` and `.agents/plugins/marketplace.json`) to that commit, by setting `ref` to the
  tag and `sha` to the commit. The value `0000000000000000000000000000000000000000` means "not released yet", and
  installing it fails.
- The release job stamps the release's version into both `plugin.json` files of the commit it pushes, so there's
  nothing to bump by hand. Claude Code and Codex only pick up an update when the version changes.
- `hooks/hooks.json` and `hooks/codex.json` must be exactly what `aipet-hook --print-plugin-hooks claude` and
  `codex` print. CI and the release check this with `scripts/check-plugin.sh --hook`. After a change to Claude's
  events, regenerate the file:
  `dotnet src/AiPet.Hook/bin/Debug/net10.0/aipet-hook.dll --print-plugin-hooks claude > plugins/aipet/hooks/hooks.json`.
- `hooks/codex.json` is frozen. Codex trusts a hook by a hash of its definition, so changing a handler makes every
  Codex user trust the hooks again.
- Don't rename the plugin or the marketplace (`aipet@aipet`): the name is part of every Codex trust entry.

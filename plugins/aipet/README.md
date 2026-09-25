# AiPet plugin for Claude Code and Codex

This plugin connects your Claude Code and Codex chats to the [AiPet](https://github.com/xMarcinator/ai-pet)
desktop pet. On each chat event it runs `aipet-hook`, which hands the event to the running pet. The hook only
observes: it prints nothing to the agent and always exits 0. While the pet is closed it does nothing at all.

It needs the AiPet app, which is installed separately. See the [main README](https://github.com/xMarcinator/ai-pet#readme).

## Install

Claude Code:

```bash
claude plugin marketplace add xMarcinator/ai-pet
claude plugin install aipet@aipet
```

Codex:

```bash
codex plugin marketplace add xMarcinator/ai-pet
codex plugin add aipet@aipet
```

Codex only runs hooks you trust. Run `codex`, choose *Review hooks* (or type `/hooks`) and trust the `aipet@aipet`
hooks, then restart the ChatGPT desktop app.

Use either this plugin or `aipet-hook --install claude|codex`, not both. With both, every event arrives twice.

## Requirements

- Windows x64, Linux x64 or Linux arm64.
- Claude Code on Windows runs the hook through Git Bash, so it needs [Git for Windows](https://gitforwindows.org/).
  Without it, register the hook with `aipet-hook --install claude` instead of using the plugin.
- Codex on Windows runs the hook through PowerShell. Codex on Linux runs it through `sh`.

## What's inside

| Path | What |
|---|---|
| `.claude-plugin/plugin.json`, `hooks/hooks.json` | The Claude Code plugin: 17 events |
| `.codex-plugin/plugin.json`, `hooks/codex.json` | The Codex plugin: 9 events |
| `native/aipet-hook.sh` | Picks the hook binary for the OS and CPU (Linux, and Git Bash on Windows) |
| `native/<rid>/` | The hook binaries: `win-x64`, `linux-x64`, `linux-arm64` |

## For maintainers

- The binaries in `native/<rid>/` aren't in the main repository. The release workflow builds them and commits
  them, with this folder, to [ai-pet-plugin](https://github.com/xMarcinator/ai-pet-plugin). It then pins both
  marketplace files (`.claude-plugin/marketplace.json` and `.agents/plugins/marketplace.json`) to that commit
  by setting `ref` to the tag and `sha` to the commit. The value `0000000000000000000000000000000000000000`
  means "not released yet", and installing it fails.
- Bump `version` in both `plugin.json` files on every release. Claude Code and Codex only pick up an update
  when the version changes.
- Don't rename the plugin or the marketplace (`aipet@aipet`): the name is part of every Codex trust entry.
- Changing a handler in `hooks/codex.json` makes every Codex user trust the hooks again. Say so in the release
  notes.

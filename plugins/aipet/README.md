# AiPet plugin for Claude Code and Codex

This plugin connects your Claude Code and Codex chats to the [AiPet](https://github.com/xMarcinator/ai-pet)
desktop pet. One folder serves both agents. On each chat event Claude Code posts the event to the running pet at
`http://127.0.0.1:47615/hook/claude` (an HTTP hook), and Codex calls the pet's `report_event` tool on its MCP
connection to the pet (an `mcp_tool` hook). No program starts per event. The hooks only observe: the pet answers
`{}` and nothing reaches the chat. While the pet is closed they do nothing, and they never start the pet.

The plugin also registers the pet's MCP server, `aipet`, so the agent can drive the pet: wear a pet, change its
sizes, read and dismiss bubbles, save and delete themes, and check and install packs. The server needs a token that
the pet keeps in its data folder; the plugin's `aipet-hook` reads it for the agent (`--print-token-header`).

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
and trust the `aipet@aipet` hooks. Then restart the ChatGPT desktop app if it's open. Updating from a release whose
hooks ran `aipet-hook` means trusting them once more: the hooks changed.

To update: `claude plugin marketplace update aipet`, then `claude plugin update aipet@aipet`. For Codex,
`codex plugin marketplace upgrade aipet`, then `codex plugin add aipet@aipet`. Running a one-line installer again
does the same.

**Use either this plugin or `AiPet --install claude|codex`, not both.** With both, every event reaches the pet
twice. When the plugin is enabled and can run, `--install` registers nothing and says so. The one-line installers
remove an older direct registration (of `AiPet --install` or `aipet-hook --install`) once the plugin is in.
`AiPet --doctor claude|codex` checks the setup.

## Requirements

- The AiPet release this plugin comes with, or a newer one: older pets have no listener, so the hooks reach nothing.
- Windows x64, or Linux x64 or arm64 with glibc 2.27 or newer.
- **Windows: Claude Code needs Git Bash** for the MCP server's token helper, `sh` with the launcher, which comes with
  [Git for Windows](https://gitforwindows.org/). The hooks themselves need no shell. Without Git Bash, register AiPet
  with `AiPet --install claude` instead of using the plugin. The Windows one-line installer does that.
- Codex runs its token helper, `native/token-header.sh`, which runs the launcher. Codex gives a header helper a bare
  environment (HOME, PATH, USER, SHELL, LANG and LOGNAME on Linux), so the helper finds the token only in the
  default data folder: with `XDG_DATA_HOME` or `AIPET_DATA_DIR` set, register with `AiPet --install codex` instead,
  whose helper names the folder. Whether Codex reads the plugin's MCP file (`hooks/codex-mcp.json`) and runs that
  helper as expected hasn't been tried on a real Codex yet, on Windows especially.

The launcher, `native/aipet-hook.sh`, picks the `aipet-hook` binary for the OS and CPU: `win-x64` on Windows (Git
Bash), `linux-x64` or `linux-arm64` on Linux. The plugin runs it only for the token's header. Anything it can't run is
a silent no-op that exits 0, so the agent gets no header and the MCP server answers 401; the hooks still report:
- another OS (macOS isn't supported yet) or CPU;
- a Linux without the glibc loader (a musl one, such as Alpine, or a 32-bit userland);
- a missing binary, or one that can't load (a glibc older than the one it was built for, or an `.exe` that
  antivirus blocks).

## What's inside

| Path | What |
|---|---|
| `.claude-plugin/plugin.json`, `hooks/hooks.json` | The Claude Code plugin: HTTP hooks for 16 events (Claude runs no HTTP hook on SessionStart: the pet makes the chat from its first other event). |
| `.mcp.json` | Claude Code's `aipet` MCP server, with the launcher as its headers helper. |
| `.codex-plugin/plugin.json`, `hooks/codex.json` | The Codex plugin: `mcp_tool` hooks for 11 events, the same on every OS (no SessionEnd: Codex runs no MCP hook there). |
| `hooks/codex-mcp.json` | Codex's `aipet` MCP server, which the manifest's `mcpServers` names, with `native/token-header.sh` as its header helper. |
| `native/aipet-hook.sh` | The launcher that picks the `aipet-hook` binary (Linux, and Git Bash on Windows). |
| `native/token-header.sh` | Codex's header helper: runs the launcher with `--print-token-header`. |
| `native/<rid>/` | The `aipet-hook` binaries: `win-x64`, `linux-x64`, `linux-arm64`. |

## For maintainers

- The binaries in `native/<rid>/` aren't in the main repository. The release workflow builds them with cargo, from
  `rust/crates/aipet-hook` (for Linux with cargo-zigbuild, for glibc 2.27 and newer), and commits them, with this folder, to
  [ai-pet-plugin](https://github.com/xMarcinator/ai-pet-plugin), tagged `v<version>`. Only the release workflow
  writes to that repository. It then pins both marketplace files (`.claude-plugin/marketplace.json` and
  `.agents/plugins/marketplace.json`) to that commit, by setting `ref` to the tag and `sha` to the commit. The value
  `0000000000000000000000000000000000000000` means "not released yet", and installing it fails.
- To try the plugin before a release, `build.sh` (or `build.ps1`) copies a local hook build into `native/<rid>/`
  and writes a local marketplace in `artifacts/marketplace/`; `claude --plugin-dir plugins/aipet` uses the folder
  for one session.
- The release job stamps the release's version into both `plugin.json` files of the commit it pushes, so there's
  nothing to bump by hand. Claude Code and Codex only pick up an update when the version changes.
- `hooks/hooks.json`, `hooks/codex.json`, `.mcp.json` and `hooks/codex-mcp.json` must be exactly what
  `aipet-hook --print-plugin-hooks claude`, `codex`, `claude-mcp` and `codex-mcp` print, the same tables
  `AiPet --install` registers. CI and the release check this with `scripts/check-plugin.sh --hook`, against the Rust
  hook. Claude's events are in `rust/crates/aipet-hook/src/claude.rs`, Codex's in `codex.rs` (`MCP_EVENTS`), the MCP
  files in `plugin_hooks.rs`. After a change, regenerate the file from the repository's root, for example:
  `cargo run -q --locked --manifest-path rust/Cargo.toml -p aipet-hook -- --print-plugin-hooks claude > plugins/aipet/hooks/hooks.json`,
  then check them all: `scripts/check-plugin.sh --marketplaces . plugins/aipet --hook cargo run -q --locked --manifest-path rust/Cargo.toml -p aipet-hook --`.
- Change `hooks/codex.json` only on purpose. Codex trusts a hook by a hash of its definition, so changing a handler
  makes every Codex user trust the hooks again. The move to `mcp_tool` hooks did that once.
- Don't rename the plugin or the marketplace (`aipet@aipet`): the name is part of every Codex trust entry.

# MCP server hands-on pass (fnt-71 task 8)

Real Claude Code and Codex sessions against the pet's listener, with the files `AiPet --install` writes. Each check
says what was run, what was seen, and a Result: **Pass**, **Fail** (with what was seen), or **Not run** (with why).
R13 and R15's measurements are in `memory.md`: both met on Linux, R15 after the fix there.

## Linux (2026-11-13)

Arch Linux, Hyprland; Claude Code 2.1.322 (model haiku), Codex CLI 0.167.2; the release `AiPet` from
`fnt-71-mcp-server`.

**Setup.** The pet ran in a sandbox: its own data folder (`XDG_DATA_HOME`) and hook socket (`AIPET_PIPE`), on the
real port 47615 (no other pet was running). `AiPet --install claude|codex` wrote into throwaway
`CLAUDE_CONFIG_DIR` and `CODEX_HOME` folders. The real agents were then pointed at those files without touching
`~/.claude` or `~/.codex`:

- Claude: `claude -p … --settings <sandbox settings.json> --mcp-config <the aipet entry from the sandbox .claude.json>
  --strict-mcp-config --setting-sources project`, so the user's own hooks didn't run.
- Codex: one thread through `codex app-server`, with the sandbox `config.toml`'s AiPet block passed as `-c`
  overrides, and the hooks trusted for that run with `hooks.state` (each key's `currentHash` from `hooks/list`), as
  in the spike. Codex's `hook/started` and `hook/completed` notifications show each hook's result.

Afterwards `~/.claude/settings.json` and `~/.codex/config.toml` had their old mtimes. `~/.claude.json` had changed
(Claude Code rewrites it every run) but held no `aipet` server.

| # | Check | Seen | Result |
|---|---|---|---|
| L1 | `AiPet --install claude`, `--install codex`, then `--doctor` for each with the pet up | Both installs exit 0. Doctor: token found, the pet answers on 47615, hooks and server registered; Codex warns its hooks aren't listed yet (not trusted) | Pass |
| L2 | A Claude session: `cat` with Bash, then the `status` tool | `headersHelper` gave the header in 17 ms, MCP connected in 24 ms; 9 HTTP hook posts all 200 in 1-6 ms; `status` answered in 20 ms. The pet recorded UserPromptSubmit → thinking, Pre/PostToolUse → working/thinking, Stop → done, SessionEnd → removed | Pass |
| L3 | A Codex turn: `cat` with Bash, then `status` | **Failed first**: the MCP handshake got 401, so every `mcp_tool` hook failed with "failed to get client". Codex runs `http_headers_helper` with only HOME, PATH, USER, SHELL, LANG and LOGNAME, so the helper's AiPet looked in the default data folder, not `XDG_DATA_HOME`'s. **Fixed**: the helper script now sets `AIPET_DATA_DIR` to the pet's data folder itself. After the fix: MCP ready, all 11 hooks completed; the pet recorded SessionStart → idle through Stop → done; `registrations_seen` true for both | Pass (after the fix) |
| L4 | Each tool from a real agent | Claude called `list_avatars`, `wear_avatar` (Hood; `config.json` changed, which Settings reads), `save_theme`, `list_themes`, `delete_theme` (the file went to `trash/`), `get_settings`, `set_settings` (pills off, in `config.json`), `get_board` (this session's card), `check_pack` and `install_pack` (`avatars/samples/basic` installed as Sample). `report_event` ran through Codex's hooks. `dismiss_card` was not called by an agent (covered by the listener tests) | Pass |
| L5 | The pet stopped and restarted during a Claude session | Hooks after the restart got 200 and the restarted pet carried on with the session's card; the `status` call after the restart reached the new pet | Pass |
| L6 | A whole Claude session with the pet down | The session finished normally (exit 0); each hook failed in about 4 ms with ECONNREFUSED. Claude **prints** `SessionEnd hook [...] failed: connect ECONNREFUSED` in `-p` mode, where the exe hook was silent: the spec's fail-fast choice; worth a line in the docs | Pass |
| L7 | The pet restarted between two Codex turns in one thread | The second turn's hooks all completed and reached the new pet: Codex reconnects to a server that went away mid-thread (only one that was down at thread start stays failed, per the spike) | Pass |
| L8 | Claude's startup output | Claude printed `unknown format "uint16" ignored` for `status`'s `port`. **Fixed**: the field's schema has no format now, and the conformance test allows only formats a validator knows | Pass (after the fix) |
| L9 | Codex re-trust through the UI ("Hooks need review" in an interactive `codex`) | The trust here came from `hooks.state` overrides; the interactive review needs the owner | Not run |
| L10 | Settings showing a tool's change while open | `config.json` changed as Settings expects; Settings wasn't opened (GUI, with the owner) | Not run |
| L11 | The Codex plugin's `hooks/codex-mcp.json` and `${PLUGIN_ROOT}` in its header helper | Installing the plugin would change `~/.codex`; with the owner | Not run |

**A limit the fix can't cover:** the Codex plugin's `token-header.sh` can't know a data folder moved by
`XDG_DATA_HOME` or `AIPET_DATA_DIR`, because Codex drops both. Plugin users with the default data folder are fine.
`AiPet --install codex` names the folder in its helper, so use that when the data folder is elsewhere.

## Windows

Not run yet: with the owner, after the Linux measurements.

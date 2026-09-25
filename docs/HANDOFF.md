# Work in progress (handoff)

This file tracks where the work stands, so it can continue on another machine. Delete it once everything below is done.

## State of this commit

This commit is a snapshot taken in the middle of the work. It has not been reviewed or tested, so don't install it yet.

**Done and tested:**
- **The socket protocol.** The hook sends each event over a named pipe (Windows) or a Unix socket (Linux). When the pet isn't running, the hook exits and writes nothing. The pet keeps chats in memory only, and the hooks no longer start it (see [ARCHITECTURE.md](ARCHITECTURE.md)).

**Implemented but not yet reviewed or tested:**
- **Hook hardening.**
  - `Native.cs` process inspection is removed. Dispatch time is the hook's start.
  - Codex events are ordered by `turn_id` and `tool_use_id`.
  - The session id comes from `ProcessIdToSessionId`, and nothing on the hook path uses `System.Diagnostics.Process`.
  - `ConcurrentGarbageCollection` is false.
- **Listener fix.** The listeners run on dedicated threads, the hook waits longer, and it logs a line when it gives up on a pipe that exists.
- **Toolbar removed** with all its features: compose, voice typing, the chevron and the badge.
- **Music has its own stack** directly above the pet.
- **`FocusAgent` no longer starts `codex app`,** which ran PowerShell and opened a stray chat.

**Distribution, part 1 (written, still being checked):**
- Repository files: LICENSE (MIT), THIRD-PARTY-NOTICES, CHANGELOG, `Directory.Build.props`.
- `plugins/aipet/`, one folder for both agents, and the Claude and Codex marketplace files. They point at `xMarcinator/ai-pet-plugin`, a separate repository that only CI writes to.
- The icon: `tools/IconGen`, `assets/icon/`, the plugin assets and the Linux hicolor icons.
- `.github/workflows/ci.yml` and `release.yml`, whose Windows and Linux jobs build natively, followed by a plugin job and a publish job.
- The end-user one-liners `install.ps1` / `install.sh`. The old from-source scripts moved to `scripts/`.
- Neutral Jira and GitHub defaults. With no orgs set, the PR lookup doesn't search all of GitHub.

## Next steps

1. Review and test the hardening, toolbar and music changes. Don't forget the end-to-end ordering test and the starved thread-pool test.
2. Finish checking distribution part 1: the release workflow, the installers and the manifests (`claude plugin validate plugins/aipet`).
3. **Distribution, part 2:**
   - Add the icon and version resource to both csproj files (`ApplicationIcon`, `AssemblyTitle`). The hook's resource should name `aipet-hook.exe`, not `.dll`.
   - Velopack in the app (packId `AiPetApp`, never `AiPet`, because that's the data folder), with updates from GitHub releases.
   - "Import defaults…" in Settings. It reads the preset JSON: `{"version":1,"jira":{site,jql,enabled},"github":{host,orgs[],jiraProjects[],enabled}}`, with no tokens and no email. The preset file itself is kept outside the repository.
   - `aipet-hook --print-plugin-hooks`, and a CI check that the plugin hook files match the code.
   - A README written for end users.
4. **Before the first release:** create `xMarcinator/ai-pet-plugin`, add a deploy key and the `PLUGIN_DEPLOY_KEY` secret, then run `release.yml`.
5. **Before going public:** squash the history, and report the false positives to Microsoft and AhnLab.
6. **Then the next-meeting bubble** (Outlook calendar, optionally Teams' local API for live meeting state), planned and built with flow-next.

## Decisions

- **Windows antivirus (Trend Micro) reacts to PowerShell and to freshly built `.exe` files.**
  - Build locally with `dotnet build -p:UseAppHost=false`, run programs with `dotnet <dll>`, and leave publishing to GitHub Actions.
- **Codex on Windows keeps its command hooks,** even though Codex runs them through PowerShell (that's Codex's choice).
  - No MCP server and no watcher-only mode.
  - Don't change the registered command strings: Codex would ask users to trust the hooks again.
  - `--doctor codex --probe` stays as it is.
- **Claude's hooks use exec form** (`command` + `args`, no shell). The plugin uses `sh` with `"shell": "bash"`, so Claude never falls back to PowerShell.
- **No autostart at sign-in.** Users start the pet themselves.
- **Never rename the marketplace or plugin `aipet`.** `aipet@aipet` is part of every Codex trust key. Codex hook definitions are frozen after the first release.
- **Commits use this identity:** `xMarcinator <25668644+xMarcinator@users.noreply.github.com>` (repository-local git config).

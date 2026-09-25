# Changelog

All notable changes to AiPet are listed here. Versions follow [Semantic Versioning](https://semver.org/). The
app, `aipet-hook` and both plugins share one version number.

## [0.2.0] - unreleased

The first public release.

### The pet
- A floating pixel pet that mirrors your coding-agent chats. It thinks, types, searches, waves when a chat needs
  you, naps when everything is quiet, runs when you drag it, waves when you hover and reacts to a click.
- Two built-in avatars (Sprout and Hood), plus your own avatars as JSON files in the data folder's `avatars/`.
- Windows 10/11 x64 and Linux x64/arm64 on X11 or XWayland. macOS isn't supported yet.

### Chats
- One status bubble per chat for Claude Code (CLI, desktop app, VS Code, Agent SDK) and Codex (CLI,
  `codex exec`, ChatGPT desktop app). The bubbles stack like cards, and a click spreads them out.
- Chats that need you (permissions, questions, MCP input) jump to the front with an amber glow. Bubbles also
  say when a chat is compacting, delegating to a helper, or stopped by a rate limit or an expired sign-in.
- Each bubble's border shows the app the chat runs in: the Claude app, the ChatGPT app, VS Code or a terminal.
- Clicking a bubble opens that chat in its desktop app. The stop button brings the app forward and presses
  Escape, but only when the pet can be sure it's the right chat.
- The pet also reads Codex's own session files, so desktop chats show up, and interrupted turns are noticed,
  even when hooks don't fire.

### Hooks and plugins
- `aipet-hook` only observes. It hands each event to the running pet over a socket that only your user can
  open, prints nothing to the agent and always exits 0. While the pet is closed it does nothing at all.
- A Claude Code plugin and a Codex plugin, both named `aipet` in the `aipet` marketplace. Each ships the hook
  for Windows x64, Linux x64 and Linux arm64.
- `aipet-hook --install claude|codex` and `--uninstall claude|codex` register the hook without the plugins,
  for example for the Codex IDE extension. Use either a plugin or a registration, not both.
- `aipet-hook --doctor claude|codex` checks the setup, asks the pet whether it's running and shows the last
  events it got. It changes nothing.

### Reviews (optional)
- Jira Cloud issues from a JQL search. The default search is your own open issues.
- GitHub pull requests that request your review, matched to their Jira issue by key. github.com and GitHub
  Enterprise Server are supported.
- Both stay off until you set them up in Settings with your own token. Tokens are kept in Windows Credential Manager or the Linux
  keyring, and Settings can remove them again.

### Other
- Toolbar: new Claude chat, voice typing (Windows) and show/hide bubbles.
- Spotify listen-along (Windows), or any MPRIS player on Linux.
- Settings window with General, Avatars, Jira and GitHub pages.
- Windows: a per-user installer with updates, and a portable zip. Linux: a tarball per architecture with an
  `install.sh`.

[0.2.0]: https://github.com/xMarcinator/ai-pet/releases/tag/v0.2.0

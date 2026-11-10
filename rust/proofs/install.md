# Installing, updating and rolling back (fn-1 task 23)

The milestone 2 checks with real release packages: a clean install, an update from the milestone 1 release (0.1.0),
and a rollback to it (R15, R17).

**Packages:**
- **The new version:** release dry run 0.2.0-dry.3, run 42780171062, from `fn-2-layered-sprite-and-vector-pets` at
  506859a. Every job passed, and every package matched the run's SHA256SUMS.
- **The old version:** 0.1.0, the published release.

## Linux (2026-11-10, Arch, Hyprland 0.56, x64)

Each run used a scratch `HOME`, so the owner's install, `~/.claude` and `~/.codex` weren't touched. The installer was
the tarball's `install.sh --no-start`.

| Check | Result |
|---|---|
| Clean install | The app folder holds `AiPet`, `aipet-hook` and `uninstall.sh`, with the menu entry and icons beside it. The pet starts and shows its layer surface; its log says it is listening on the socket. |
| Clean uninstall | The app, menu entry, icons and Hyprland rules are gone. `aipet.log` stays, as the uninstall says it will. |
| Update from 0.1.0 | Set up first: 0.1.0 installed, a non-default `config.json` (position, Pills, OnTop, Avatar, Music, WindowHeight), and hooks registered with `aipet-hook --install claude` and `--install codex`. After installing the new version over it: the .NET files are gone and the app is the two Rust binaries; `config.json`, `.claude/settings.json` and `.codex/config.toml` are byte for byte as before. |
| The updated pet uses the old data | The Rust pet reads the 0.1.0 config (`Loaded`). Events through the registered hook reach it (`UserPromptSubmit … -> thinking`). `aipet-hook --doctor claude` says "All good". `--doctor codex` reports only that the sandbox's hooks aren't trusted yet, since Codex was never run there. |
| Rollback to 0.1.0 | The Rust pet's own config writer (`aipet_core::config`) wrote a change that includes the Rust-only fields `AvatarTheme`, `AvatarStyle`, `PetSize` and `BubbleSize`. 0.1.0 installed over it then reads that file: its window opens at the saved Top, adjusted for its height (y 626), and moving Left by 400 moves x by 400. With no config it opens elsewhere (5464,1040). It leaves the file unchanged. |
| Uninstall after the update | The Rust version's `uninstall.sh` removes the hooks 0.1.0 registered with Claude Code and Codex, and keeps `config.json` and the logs. |

**Not covered on Linux:**
- Tokens. The keyring sits on the session's D-Bus, which a scratch `HOME` doesn't separate, so tokens weren't tested.
  On Windows, the token check is part of the owner's run.
- Velopack's in-app update. Linux has no in-app update; the tarball installer is the update path.

## Windows (2026-11-10, the owner's machine)

The owner installed `AiPet-Setup-0.2.0-dry.3-win-x64.exe` from the same run and reported that the installation works.
Their run was not itemised step by step: the update from 0.1.0, the rollback and the tokens are covered by "the setup
seems to work".

The same run turned up three things outside installing, now in fn-2's polish task:
- **Ghosts:** stale copies of the pet show when it jumps and when its size changes.
- **Bubble gap:** the vector Hood's bubbles sat higher than the pixel Hood's.
- **Settings:** a pet's style and theme weren't remembered when switching pets.

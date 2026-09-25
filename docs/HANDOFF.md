# Work in progress (handoff)

This file tracks where the work stands, so it can continue on another machine. Delete it once everything below is done.

## State of this commit

The first snapshot has been reviewed, fixed and tested on Linux (Arch, Hyprland 0.56), together with the second part of the distribution work. What's left to check needs Windows or the first CI and release runs. Nothing is released yet.

**Done and tested on Linux:**
- **The tests.** `dotnet build AiPet.slnx -p:UseAppHost=false` builds with no warnings, and `dotnet test tests/AiPet.Tests --no-build` passes all 234 tests. They run the hook as `dotnet aipet-hook.dll` against a temporary socket and folders, and never reach your pet, config or data ([TestEnv.cs:15](../tests/AiPet.Tests/TestEnv.cs)). `claude plugin validate` passes for `plugins/aipet` and for the marketplace, and so does `scripts/check-plugin.sh --marketplaces . plugins/aipet --hook dotnet <the hook's dll>`.
- **The socket protocol and the listeners.** On Linux the pet listens on a plain blocking Unix socket: mode 0600 before it listens, accepts on 8 threads of its own, checks each peer's uid and cuts off silent clients with socket timeouts. At most 64 connections are handled at once ([HookServer.cs:36](../src/AiPet.Core/HookServer.cs)). The starved thread-pool test passes ([StarvedPoolTests.cs:21](../tests/AiPet.Tests/StarvedPoolTests.cs)).
- **Hook hardening.** A 2 s stdin deadline (nothing is sent after it), room in the line for `sent`, escaped lone surrogates mended, a time limit on the Linux connect, a per-user 0600 trace log on Linux, and a launcher that maps only known platforms and always exits 0. See [ARCHITECTURE.md](ARCHITECTURE.md).
- **Ordering.** Claude's PreToolUse and PermissionRequest are paired, the Codex rules cover late prompts, reruns of the same command, manual `/compact` and a clock set back, and Board orders the hook against Codex's session files by turn. The end-to-end ordering tests pass ([HookServerTests.cs:22](../tests/AiPet.Tests/HookServerTests.cs)).
- **Registration and the doctor.** `--install` changes nothing while the plugin is enabled, refuses to run as `dotnet aipet-hook.dll`, and keeps the config files' modes. The Codex doctor understands the plugin's hooks.
- **The UI.** The toolbar is gone and music has its own stack. Only one stack spreads out at a time, and Stop shows only for desktop-app chats. Avatars the pet can't draw are skipped, `secrets.json` is 0600 from the start, and "Import defaults…" reads a preset (the parsing and the token rule are tested).
- **Linux and Hyprland,** tried on Hyprland 0.56.2 through XWayland. Menus and tooltips are drawn inside the window ([Program.cs:48](../src/AiPet.UI/Program.cs)), the menu opens above the pet, and the Avatar submenu became "Avatars…". The window is fitted to what it shows ([MainWindow.axaml.cs:691](../src/AiPet.UI/MainWindow.axaml.cs)): 164x160 with only the pet, and the whole menu shows. The window rules in `packaging/linux/hyprland/` work. The installers' Hyprland hint and the uninstall's removal of the include line were tried in sandboxes, including a symlinked config, and so was a round trip of `scripts/install-from-source.sh` and its new `--uninstall`.
- **Distribution files.** `aipet-hook --print-plugin-hooks` prints both hook files byte for byte, and CI compares them on both OSes. CI checks the scripts' executable bits. The release workflow's latest check, re-trust note and deploy-key check are tested with stand-ins ([ReleaseWorkflowTests.cs:150](../tests/AiPet.Tests/ReleaseWorkflowTests.cs)). The icon and version resources are in both dlls ([ResourceTests.cs:39](../tests/AiPet.Tests/ResourceTests.cs)), and a win-x64 build made on Linux copied them into the `AiPet.exe` apphost.
- **Velopack's wiring,** as tests can see it: Main calls `VelopackApp.Run` first, `Updates.Start` comes after the single-instance mutex, auto-apply is off ([VelopackTests.cs:127](../tests/AiPet.Tests/VelopackTests.cs)), and `VPK_VERSION` is the package's version. The uninstall cleanup's matching, and the `--uninstall` runs it starts, are tested with stand-in configs and a stand-in hook.

**Done, but only Windows or the first CI or release run can show that it works:**
- **Velopack at run time.** That vpk pack accepts the app (it checks that Main calls `VelopackApp.Run`), the installer, the update check and download, "Restart to update" and the install on Quit. That `Updates.Start` hands a pending update to `Update.exe` and the pet comes back ([Updates.cs:73](../src/AiPet.UI/Updates.cs)), and that a second launch with an update pending leaves the running pet alone. That the uninstall removes the hooks ([HookCleanup.cs:66](../src/AiPet.Core/HookCleanup.cs)). That the portable zip and `dotnet AiPet.dll` write no `velopack.log`, while the installed copy keeps Velopack's locator and its log.
- **The NativeAOT win-x64 resources.** The icon and version resource in `aipet-hook.exe`, and both exes' versions as Windows reads them. release.yml's "Check the exes' version resources" step ([release.yml:151](../.github/workflows/release.yml)) and ResourceTests on windows-2022 check them.
- **The pipe server.** The silent-client cut-off that repeats `CancelIoEx` every `Ipc.TimeoutMs` ([HookServer.cs:243](../src/AiPet.Core/HookServer.cs)) is only reasoned about. The starved thread-pool test runs on windows-2022 in CI, but hasn't been seen to run there.
- **The Linux release jobs.** The builds in Microsoft's cross-build containers (pinned by digest), the arm64 hook cross-compiled on x64, and the guard that fails when the hook needs a glibc newer than 2.27 ([release.yml:282](../.github/workflows/release.yml)). The guard was only tried on a local build, where it failed as it should.
- **The deploy-key check** in the check job ([release.yml:77](../.github/workflows/release.yml)), tested only with a stand-in ssh, and the plugin job's push.
- **CI on windows-2022.** check-plugin.sh with Git Bash, jq and diff, and the tests that only check something on Windows (such as `InstallClaude_WithAPluginThatCantRun_Registers`). The executable-bit guard has only run locally.
- **install.ps1** (there's no pwsh here): removing the old `Run` values, keeping `GITHUB_TOKEN` from the pet, and the fallback without Git Bash.
- **Windows registration and UI.** The Git Bash lookup ([Install.cs:136](../src/AiPet.Hook/Install.cs)), config writes and the Codex plugin probe on Windows, a real `--install` from the NativeAOT exe, `FocusAgent` opening the chat's link when no window is found, and the quiet exit when an elevated pet holds the mutex.
- **Codex and Claude behaviour the ordering rules assume.** That a manual `/compact` has its own turn and no Stop, that `turn_aborted` carries `turn_id`, and that Claude's PermissionRequest has the same command or file path as its PreToolUse. Real traffic hasn't shown these yet.

**Known limits:**
- On Hyprland the transparent parts inside the fitted window (around the pet, between the bubbles) still take the mouse, because Hyprland ignores the X11 input shape. Outside the window everything clicks through.
- Tooltips drawn in the fitted window can be cut off at its edges.
- On Windows, a pet whose listeners only partly start logs "can't listen" although it serves hooks (older than this work; Linux logs it correctly).

## Next steps

1. **Watch the first release (0.1.0)** and its CI run: they exercise what can't run here (the Windows and CI items above). The plugin repository, its deploy key and the `PLUGIN_DEPLOY_KEY` secret are in place.
2. **Small leftovers:**
   - Add the test that `packaging/linux/install.sh` creates a new data folder with mode 0700 and leaves an existing one's mode alone. It belongs in PackageInstallerTests, before `Uninstall_RemovesTheHooksRegisteredAtTheAppsHook` ([InstallerTests.cs](../tests/AiPet.Tests/InstallerTests.cs)).
   - release.yml's header says the check job checks that "the plugin files are consistent" ([release.yml:6](../.github/workflows/release.yml)). It checks the manifests and marketplaces, and the hook files only as JSON; the plugin job compares the hook files with what the released Linux hook prints.
   - The Windows job's `vpk download github` fetches the latest release for the delta package; releasing a version that isn't newer than it (a hotfix number after a newer release) makes `vpk pack` refuse. Download it only when the new version is newer.
   - Try "Import defaults…" (the file picker) and the ABOUT card in the GUI.
   - If branch protection on main requires ci.yml's old "Plugin files" check, replace it with "Build (windows-2022)" and "Build (ubuntu-22.04)".
3. **A native Wayland front end for the pet?** Hyprland ignores X11 input shapes, so on Linux the pet's window is fitted to what it shows, but the transparent parts inside it still take clicks. Only layer-shell (Hyprland, Sway, KDE; not GNOME) gives exact input regions, placement and always-on-top without window rules, and Avalonia's Wayland backend has no layer-shell. A research report compares GTK4 + gtk4-layer-shell, Qt 6 + LayerShellQt, Rust options and hybrids (keeping Avalonia on Windows, macOS and for Settings); decide from it, then spike a minimal layer-shell pet on Hyprland first.
4. **Before going public:** squash the history, and report the false positives to Microsoft and AhnLab.
5. **Then the next-meeting bubble** (Outlook calendar, optionally Teams' local API for live meeting state), planned and built with flow-next.

## Decisions

- **Windows antivirus (Trend Micro) reacts to PowerShell and to freshly built `.exe` files.**
  - Build locally with `dotnet build -p:UseAppHost=false`, run programs with `dotnet <dll>`, and leave publishing and NativeAOT to GitHub Actions.
- **Codex on Windows keeps its command hooks,** even though Codex runs them through PowerShell (that's Codex's choice).
  - No MCP server and no watcher-only mode.
  - Don't change the registered command strings: Codex would ask users to trust the hooks again.
  - `--doctor codex --probe` stays as it is for direct hooks.
- **Claude's hooks use exec form** (`command` + `args`, no shell). The plugin uses `sh` with `"shell": "bash"`, so Claude never falls back to PowerShell.
- **No autostart at sign-in, and hooks never start the pet.** Users start the pet themselves. The hook never writes to stdout on the hook path, always exits 0, and writes and starts nothing when the pet isn't running.
- **Never rename the marketplace or plugin `aipet`.** `aipet@aipet` is part of every Codex trust key. Codex hook definitions are frozen after the first release.
- **The plugin repository `xMarcinator/ai-pet-plugin` is separate,** and only CI writes to it.
- **Velopack's packId is `AiPetApp`, never `AiPet`:** `%LOCALAPPDATA%\AiPet` is the data folder. Velopack's auto-apply stays off, because `VelopackApp.Run` comes before the single-instance mutex, where a second launch would have `Update.exe` stop the running pet. `Updates.Start` installs a pending update instead ([Updates.cs:43](../src/AiPet.UI/Updates.cs)).
- **Hyprland window rules are shipped as files** (`packaging/linux/hyprland/aipet.lua` and `aipet.conf`) that users include themselves. The installers print the include line and never edit the config. The uninstall removes that line again, after a backup.
- **No override-redirect pet window, and never `no_focus`.** Hyprland didn't follow an override-redirect window's own moves (dragging broke) and still blurred it. An XWayland window with `no_focus` gets no mouse input at all.
- **On Linux the pet's window is fitted to what it shows,** because a compositor that ignores the X11 input shape (Hyprland) would let no click through the whole 380x600 window. `config.json` still stores the full window's position. `AIPET_NOFIT=1` turns fitting off ([LinuxPlatform.cs:95](../src/AiPet.UI/Platform/LinuxPlatform.cs)).
- **Overlay popups on X11.** Menus and tooltips are drawn inside the pet's window, so no compositor can stack the pet above them and focus following the mouse can't close them. So the menu opens above the pet and has no submenu.
- **The Linux hook is built for glibc 2.27,** in Microsoft's cross-build containers against their Ubuntu 18.04 root file system, and the release fails if it needs anything newer. Linked on a runner, it would need the runner's much newer glibc, and older distributions would refuse it on every event.
- **Commits use this identity:** `xMarcinator <25668644+xMarcinator@users.noreply.github.com>`, set in the repository-local git config. It isn't set on this machine yet, where the global identity is a different one: set `user.name` and `user.email` with `git config` in the repository before committing.

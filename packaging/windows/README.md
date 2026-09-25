# Windows packaging

The `win` job of [`.github/workflows/release.yml`](../../.github/workflows/release.yml) builds the Windows packages.
Nothing in this folder runs during a release. This page describes what the job makes and how, what the app does
with Velopack, and how to check a package by hand.

## What a release contains

| File | What it is |
|---|---|
| `AiPet-Setup-<version>-win-x64.exe` | The Velopack installer. It installs for the current user, with no admin rights. The one-line installer (`install.ps1`) runs it with `--silent`. |
| `AiPet-<version>-win-x64.zip` | The portable app: the app folder, zipped. It doesn't update itself and makes no shortcuts. |
| `AiPetApp-<version>-full.nupkg` | Velopack's full update package. |
| `AiPetApp-<version>-delta.nupkg` | Velopack's delta from the previous release. It's missing from the first release. |
| `releases.win.json` | Velopack's update feed for the `win` channel. It lists this release's packages. |

`SHA256SUMS` covers these files and the Linux tarballs.

## How the job builds them

1. It publishes the app self-contained, so users need no .NET:
   `dotnet publish src/AiPet.UI/AiPet.UI.csproj -c Release -r win-x64 --self-contained -p:Version=<version>`.
2. It publishes the hook with NativeAOT:
   `dotnet publish src/AiPet.Hook/AiPet.Hook.csproj -c Release -r win-x64 -p:PublishAot=true -p:Version=<version>`.
   - The hook runs on every agent event, so it has to start in milliseconds.
   - The job fails rather than fall back to a managed build, as `build.ps1` does.
   - NativeAOT needs the Visual Studio C++ tools. The `windows-2022` runner has them.
3. It checks both exes' version resources (release.yml:151, and [below](#version-resources)).
4. It copies `aipet-hook.exe` next to `AiPet.exe`, adds `LICENSE` and `THIRD-PARTY-NOTICES.md`, and deletes every
   `*.pdb`. The native libraries' symbols alone come to about 100 MiB.
5. It runs Velopack's CLI, `vpk`, at the version set by `VPK_VERSION` in the workflow (release.yml:47):

   ```
   dotnet tool install vpk --version <VPK_VERSION> --tool-path out/tools
   vpk download github --repoUrl https://github.com/xMarcinator/ai-pet --token <token> --outputDir out/velopack
   vpk pack --packId AiPetApp --packVersion <version> --packDir out/app --mainExe AiPet.exe --packTitle AiPet
            --packAuthors xMarcinator --icon assets/icon/aipet.ico --channel win --shortcuts StartMenuRoot
            --noPortable --outputDir out/velopack
   ```

   `download github` fetches the latest release's full package, so `pack` can make a delta from it. Before the first
   release there's nothing to fetch, and the job carries on without a delta. `pack` checks that the app's `Main`
   calls `VelopackApp.Build().Run()`, and the job never skips that check.
6. It renames Velopack's `*-Setup.exe` to `AiPet-Setup-<version>-win-x64.exe`. Setup embeds its package, so the
   file name doesn't matter.
7. It cuts `releases.win.json` down to this version's packages, as `vpk upload github` does. Earlier packages stay
   in their own releases.
8. It zips `out/app` into the portable zip.

The `publish` job uploads all of these to the GitHub release with `gh release create`. It doesn't use
`vpk upload`, so that one job creates the release, only after every build has passed. Velopack's `RELEASES` and
`assets.win.json` aren't uploaded:
- `RELEASES` only serves apps migrating from Squirrel, and AiPet never used Squirrel.
- `assets.win.json` is the list that `vpk upload` reads.

## Choices

- **packId `AiPetApp`, and never change it.**
  - Velopack installs to `%LOCALAPPDATA%\<packId>`, and `%LOCALAPPDATA%\AiPet` is already the app's data folder
    (settings and logs, `Paths.cs`).
  - The packId is also the app's identity for updates, so a new one would make a second installation.
- **Install layout.**
  - The app is in `%LOCALAPPDATA%\AiPetApp\current\` (`AiPet.exe`, `aipet-hook.exe` and the runtime).
  - Velopack also installs `Update.exe`, and a stub `AiPet.exe` in `%LOCALAPPDATA%\AiPetApp\` that starts
    `current\AiPet.exe`.
  - The `current` path stays the same across updates. That matters for `aipet-hook --install`, whose registration
    (and, for Codex, its trust) includes the exe's path.
- **Shortcuts: the Start menu only** (`StartMenuRoot`). There's no desktop shortcut and nothing in Startup: AiPet
  doesn't start at sign-in.
- **Plain portable zip.** Velopack's own portable zip is turned off (`--noPortable`). The zip is the app folder
  without `Update.exe` above it, so the app treats it as not installed and never updates it.
- **Unsigned.** SmartScreen warns about the installer and the exes. Signing can be added later with `vpk pack`'s
  signing options.
- **Velopack's package and CLI move together.** The app references the `Velopack` package
  ([AiPet.UI.csproj](../../src/AiPet.UI/AiPet.UI.csproj):26) at the same version as `VPK_VERSION`.
  `VelopackTests.VpkVersion_IsThePackageVersion` fails when they differ.

## Velopack in the app

[Updates.cs](../../src/AiPet.UI/Updates.cs) holds all of it.

- **Startup.** `Program.Main` calls `Updates.App().Run()` before anything else
  ([Program.cs](../../src/AiPet.UI/Program.cs):13). The installer, updater and uninstaller start `AiPet.exe` with
  `--veloapp-*` arguments; `Run` handles those and exits. Any other start goes on at once.
- **Only the installed copy uses Velopack's locator.** A copy counts as installed when `Update.exe` is in the folder
  above the app's (`UpdateStatus.Installed`, [UpdateStatus.cs](../../src/AiPet.Core/UpdateStatus.cs):22). The
  portable zip, `dotnet AiPet.dll` and Linux get a locator that finds nothing
  ([Updates.cs](../../src/AiPet.UI/Updates.cs):49-50), so they never check for updates.
- **The uninstall hook.** Before Velopack removes the app, its uninstall callback runs
  `HookCleanup.Run(<app folder>\aipet-hook.exe)` ([Updates.cs](../../src/AiPet.UI/Updates.cs):48,
  [HookCleanup.cs](../../src/AiPet.Core/HookCleanup.cs):66). `install.ps1` registers the hook directly
  (`aipet-hook --install claude`) when Claude Code's plugin can't run for lack of Git Bash, and that registration
  names the installed exe. For each agent whose config names this install's hook, the callback runs
  `aipet-hook --uninstall claude|codex`, with no window and at most 10 seconds each, and logs the result to
  `aipet.log`. It matches the path with `/` or `\`, JSON and TOML escaping, and the 8.3 short form Codex's command
  uses for a path with spaces. Registrations of another copy (the portable zip, a build from source) and the
  plugins are left alone.
- **Updates.** The installed app checks
  `GithubSource("https://github.com/xMarcinator/ai-pet")` on the `win` channel (stable releases only, no token):
  3 minutes after it starts, then every 6 hours ([UpdateStatus.cs](../../src/AiPet.Core/UpdateStatus.cs):11-17). A
  new version downloads in the background. Settings > General shows the version and the state, with
  **Check for updates**.
  - **Quit** in the pet's menu installs a downloaded update once the pet has exited, without a window, and the pet
    stays closed ([MainWindow.axaml.cs](../../src/AiPet.UI/MainWindow.axaml.cs):256).
  - **Restart to update** in Settings installs it and starts the pet again
    ([SettingsWindow.axaml.cs](../../src/AiPet.UI/SettingsWindow.axaml.cs):133).
  - Velopack's own install-on-start is off (`SetAutoApplyOnStartup(false)`), because `Run` comes before the
    single-instance check: in a second copy, `Update.exe` would stop the running pet. Instead, once `Main` knows it
    is the only pet, `Updates.Start` installs an update that was downloaded but never installed, and `Main` exits so
    that `Update.exe` can start the pet again ([Program.cs](../../src/AiPet.UI/Program.cs):25,
    [Updates.cs](../../src/AiPet.UI/Updates.cs):83-88). The start that follows an update never does this, so a
    failed update can't loop; Settings then shows the update as ready.
  - A failed check or download is only a state Settings shows, and a line in `aipet.log`.

## Version resources

Both exes carry the icon and a version resource that names the exe. The compiler's own version resource would name
the `.dll`, and antivirus heuristics dislike an `.exe` that says it's a `.dll`.

- [build/Win32Resources.targets](../../build/Win32Resources.targets) writes the `.res` file itself, on every OS and
  without `rc.exe`, before the compiler runs, and gives the compiler only that. It holds the icons from
  `ApplicationIcon`, the manifest (the project's, or `build/default.win32manifest` for the hook), and a version
  resource. The version resource has `CompanyName`, `FileDescription` (the project's `AssemblyTitle`),
  `FileVersion`, `InternalName` and `OriginalFilename` (`AiPet.exe`, `aipet-hook.exe`), `LegalCopyright`,
  `ProductName` (`AiPet`) and `ProductVersion` (the version as released).
- Both projects set `AssemblyTitle` and `ApplicationIcon` and import the targets
  ([AiPet.UI.csproj](../../src/AiPet.UI/AiPet.UI.csproj):11-14, 33;
  [AiPet.Hook.csproj](../../src/AiPet.Hook/AiPet.Hook.csproj):11-13, 26).
- The versions come from `-p:Version` through [Directory.Build.props](../../Directory.Build.props): with
  `-p:Version=1.2.3-rc.4`, `FileVersion` is `1.2.3.0` and `ProductVersion` is `1.2.3-rc.4`.
- The SDK copies the dll's resources into the apphost (`AiPet.exe`), and NativeAOT into the native exe
  (`aipet-hook.exe`). The release's check step reads them as Windows does and fails unless each exe's
  `OriginalFilename` and `InternalName` are its own name, `ProductName` is `AiPet` and `ProductVersion` is the
  release's version.
- `ResourceTests` checks the same fields in the built dlls on every OS.

## Checking a package by hand

On a test machine or a Windows Sandbox:

- Run `AiPet-Setup-<version>-win-x64.exe --silent --log setup.log`. Check the result:
  - the app is in `%LOCALAPPDATA%\AiPetApp\current`, with `aipet-hook.exe` next to `AiPet.exe`;
  - the Start menu has an **AiPet** entry;
  - Settings > Apps lists AiPet;
  - nothing is in `shell:startup`;
  - the properties of both exes show their version.
- Run the same Setup again, with the pet running. It should replace the installation.
- Register the hook directly (`%LOCALAPPDATA%\AiPetApp\current\aipet-hook.exe --install claude`), then uninstall
  AiPet from Settings > Apps. `%LOCALAPPDATA%\AiPetApp` should go, the hooks should be gone from
  `~/.claude/settings.json` (`aipet.log` says what the uninstaller did), and `%LOCALAPPDATA%\AiPet` should stay.
- With an older release installed, Settings > General should find the newer one, download it and offer
  **Restart to update**.

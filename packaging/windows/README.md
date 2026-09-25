# Windows packaging

The `win` job of [`.github/workflows/release.yml`](../../.github/workflows/release.yml) builds the Windows packages.
Nothing in this folder runs during a release. This page describes what the job makes and how, for when you change
it or rebuild a package by hand.

## What a release contains

| File | What it is |
|---|---|
| `AiPet-Setup-<version>-win-x64.exe` | The Velopack installer. It installs for the current user, with no admin rights. The one-line installer (`install.ps1`) runs it with `--silent`. |
| `AiPet-<version>-win-x64.zip` | The portable app: the app folder, zipped. It has no updater and makes no shortcuts. |
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
3. It copies `aipet-hook.exe` next to `AiPet.exe` and deletes every `*.pdb`. The native libraries' symbols alone
   come to about 100 MiB.
4. It runs Velopack's CLI, `vpk`, at the version set by `VPK_VERSION` in the workflow:

   ```
   dotnet tool install vpk --version <VPK_VERSION> --tool-path out/tools
   vpk download github --repoUrl https://github.com/xMarcinator/ai-pet --token <token> --outputDir out/velopack
   vpk pack --packId AiPetApp --packVersion <version> --packDir out/app --mainExe AiPet.exe --packTitle AiPet
            --packAuthors xMarcinator --icon assets/icon/aipet.ico --channel win --shortcuts StartMenuRoot
            --noPortable --outputDir out/velopack
   ```

   `download github` fetches the latest release's full package, so `pack` can make a delta from it. Before the first
   release there's nothing to fetch, and the job carries on without a delta.
5. It renames Velopack's `*-Setup.exe` to `AiPet-Setup-<version>-win-x64.exe`. Setup embeds its package, so the
   file name doesn't matter.
6. It cuts `releases.win.json` down to this version's packages, as `vpk upload github` does. Earlier packages stay
   in their own releases.
7. It zips `out/app` into the portable zip.

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
  - The `current` path stays the same across updates. That matters for `aipet-hook --install codex`, whose Codex
    trust includes the exe's path.
- **Shortcuts: the Start menu only** (`StartMenuRoot`). There's no desktop shortcut and nothing in Startup: AiPet
  doesn't start at sign-in.
- **Plain portable zip.** Velopack's own portable zip is turned off (`--noPortable`). Without the Velopack
  integration in the app (below), it would add nothing but an indirection.
- **Unsigned.** SmartScreen warns about the installer and the exes. Signing can be added later with `vpk pack`'s
  signing options.

## Still to do in the app

- **Call `VelopackApp.Build().Run()` first thing in `Main`.**
  - This needs the `Velopack` NuGet package, at the same version as `VPK_VERSION`.
  - Velopack runs `AiPet.exe` with `--veloapp-install`, `--veloapp-updated` and `--veloapp-uninstall` while it
    installs, updates and removes the app, and waits up to 30 seconds for it to exit.
  - Without that call the pet starts instead, and Velopack kills it when the time runs out. Setup then takes
    30 seconds longer, and the pet shows up briefly while it runs.
- **Check for updates in the app.** `UpdateManager` with `GithubSource("https://github.com/xMarcinator/ai-pet")`
  reads `releases.win.json` from the latest release. Until the app does this, running the one-line installer again
  is how users update: Setup replaces the installed version, stopping the running pet first.

## Checking a package by hand

On a test machine or a Windows Sandbox:

- Run `AiPet-Setup-<version>-win-x64.exe --silent --log setup.log`. Check the result:
  - the app is in `%LOCALAPPDATA%\AiPetApp\current`, with `aipet-hook.exe` next to `AiPet.exe`;
  - the Start menu has an **AiPet** entry;
  - Settings > Apps lists AiPet;
  - nothing is in `shell:startup`.
- Run the same Setup again, with the pet running. It should replace the installation.
- Uninstall AiPet from Settings > Apps. `%LOCALAPPDATA%\AiPetApp` should go, and `%LOCALAPPDATA%\AiPet` should stay.

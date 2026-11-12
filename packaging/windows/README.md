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

1. It builds the pet, `AiPet.exe` (`rust/crates/aipet`), and the hook, `aipet-hook.exe` (`rust/crates/aipet-hook`),
   with cargo in `rust/`:
   `cargo build -p aipet -p aipet-hook --release --locked --target x86_64-pc-windows-msvc`, with
   `AIPET_VERSION=<version>` and `RUSTFLAGS=-C target-feature=+crt-static`. Users need no runtime of any kind.
   - The hook runs on every agent event, so it has to start in milliseconds.
   - `+crt-static` links the C runtime in, so both exes need only Windows' own DLLs, not the Visual C++ runtime's,
     which a Windows install may lack. The step fails if either exe names a C runtime DLL (`vcruntime*.dll`,
     `msvcp*.dll`, `api-ms-win-crt-*`).
   - Their build scripts embed the icon, the manifest and a version resource with the release's version
     ([below](#version-resources)).
   - It needs the Visual Studio C++ tools and the Windows SDK's `rc.exe`. The `windows-2022` runner has them.
2. It checks both exes' version resources (the step "Check the exes' version resources", and
   [below](#version-resources)).
3. It copies `aipet-hook.exe` next to `AiPet.exe`, and adds `LICENSE` and `THIRD-PARTY-NOTICES.md`. Only the exes
   are copied out of cargo's target folder, so no `.pdb` goes in.
4. It installs .NET, for `vpk` alone (a .NET tool), and runs Velopack's CLI at the version set by `VPK_VERSION` in
   the workflow:

   ```
   dotnet tool install vpk --version <VPK_VERSION> --tool-path out/tools
   vpk download github --repoUrl https://github.com/xMarcinator/ai-pet --token <token> --outputDir out/velopack
   vpk pack --packId AiPetApp --packVersion <version> --packDir out/app --mainExe AiPet.exe --packTitle AiPet
            --packAuthors xMarcinator --icon assets/icon/aipet.ico --channel win --shortcuts StartMenuRoot
            --noPortable --outputDir out/velopack
   ```

   `download github` fetches the latest release's full package, so `pack` can make a delta from it. Before the first
   release there's nothing to fetch, and the job carries on without a delta. `pack` checks that a .NET app's `Main`
   calls Velopack's startup, but not a native app's: the pet's own tests hold that its `main` runs it first.
5. It renames Velopack's `*-Setup.exe` to `AiPet-Setup-<version>-win-x64.exe`. Setup embeds its package, so the
   file name doesn't matter.
6. It cuts `releases.win.json` down to this version's packages, as `vpk upload github` does. Earlier packages stay
   in their own releases.
7. It zips `out/app` into the portable zip.

The `plugin` job writes `SHA256SUMS` over these files and the Linux tarballs before it pushes the plugin. The
`publish` job checks the files against it, then uploads them and `SHA256SUMS` to the GitHub release with
`gh release create`. It doesn't use `vpk upload`, so that one job creates the release, only after every build has
passed. Velopack's `RELEASES` and `assets.win.json` aren't uploaded:
- `RELEASES` only serves apps migrating from Squirrel, and AiPet never used Squirrel.
- `assets.win.json` is the list that `vpk upload` reads.

## Choices

- **packId `AiPetApp`, and never change it.**
  - Velopack installs to `%LOCALAPPDATA%\<packId>`, and `%LOCALAPPDATA%\AiPet` is already the app's data folder
    (settings and logs, `rust/crates/aipet-ipc/src/paths.rs`).
  - The packId is also the app's identity for updates, so a new one would make a second installation.
- **Install layout.**
  - The app is in `%LOCALAPPDATA%\AiPetApp\current\` (`AiPet.exe`, `aipet-hook.exe`, `LICENSE` and
    `THIRD-PARTY-NOTICES.md`).
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
- **Velopack's crate and CLI move together.** The pet depends on the `velopack` crate
  (`rust/crates/aipet/Cargo.toml`) at exactly the version of `VPK_VERSION`.
  `rust/crates/aipet-testkit/tests/velopack.rs::vpk_version_is_the_package_version` fails when they differ.

## Velopack in the app

[rust/crates/aipet/src/updates.rs](../../rust/crates/aipet/src/updates.rs) holds all of it, through Velopack's Rust
SDK.

- **Startup.** The pet's `main` calls `updates::startup()` before anything else. The installer, updater and
  uninstaller start `AiPet.exe` with arguments of their own; Velopack handles those and exits. Any other start goes on
  at once.
- **Only the installed copy uses Velopack's locator.** A copy counts as installed when `Update.exe` is in the folder
  above the app's. The portable zip, a build from source and Linux get a locator that finds nothing, so they never
  check for updates.
- **The uninstall hook.** Before Velopack removes the app, its uninstall callback runs the hook cleanup
  ([rust/crates/aipet-core/src/cleanup.rs](../../rust/crates/aipet-core/src/cleanup.rs)) for `<app folder>\aipet-hook.exe`.
  `install.ps1` registers the hook directly (`aipet-hook --install claude`) when Claude Code's plugin can't run for
  lack of Git Bash, and that registration names the installed exe. For each agent whose config names this install's
  hook, the callback runs `aipet-hook --uninstall claude|codex`, with no window and at most 10 seconds each, and logs
  the result to `aipet.log`. It matches the path with `/` or `\`, JSON and TOML escaping, and the 8.3 short form
  Codex's command uses for a path with spaces. Registrations of another copy (the portable zip, a build from source)
  and the plugins are left alone.
- **Updates.** The installed app checks the GitHub releases of `https://github.com/xMarcinator/ai-pet` on the `win`
  channel (stable releases only, no token): 3 minutes after it starts, then every 6 hours
  (`rust/crates/aipet-core/src/update_status.rs`). A new version downloads in the background. Settings > General shows
  the version and the state, with **Check for updates**.
  - **Quit** in the pet's menu installs a downloaded update once the pet has exited, without a window, and the pet
    stays closed (`updates::install_on_quit`).
  - **Restart to update** in Settings installs it and starts the pet again (`updates::restart`).
  - Velopack's own install-on-start is off (`set_auto_apply_on_startup(false)`), because its startup comes before the
    single-instance check: in a second copy, `Update.exe` would stop the running pet. Instead, once the pet knows it
    is the only one, `updates::start` installs an update that was downloaded but never installed, and the pet exits
    so that `Update.exe` can start it again. The start that follows an update never does this, so a failed update
    can't loop; Settings then shows the update as ready.
  - A check that gets no answer for 5 minutes, or a download that stalls for 10, is given up on. A failed check or
    download is only a state Settings shows, and a line in `aipet.log`.
- **The proof.** [velopack-proof.yml](../../.github/workflows/velopack-proof.yml), run by hand, installs a Rust pet
  built as the release builds it, has it update itself from a local feed, and uninstalls it
  ([rust/proofs/velopack.md](../../rust/proofs/velopack.md) also records the update from the .NET releases).

## Version resources

Both exes carry the icon and a version resource that names the exe: a Rust exe has none of its own, and antivirus
heuristics dislike an `.exe` without one.

- Their build scripts ([rust/crates/aipet/build.rs](../../rust/crates/aipet/build.rs) and
  [rust/crates/aipet-hook/build.rs](../../rust/crates/aipet-hook/build.rs)) give `AiPet.exe` and `aipet-hook.exe`
  the resources the .NET exes had: the icon, the manifest (the pet's `app.manifest`, and
  `build/default.win32manifest` for the hook) and a version resource with `CompanyName`, `FileDescription`,
  `FileVersion`, `InternalName` and `OriginalFilename` (`AiPet.exe`, `aipet-hook.exe`), `LegalCopyright`,
  `ProductName` (`AiPet`) and `ProductVersion` (the version as released). They compile them with the Windows SDK's
  `rc.exe`, through the `winresource` crate, and cargo links them into the exes.
- The version is `AIPET_VERSION`, which the release sets to the version it releases (`0.0.0-dev` without it). With
  `1.2.3-rc.4`, `FileVersion` is `1.2.3.0` and `ProductVersion` is `1.2.3-rc.4`.
- The release's check step reads them as Windows does and fails unless each exe's `OriginalFilename` and
  `InternalName` are its own name, `ProductName` is `AiPet` and `ProductVersion` is the release's version.
- `rust/crates/aipet-testkit/tests/resources.rs` checks them on Windows against the values the .NET exes had, byte for
  byte.

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

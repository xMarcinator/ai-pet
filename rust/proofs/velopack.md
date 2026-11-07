# Velopack proof: a .NET install updates in place to a Rust release

Scope: whether a Windows install of the .NET AiPet (Velopack packId `AiPetApp`) updates in place to a release whose
`AiPet.exe` is a Rust program, by a full package and by a delta, and whether the Rust app's uninstall hook runs. This
resolves the spec's parked unknown about the update path (R14, task fn-1-migrate-aipet-from-net-to-rust.14).

The harness lives in `.github/workflows/velopack-proof.yml` and `rust/notes/prototypes/velopack-proof/`:
- `src/main.rs`: the Rust stand-in, a GUI exe named `AiPet.exe`. It calls `VelopackApp::build().run()` first and
  appends one line per event to `%LOCALAPPDATA%\AiPet\velopack-proof\<event>.txt`.
- `driver/Program.cs`: the update driver. It updates the install with the .NET pet's own Velopack library and calls
  (`src/AiPet.UI/Updates.cs`), with a local feed in place of the GitHub releases.
- `proof.ps1`: the Windows steps and checks. It runs by hand in Windows Sandbox too.

## Verdict

An installed .NET 0.1.x release updates in place to a Rust-packed release. Both the full package and a delta across
the runtimes work, so the first Rust release needs no one-time reinstall and no special packaging. The data folder
stays byte-identical, and the Rust app's `--veloapp-updated` and `--veloapp-uninstall` hooks run.

| | Full job | Delta job |
|---|---|---|
| What the .NET client chose | target `0.1.99-full`, base none, deltas none | base `0.1.98-full` (Setup keeps it in `packages\`) plus the `0.1.99` delta |
| Download | "Full release download complete" | "Delta update download complete", with no full file in the feed to fall back to |
| `current\AiPet.exe` after the update | the Rust stand-in (SHA-256 `FA41B5C1...FD94`), `AiPet.dll` gone | the same |
| `current\sq.version` | 0.1.99 | 0.1.99 |
| Rust hooks | `--veloapp-updated 0.1.99` once, then `restarted` | the same |
| Start menu entry | starts `current\AiPet.exe` (Rust) | the same |
| Uninstall | `--veloapp-uninstall 0.1.99` ran, `current\` and the Settings > Apps entry gone | the same |
| Data folder (all but the stand-in's notes) | byte-identical after the update and after the uninstall | the same |

Run: https://github.com/xMarcinator/ai-pet/actions/runs/36477504598 (2026-09-28, commit `e0bdff0` on branch
`proof/velopack`), conclusion success for all three jobs. Its artifacts `velopack-proof`,
`velopack-proof-logs-full` and `velopack-proof-logs-delta` expire after 7 days. The lines quoted here come from
`gh run view 36477504598 --log`.

## Method

The pack job runs on windows-2022 with vpk 1.2.158, the version `release.yml` pins as `VPK_VERSION`. A step fails the
job unless the UI csproj, the driver csproj and the stand-in's `Cargo.toml` all name that Velopack version.

1. It publishes the .NET app as 0.1.98 the way `release.yml`'s win job does (self-contained app plus the NativeAOT
   hook), and packs it with `release.yml`'s `vpk pack` arguments (`--packId AiPetApp --channel win --mainExe AiPet.exe
   --shortcuts StartMenuRoot --noPortable`) into a local feed. It keeps that Setup.exe.
2. It builds the Rust stand-in and packs it as 0.1.99 with the same arguments into the same feed. vpk makes the delta
   from the newest full package it finds there, the .NET 0.1.98 one.
3. It builds the update driver.

Each update job starts on a fresh windows-2022 runner:

1. `install` seeds `%LOCALAPPDATA%\AiPet` with the files the pet keeps there (`config.json`, `jira.json`,
   `github.json`, `aipet.log`, `hook-events.log`, `avatars\`), hashes them, and runs the 0.1.98 Setup with
   `--silent`.
2. `update` builds a feed that lists only the 0.1.99 packages, as each release's `releases.win.json` does. The full
   job's feed lists and holds only the full package. The delta job's feed lists both but holds only the delta's file,
   so a delta the client refuses or can't apply fails the job instead of falling back to the full package. The
   driver then runs `UpdateManager` with `WindowsVelopackLocator` on the installed `AiPet.exe`, `SimpleFileSource`
   and channel `win`: `CheckForUpdatesAsync`, `DownloadUpdatesAsync`, then
   `WaitExitThenApplyUpdates(silent: true, restart: true)`.
3. `check` compares `current\AiPet.exe` with the stand-in's hash, reads `sq.version`, the stand-in's notes and the
   data folder's hashes.
4. `restart` starts the app through its Start menu entry, which Setup pointed at `current\AiPet.exe`.
5. `uninstall` runs the Settings > Apps `UninstallString` (`Update.exe --uninstall`) with `--silent`.

The driver uses the .NET library because the real update is performed by the installed .NET pet, whose client picks
between the full package and the deltas. A Rust or `Update.exe`-only driver would not show what that client does.

## Findings

**Delta across runtimes.** vpk reported "Delta processed 0004 files. 0003 patched, 0001 unchanged, 0000 new, 0221
removed". The .NET client downloaded and verified the delta, and `Update.exe` patched `AiPet.exe`,
`AiPet_ExecutionStub.exe` and `sq.version` with zsdiff and deleted the 221 .NET files, in 1.6 s.

| Package | Bytes |
|---|---|
| `AiPetApp-0.1.98-full.nupkg` (.NET) | 52,508,947 |
| `AiPetApp-0.1.99-full.nupkg` (Rust) | 2,660,151 |
| `AiPetApp-0.1.99-delta.nupkg` | 584,265 |
| Rust `AiPet.exe` | 1,129,472 |

The delta is 22 % of the Rust full package. The full package rebuilt from the delta is 2,660,143 bytes, 8 bytes
shorter than vpk's, because `Update.exe` assembles its own zip from the patched files. The app files inside match: `current\AiPet.exe` has the
stand-in's hash in both jobs.

**Hooks across runtimes.** Setup ran `--veloapp-install` on the .NET app. At the update, `Update.exe` ran
`--veloapp-obsolete` on the old .NET `AiPet.exe` (the .NET library's log shows "Found fast exit hook"), swapped
`current\`, and ran `--veloapp-updated 0.1.99` on the Rust exe. It then started the Rust exe, which Velopack reported
as `restarted`. An ordinary start afterwards was not reported as a restart. The uninstaller ran
`--veloapp-uninstall 0.1.99` on the Rust exe, removed the shortcut, the install folder and the registry keys, and
left the data folder alone.

**vpk and native exes.** vpk's check that `Main` calls `VelopackApp.Run` ("Verified VelopackApp.Run()" in the .NET
pack) runs only for .NET exes. The Rust pack printed no such line and needed no `--skipVeloAppCheck`. Nothing in the
toolchain checks that the Rust pet runs Velopack first, so the Rust pet needs its own test of that order.

**Install layout.** The install folder, the Start menu entry, the Settings > Apps entry (`AiPet`, now 0.1.99), the
launcher `AiPetApp\AiPet.exe` (re-extracted from the new package's execution stub) and `Update.exe` 1.2.158 all stayed
where they were.

## What later tasks take from this

- **Task fn-1-migrate-aipet-from-net-to-rust.23 (the Rust release)** packs the Rust pet with `release.yml`'s current
  `vpk pack` arguments and keeps `vpk download github` for the delta. The first Rust release gets its delta from the
  last .NET release's full package, as any release does.
- **`aipet-hook.exe` must be in the Rust package.** An update replaces `current\` wholesale. The delta deleted the
  .NET `aipet-hook.exe` because the stand-in's package had none. Hook registrations made by `install.ps1` name
  `%LOCALAPPDATA%\AiPetApp\current\aipet-hook.exe` (`HookCleanup.cs`), so a Rust release without the hook at that path
  breaks them.
- **Task fn-1-migrate-aipet-from-net-to-rust.21 (updates and uninstall)** keeps `VelopackApp::build().run()` first,
  with `set_auto_apply_on_startup(false)` as the .NET pet has it, and runs the hook cleanup in
  `on_before_uninstall_fast_callback` with `current\aipet-hook.exe`. The proof shows that callback runs from the
  uninstaller. The task needs its own test that `run()` comes first (see above).

## Limits

- The proof checks the data folder. It doesn't exercise saved tokens or hook registrations. Both live outside the
  folders Velopack changes (Credential Manager, and the agents' own configs), and the uninstall hook that removes
  registrations ran.
- The driver applies with `silent: true` because a runner has nobody to answer a dialog. The pet's "Restart to update"
  uses `silent: false`, which only adds `Update.exe`'s progress window. The pet's other paths (install on quit, and a
  pending update installed at start) call the same `WaitExitThenApplyUpdates` and weren't run separately.
- In both jobs `Update.exe` logged "Failed to wait for process ... Access is denied. Continuing..." for the driver's
  process, and applied the package at once. The driver holds no file in the install folder, so nothing depended on
  that wait. Waiting for the old app to exit is the .NET pet's existing behaviour and doesn't depend on what the new
  package contains.
- Both packs, like `release.yml`'s, warn that no `--runtime` was given and default the package's architecture to
  x86. The proof kept that, so it says nothing about adding `--runtime win-x64` at the Rust release.
- A rollback from a Rust release to a .NET one is a new, higher-versioned release. The proof doesn't cover it (R17).

## Re-running it

Push to `proof/velopack` or start "Velopack proof" from the Actions tab. It publishes nothing and reads no release.
To run it by hand, take the `velopack-proof` artifact into Windows Sandbox and run `proof.ps1 install`, `update -Mode
full` (or `delta`), `check`, `restart`, `uninstall` and `logs` with `-Proof <artifact folder>` and `-From 0.1.98 -To
0.1.99`. The driver needs the .NET 10 runtime there.

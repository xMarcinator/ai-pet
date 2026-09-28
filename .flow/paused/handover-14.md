# fn-1-migrate-aipet-from-net-to-rust.14 - paused handover (status: in_progress)

Stopped on the conductor's pause request. The proof harness is committed and its GitHub Actions run PASSED; the only
thing left is writing the results doc (rust/proofs/velopack.md), then the Verify gates and the handover evidence.

## Where things are
- Workspace: /home/marcinator/Documents/projects/ai-pet-waves/task-14, branch wave/fn-1.14, clean tree.
- Base commit: 88b528fb14c628ff0f13d0381fa8a0652ac63454 (.flow/tmp/base_commit in the workspace).
- Task commit: e0bdff0 "feat(proof): add the Velopack proof workflow, Rust stand-in and update driver".
  No wip commit: nothing was uncommitted.
- Pushed: branch proof/velopack on origin (github.com/xMarcinator/ai-pet) = e0bdff0. Nothing else pushed.
- Run: https://github.com/xMarcinator/ai-pet/actions/runs/36477504598 (id 36477504598), conclusion success:
  "Pack the .NET and Rust releases", "Update full", "Update delta" all green. Artifacts (7-day retention):
  velopack-proof, velopack-proof-logs-full, velopack-proof-logs-delta.

## Done (in e0bdff0)
- .github/workflows/velopack-proof.yml: workflow_dispatch + push to proof/velopack (not on main). The pack job packs
  the .NET app as 0.1.98 exactly like release.yml's win job (self-contained app + NativeAOT hook, pinned vpk read
  from release.yml's VPK_VERSION, and a check that the UI csproj, driver csproj and stub Cargo.toml all name it).
  It packs the Rust stand-in as 0.1.99 with packId AiPetApp into the same feed (full + delta), then builds the
  driver. The update matrix [full, delta] runs on fresh runners: install, update, check, restart, uninstall, logs.
- rust/notes/prototypes/velopack-proof/{Cargo.toml,src/main.rs}: Rust stub bin "AiPet" (velopack =1.2.158,
  aipet-ipc for the data folder, windows_subsystem). VelopackApp::build().run() comes first. It notes start/restarted
  and the install/updated/obsolete/uninstall hooks in <data>\velopack-proof\<event>.txt.
- rust/notes/prototypes/velopack-proof/driver/: the update driver is the Velopack .NET library 1.2.158 itself. It uses
  WindowsVelopackLocator on the installed AiPet.exe via a custom IProcessImpl, SimpleFileSource instead of
  GithubSource, and the channel win. Then Check -> Download -> WaitExitThenApplyUpdates(silent, restart). This matters
  because the real .NET->Rust update is done by the installed .NET client, which drops deltas bigger than the full.
- rust/notes/prototypes/velopack-proof/proof.ps1: the Windows steps and checks (runnable by hand in Windows Sandbox).
  The delta feed withholds the full package file, so a failed or refused delta can't fall back silently.

## Results from run 36477504598 (for rust/proofs/velopack.md)
- Sizes: AiPetApp-0.1.98-full.nupkg 52,508,947 B (.NET); AiPetApp-0.1.99-full.nupkg 2,660,151 B (Rust);
  AiPetApp-0.1.99-delta.nupkg 584,265 B (delta across runtimes, far below the full, so the .NET client takes it).
- Delta job: the client chose base AiPetApp-0.1.98-full.nupkg (Setup keeps it in packages\) plus the 0.1.99 delta.
  The log says "Delta update download complete", with no full file available to fall back to.
- Full job: "base: none, deltas: none", then "Full release download complete".
- Both jobs:
  - current\AiPet.exe SHA-256 FA41B5C1...FD94 = the Rust stub, and the .NET files (AiPet.dll) are gone;
  - sq.version is 0.1.99;
  - Update.exe ran `--veloapp-updated 0.1.99` on the Rust exe, then restarted it (restarted + start notes, exe =
    %LOCALAPPDATA%\AiPetApp\current\AiPet.exe, same path as before);
  - the Start menu entry started it again;
  - the uninstaller (UninstallString + --silent) ran `--veloapp-uninstall 0.1.99`, and current\ and the Settings >
    Apps entry are gone;
  - the data folder (%LOCALAPPDATA%\AiPet) was byte-identical after the update and after the uninstall.
- Verdict to record: an installed .NET 0.1.x updates IN PLACE to a Rust-packed release, by delta or full. No
  one-time reinstall is needed. vpk packs a native exe without --skipVeloAppCheck: its VelopackApp.Run check only
  inspects .NET exes (CompatUtil.cs), so the Rust pet needs its own test that Run comes first (task 21).

## Left to do
1. Write rust/proofs/velopack.md (new) from the results above: method, run link, full vs delta outcome, the parked
   unknown resolved ("in place, delta works"). Pull the detail lines from the run logs:
   `gh run view 36477504598 --log`.
2. Commit it ("docs(proof): record the Velopack proof's results", Task: ...14).
3. Phase 5 Verify: `gate classify --base 88b528f`, then the Quick commands (cargo test/clippy/fmt in rust/; dotnet
   build + test AiPet.slnx). Baseline in a fresh worktree: cargo green; `dotnet test AiPet.slnx` alone was red (5
   ResourceTests/VelopackTests need src/AiPet.UI/bin, which a fresh worktree lacks), and green (234 passed, 1 skipped)
   after `dotnet build AiPet.slnx`, so record it as an inherited env red, fixed by building first.
4. Write the handover evidence (.flow/tmp/handover-14-evidence.json) with base_commit and commits 88b528f..HEAD.
   Return in_progress (parallel wave: no impl-review, no flowctl done).
- Optional: re-push proof/velopack if the doc or the harness changes, to re-run the proof.

stage: impl-review - skipped(policy: parallel-wave - conductor owns the gate)

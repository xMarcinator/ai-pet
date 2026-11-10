# The parity ledger

Every test of the C# suite (AiPet.Tests: 162 test attributes, 263 cases) and the Rust tests that hold the same
cases, so that nothing the C# tested was lost when the .NET code went (spec fn-1-migrate-aipet-from-net-to-rust, R16).
`scripts/check-parity.sh` checks it, and CI runs it (ci.yml's Shell scripts job). The C# was removed at the cutover
(task 25); its tests are at commit `bf3696ea4a`, and the rows stay as the record.

## Reading a row

| Column | What it holds |
|---|---|
| C# test | `Class.Method`, under the heading of its file |
| Attribute | `[Fact]`, `[Theory]`, or the platform ones (`[UnixFact]`, `[LinuxTheory]`, `[WindowsFact]`, ...) |
| C# cases | one per `[InlineData]`; 1 for a fact; a `[MemberData]` theory's rows, counted by hand |
| Rust | the Rust tests, each `<file under rust/>::<module>::<fn>` and its cases; or `n/a:` and why the test no longer applies |
| Rust cases | the sum of the Rust tests' cases |

A Rust test's cases are the rows it loops over (the C#'s `[InlineData]` became a `for` over the same list), or the
cases of the golden corpus it replays. One Rust test can hold several C# tests; per file, each counts once.

`scripts/check-parity.sh` fails when a row isn't under a file's heading, names a test twice or has no cases, when a
Rust test it names isn't a `fn` in that file, or when the Rust has fewer cases than the C#, in a row or in a file. It
prints each file's counts. Until the cutover it also held the rows to the C# tests' attributes and case counts; now it
checks the ledger against the Rust tests alone.

## Where the Rust tests run

- **In-module and crate tests** (`src/**` `#[cfg(test)]`, `crates/*/tests/`): each makes folders of its own and sets
  the variables it reads (`AIPET_PIPE`, `AIPET_DATA_DIR`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `TMPDIR`) on the processes it
  starts.
- **The black-box suites** (`crates/aipet-testkit/tests/`): the C#'s `TestEnv` and `Scripts.Sandbox`, ported as
  `aipet_testkit::Sandbox`. A program run in it gets fresh folders for HOME, the XDG folders, the agents' config
  folders, the data and temp folders, an endpoint no pet listens on, and a PATH of the test's stand-ins before an
  allow-list of the system's tools; nothing else of the user's environment. The testkit's own tests
  (`tests/sandbox.rs`) assert that. The suites run the hook built with the tests (`cargo test --workspace` builds
  it).
- **Skipped by platform**: `#[cfg_attr(<not this platform>, ignore = "why")]`, so cargo reports them as ignored, as
  xUnit reported the C#'s `Skip`.
- **Golden replays**: the golden generator ran the C#'s own code and wrote corpora under `crates/*/tests/golden/`,
  which the Rust replays. They are frozen since the cutover (each folder's README.md); until then CI also had the C#
  write them again on the runner's OS and replayed that, which the Linux corpora written at the cutover now hold.

## Deviations

Where the Rust holds a case differently from the C#, or the C# and the Rust answer differently on purpose.

- **The watchers' requests and errors (task 11).** The expected requests and error texts in
  `aipet-core/tests/watchers.rs` were recorded once from .NET 10 (the C#'s request code against a local stub, in a
  scratch app), not by the golden generator, so CI didn't replay them against the C#: they are the record as they stand. The
  deliberate differences are listed in the module docs of `aipet-core/src/jira.rs`, `github.rs` and `http.rs`.
- **Events the C# applies but the port refuses (task 9).** A .NET 10 probe of the C#'s HookServer found events the C#
  applies and the port's `Envelope::parse` refuses, so the Rust server gives them no answer: a number past a double's
  range (`1e400`); a lone-surrogate escape (the C# applies it, or answers `error:InvalidOperationException` when Apply
  reads that string); nesting exactly 128 deep (serde_json stops at 127, System.Text.Json allows 128). A payload naming
  a member twice gets `error:ArgumentException` from the C#, while the port applies it with the last value. No hook
  sends any of these: the hooks mend lone surrogates, nest at most 65 deep, and send `{}` for a repeated member. A
  follow-up could back `Envelope` with raw JSON values (the sessions code, outside task 24's files).
- **StarvedPoolTests**: Rust has no thread pool. The ports keep every CPU busy with threads below normal priority, so
  the pet's own threads are what the hooks' 2 s holds.
- **RegistrationTests' in-process cases** called ClaudeConfig and CodexConfig in the test process; the ports run the
  built hook (`--install`, `--uninstall`, `--doctor`) in the sandbox. `PluginEvents_AreThePluginsHooks` is held by
  the doctor taking exactly codex.json's events as the plugin's, and by `--print-plugin-hooks codex` printing the file.
  `Install_UnderDotnet_Refuses`: a copy of the hook under another name stands in for `dotnet` (as the C# did with
  the test hook it named).
- **VelopackTests**: `VpkVersion_IsThePackageVersion` holds VPK_VERSION to the velopack crate the pet pins, not the
  csproj's PackageReference; `Main_RunsVelopackFirst` and `PendingUpdate_IsInstalledByTheOnlyPet` read the order in
  `main.rs` and `updates.rs`, not Program.Main's IL.
- **ResourceTests' `RustHook_CarriesTheDotnetHooksResources`** compared the Rust hook's resources with the .NET hook's,
  byte for byte. With no .NET hook, both Rust exes' resources are held to the values the .NET exes had: the names, the
  version, the icon images and the manifests.
- **SingleInstanceTests' `PetsStartedTogether_OnTheDefaultLock_OneRuns`**: the test holds the lock (the .NET pet's way)
  while a pet in the namespace is kept out, then lets go and the pet runs; two pets racing are
  `pets_started_at_the_same_moment_give_one`. Where no user and mount namespace can be made, the test says so and
  passes rather than reporting itself skipped.
- **LauncherTests** ran the launcher with `/usr/bin:/bin` on PATH; the sandbox's PATH is the allow-list.

## Not run here

The Windows-only tests are written but run only on Windows (CI's windows-2022 jobs): `resources.rs` (the five PE
resource tests, whose reader was checked on Linux against the .NET build's dlls), `registration.rs`'s
`install_claude_with_a_plugin_that_cant_run_registers`, and the Windows halves of `aipet-core`'s server, `aipet-ipc`'s
connect, `aipet-hook`'s claude.rs and `aipet`'s single.rs.

## At the cutover (task 25)

These read the C# or ran it, and were changed when the C# went (it is at commit `bf3696ea4a`):

- `crates/aipet-hook/tests/registration.rs::a_mode_without_an_agent_is_a_usage_error` read AiPet.Hook's Program.cs,
  Install.cs and PluginHooks.cs for the usage lines; it now holds them as the last C# had them.
- `crates/aipet-ipc/src/csharp.rs::every_constant_equals_ipc_cs` read AiPet.Core/Ipc.cs, and
  `crates/aipet/src/single.rs::the_mutex_is_the_csharps` (now Windows only) AiPet.UI/Program.cs: each holds the
  values as the last C# had them.
- The golden replays' live modes, which ran the generator on CI's runners: aipet-hook's registration.rs, codex.rs and
  doctor.rs replay corpora the C# wrote on Linux at the cutover (`tests/golden/*/linux/`) in their place. aipet-core's
  data.rs had the C# read what the Rust writes, and aipet-ipc's csharp.rs compared the endpoint and data paths with
  the C#'s in the same environments; those went with the C# (endpoint.rs's and paths.rs's tests hold the paths'
  rules). Neither frozen doctor corpus has the managed-settings scenarios, which the C# ran only live on CI.
- `scripts/check-parity.sh` checks the ledger against the Rust tests alone; the rows stay as the record.

## AvatarTests.cs

Custom avatar files the pet can't draw are skipped. Both lists the Rust has are held to it: `Avatar::all` and the avatars folder's pets (`load::all`, which bakes a legacy avatar file into a pack).

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `AvatarTests.AnAvatarThePetCantDraw_IsSkipped` | `[Theory]` | 10 | `crates/aipet-sprite/tests/avatars.rs::an_avatar_the_pet_cant_draw_is_skipped` (10)<br>`crates/aipet-sprite/src/avatar.rs::tests::drawable_needs_real_ellipses_and_face_rows_on_the_grid` (14) | 24 |
| `AvatarTests.AGoodAvatar_IsListed_AndEveryListedOneDraws` | `[Fact]` | 1 | `crates/aipet-sprite/tests/avatars.rs::a_good_avatar_is_listed_and_every_listed_one_draws` (1) | 1 |

## BoardTests.cs

A Codex chat as the hook and the watcher report it (`merge`), and the review bubbles (`reviews`). Beyond these, `every_board_case_matches_the_csharp` and `every_watcher_case_matches_the_csharp` replay the golden corpus.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `BoardMergeTests.LogsInterrupt_BeatsALaterStartedHookOfThatTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::merge::logs_interrupt_beats_a_later_started_hook_of_that_turn` (1) | 1 |
| `BoardMergeTests.LogsErrorEnd_BeatsTheHooksLatePrompt` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::merge::logs_error_end_beats_the_hooks_late_prompt` (1) | 1 |
| `BoardMergeTests.HookOfALaterTurn_BeatsTheLogsOlderEnd` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::merge::hook_of_a_later_turn_beats_the_logs_older_end` (1) | 1 |
| `BoardMergeTests.BothHaveTheTurnsEnd_TheNewerReportWins` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::merge::both_have_the_turns_end_the_newer_report_wins` (1) | 1 |
| `BoardMergeTests.WithoutTurnIds_TheNewerReportWins` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::merge::without_turn_ids_the_newer_report_wins` (1) | 1 |
| `BoardMergeTests.LogOfTheSameTurnStillGoing_GoesByTime` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::merge::log_of_the_same_turn_still_going_goes_by_time` (1) | 1 |
| `BoardReviewTests.WatchersError_IsShownFirst_AndNotCountedAsAReview` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::reviews::watchers_error_is_shown_first_and_not_counted_as_a_review` (1) | 1 |
| `BoardReviewTests.BothWatchersErrors_AreShown_AndOnlyReviewsOverflow` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::reviews::both_watchers_errors_are_shown_and_only_reviews_overflow` (1) | 1 |
| `BoardReviewTests.FewReviews_AndAnError_AllShow` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::reviews::few_reviews_and_an_error_all_show` (1) | 1 |
| `BoardReviewTests.JiraIssue_SaysItsStatus_NotThatAReviewWasRequested` | `[Fact]` | 1 | `crates/aipet-core/tests/board.rs::reviews::jira_issue_says_its_status_not_that_a_review_was_requested` (1) | 1 |

## HookContractTests.cs

The hook's contract. The cases that ran it against the real pet are aipet-core's server.rs; the fake-socket ones, the testkit's hook_contract.rs. aipet-hook's in-module tests hold the same rules on the event path's functions.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `HookContractTests.LoneSurrogates_StillReachThePet` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::lone_surrogates_still_reach_the_pet` (2) | 2 |
| `HookContractTests.StdinThatStaysOpen_SendsNothing_AndWritesNothing` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::stdin_that_stays_open_sends_nothing_and_writes_nothing` (1) | 1 |
| `HookContractTests.TraceLog_OnUnix_IsPerUser_AndPrivate` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/hook_contract.rs::trace_log_on_unix_is_per_user_and_private` (1) | 1 |
| `HookContractTests.TraceLog_OnUnix_LeavesAFileItDidntMakeAlone` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/hook_contract.rs::trace_log_on_unix_leaves_a_file_it_didnt_make_alone` (2) | 2 |
| `HookSocketTests.SocketThatNeverAccepts_EndsTheHookWithinItsBudget` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/hook_contract.rs::socket_that_never_accepts_ends_the_hook_within_its_budget` (1) | 1 |
| `HookSocketTests.LoneSurrogates_AreMended_AndTheRestKept` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/hook_contract.rs::lone_surrogates_are_mended_and_the_rest_kept` (1)<br>`crates/aipet-hook/src/event.rs::tests::lone_surrogates_are_mended_and_the_rest_kept` (1) | 2 |
| `HookRequestSizeTests.ALineJustUnderTheLimit_StaysWithinIt_OnceSentIsAdded` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/hook_contract.rs::a_line_just_under_the_limit_stays_within_it_once_sent_is_added` (1)<br>`crates/aipet-hook/src/event.rs::tests::tool_input_is_trimmed_at_the_limit` (1) | 2 |

## HookServerTests.cs

EndToEndTests and HookServerTests against `aipet_core::server` with the real hook; NoPetTests, the hook with no pet at all.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `EndToEndTests.Codex_ScrambledHooks_EndAtTheLatestTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::codex_scrambled_hooks_end_at_the_latest_turn` (1) | 1 |
| `EndToEndTests.Claude_LateEnvelopes_OverTheSocket_AreStale` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::claude_late_envelopes_over_the_socket_are_stale` (1) | 1 |
| `EndToEndTests.ClaudeHook_ForwardsTheEventAndItsEnvironment_Silently` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::claude_hook_forwards_the_event_and_its_environment_silently` (1) | 1 |
| `EndToEndTests.Hook_WithGarbageOnStdin_ExitsZeroSilently` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::a_hook_with_garbage_on_stdin_exits_0_silently` (2) | 2 |
| `HookServerTests.SilentClients_AreCutOff_WhileThePetKeepsServing` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::silent_clients_are_cut_off_while_the_pet_keeps_serving` (1) | 1 |
| `HookServerTests.Stop_ReturnsPromptly_WithClientsConnected` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::stop_returns_promptly_with_clients_connected` (1) | 1 |
| `HookServerTests.Garbage_IsDroppedWithoutAReply` | `[Theory]` | 5 | `crates/aipet-core/tests/server.rs::garbage_is_dropped_without_a_reply` (5) | 5 |
| `HookServerTests.TooDeep_IsDroppedWithoutAReply` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::too_deep_is_dropped_without_a_reply` (1) | 1 |
| `HookServerTests.JsonItCantServe_IsRefused` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::json_it_cant_serve_is_refused` (1) | 1 |
| `HookServerTests.OversizedRequest_IsDroppedWithoutAReply` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::an_oversized_request_is_dropped_without_a_reply` (1) | 1 |
| `NoPetTests.Hook_WithNoPet_LeavesNoTrace` | `[Theory]` | 2 | `crates/aipet-hook/tests/event.rs::with_no_pet_it_leaves_no_trace` (8) | 8 |
| `NoPetTests.Hook_WithAStaleSocketFile_LeavesNoTrace` | `[LinuxFact]` | 1 | `crates/aipet-testkit/tests/hook_contract.rs::hook_with_a_stale_socket_file_leaves_no_trace` (2)<br>`crates/aipet-ipc/src/connect.rs::tests::unix::a_missing_or_stale_socket_is_closed_at_once` (1) | 3 |

## HookServerUnixTests.cs

The socket's mode, a crashed pet's socket, a live pet, and a flood of silent connections.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `HookServerUnixTests.Socket_IsTheUsersOnly_AndGoesWithStop` | `[UnixFact]` | 1 | `crates/aipet-core/tests/server.rs::unix::the_socket_is_the_users_only_and_goes_with_stop` (1) | 1 |
| `HookServerUnixTests.WhatACrashLeft_IsReplaced` | `[LinuxTheory]` | 3 | `crates/aipet-core/tests/server.rs::unix::what_a_crash_left_is_replaced` (3) | 3 |
| `HookServerUnixTests.ALivePet_IsNotReplaced` | `[UnixFact]` | 1 | `crates/aipet-core/tests/server.rs::a_live_pet_is_not_replaced` (1) | 1 |
| `HookServerUnixTests.AFloodOfSilentConnections_IsCapped_AndThePetStillAnswers` | `[UnixFact]` | 1 | `crates/aipet-core/tests/server.rs::unix::a_flood_of_silent_connections_is_capped_and_the_pet_still_answers` (1) | 1 |

## InstallerTests.cs

Rehosted in the sandbox: packaging/linux/install.sh with the real Rust hook in the package, and the one-line install.sh against a release curl (a stand-in) serves. ScriptModeTests also holds scripts/check-parity.sh executable.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `PackageInstallerTests.Install_StartsThePet_WithoutTheToken` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/installer.rs::install_starts_the_pet_without_the_token` (1) | 1 |
| `PackageInstallerTests.Install_RemovesAnOlderSignInEntry_ButNotTheUsersOwn` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/installer.rs::install_removes_an_older_sign_in_entry_but_not_the_users_own` (1) | 1 |
| `PackageInstallerTests.Uninstall_RemovesTheHooksRegisteredAtTheAppsHook` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/installer.rs::uninstall_removes_the_hooks_registered_at_the_apps_hook` (1) | 1 |
| `PackageInstallerTests.Uninstall_RemovesTheHooksRegisteredAtTheAppsHook_BehindASymlink` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/installer.rs::uninstall_removes_the_hooks_registered_at_the_apps_hook_behind_a_symlink` (1) | 1 |
| `PackageInstallerTests.Uninstall_LeavesTheHooksOfAnotherAiPetHookAlone` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/installer.rs::uninstall_leaves_the_hooks_of_another_aipet_hook_alone` (1) | 1 |
| `OneLineInstallerTests.Install_RunsThePackagesInstaller_WithoutTheToken` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/installer.rs::install_runs_the_packages_installer_without_the_token` (1) | 1 |
| `OneLineInstallerTests.AipetVersion_SaysThePluginComesFromTheLatestRelease` | `[UnixTheory]` | 2 | `crates/aipet-testkit/tests/installer.rs::aipet_version_says_the_plugin_comes_from_the_latest_release` (2) | 2 |
| `ScriptModeTests.ShellScripts_AreExecutable` | `[UnixTheory]` | 5 | `crates/aipet-testkit/tests/installer.rs::shell_scripts_are_executable` (6) | 6 |

## LauncherTests.cs

plugins/aipet/native/aipet-hook.sh, rehosted in the sandbox with a fake uname and fake hooks.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `LauncherTests.RunsOnlyTheHookThisPlatformCanLoad` | `[UnixTheory]` | 13 | `crates/aipet-testkit/tests/launcher.rs::runs_only_the_hook_this_platform_can_load` (13) | 13 |
| `LauncherTests.OnWindows_RunsTheWindowsHook_WithoutUname` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/launcher.rs::on_windows_runs_the_windows_hook_without_uname` (1) | 1 |
| `LauncherTests.FindsTheHook_FromARelativePath` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/launcher.rs::finds_the_hook_from_a_relative_path` (1) | 1 |
| `LauncherTests.PassesTheHooksStdoutThrough` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/launcher.rs::passes_the_hooks_stdout_through` (1) | 1 |
| `LauncherTests.ABrokenHook_EndsInAQuietZero` | `[UnixTheory]` | 6 | `crates/aipet-testkit/tests/launcher.rs::a_broken_hook_ends_in_a_quiet_zero` (6) | 6 |

## OrderingTests.cs

ClaudeOrderingTests (`ordering`) and CodexOrderingTests (`codex_ordering`), case for case. Beyond these, `every_claude_case_matches_the_csharp` and `every_codex_case_matches_the_csharp` replay the golden corpus of the C#'s AgentSessions.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `ClaudeOrderingTests.PreToolUse_AfterTheStopItPreceded_IsStale` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::pre_tool_use_after_the_stop_it_preceded_is_stale` (1) | 1 |
| `ClaudeOrderingTests.SameTick_GoesByRank` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::same_tick_goes_by_rank` (1) | 1 |
| `ClaudeOrderingTests.SameTick_InTurnOrder_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::same_tick_in_turn_order_is_taken` (1) | 1 |
| `ClaudeOrderingTests.SessionEnd_ThenALateEvent_DoesNotBringTheChatBack` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::session_end_then_a_late_event_does_not_bring_the_chat_back` (1) | 1 |
| `ClaudeOrderingTests.ClockSetBack_TakesTheNextEvent` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::clock_set_back_takes_the_next_event` (1) | 1 |
| `ClaudeOrderingTests.ClockSetBack_AfterSessionEnd_TakesTheChatUpAgain` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::clock_set_back_after_session_end_takes_the_chat_up_again` (1) | 1 |
| `ClaudeOrderingTests.PermissionRequest_StartedJustBeforeItsPreToolUse_ArrivingAfterIt_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::permission_request_started_just_before_its_pre_tool_use_arriving_after_it_is_taken` (1) | 1 |
| `ClaudeOrderingTests.PreToolUse_StartedJustAfterItsPermissionRequest_ArrivingAfterIt_IsStale` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::pre_tool_use_started_just_after_its_permission_request_arriving_after_it_is_stale` (1) | 1 |
| `ClaudeOrderingTests.SameCommandAgain_AfterAPairedRequest_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::same_command_again_after_a_paired_request_is_taken` (1) | 1 |
| `ClaudeOrderingTests.PermissionRequest_BehindAnotherCallsPreToolUse_GoesByAt` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::permission_request_behind_another_calls_pre_tool_use_goes_by_at` (1) | 1 |
| `ClaudeOrderingTests.PreToolUse_OfAnotherCommand_OrLongAfter_ARequest_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::pre_tool_use_of_another_command_or_long_after_a_request_is_taken` (1) | 1 |
| `ClaudeOrderingTests.PairedRequest_DoesNotMoveTheLatestBack` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::paired_request_does_not_move_the_latest_back` (1) | 1 |
| `ClaudeOrderingTests.UnknownEvent_IsIgnored_AndMakesNoChat` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::ordering::unknown_event_is_ignored_and_makes_no_chat` (1) | 1 |
| `CodexOrderingTests.EarlierTurn_AfterTheNextTurnStarted_IsStale_EvenWithALaterAt` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::earlier_turn_after_the_next_turn_started_is_stale_even_with_a_later_at` (1) | 1 |
| `CodexOrderingTests.NothingOfATurn_IsTakenAfterItsStop` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::nothing_of_a_turn_is_taken_after_its_stop` (1) | 1 |
| `CodexOrderingTests.NothingOfATurn_IsTakenAfterItsInterrupt` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::nothing_of_a_turn_is_taken_after_its_interrupt` (1) | 1 |
| `CodexOrderingTests.PreToolUse_AfterItsOwnPostToolUse_IsStale` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::pre_tool_use_after_its_own_post_tool_use_is_stale` (1) | 1 |
| `CodexOrderingTests.PreToolUse_AfterItsPermissionRequest_IsStale` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::pre_tool_use_after_its_permission_request_is_stale` (1) | 1 |
| `CodexOrderingTests.PreToolUse_OfAnotherCommand_AfterAPermissionRequest_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::pre_tool_use_of_another_command_after_a_permission_request_is_taken` (1) | 1 |
| `CodexOrderingTests.OwnNewTurn_IsTaken_EvenWhenItsAtIsBeforeThePreviousStop` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::own_new_turn_is_taken_even_when_its_at_is_before_the_previous_stop` (1) | 1 |
| `CodexOrderingTests.SubAgent_AfterTheChatsStop_IsStale_UntilTheNextTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::sub_agent_after_the_chats_stop_is_stale_until_the_next_turn` (1) | 1 |
| `CodexOrderingTests.NonV7TurnIds_OnlyTellASeenTurnFromANewOne` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::non_v7_turn_ids_only_tell_a_seen_turn_from_a_new_one` (1) | 1 |
| `CodexOrderingTests.EventsWithoutATurn_GoByAt` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::events_without_a_turn_go_by_at` (1) | 1 |
| `CodexOrderingTests.LateUserPromptSubmit_OfASeenTurn_KeepsTheState_ButNamesTheChat` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::late_user_prompt_submit_of_a_seen_turn_keeps_the_state_but_names_the_chat` (1) | 1 |
| `CodexOrderingTests.UserPromptSubmit_ThenItsTurnsEvents_AreTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::user_prompt_submit_then_its_turns_events_are_taken` (1) | 1 |
| `CodexOrderingTests.NewChat_FirstEventIsItsPrompt_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::new_chat_first_event_is_its_prompt_is_taken` (1) | 1 |
| `CodexOrderingTests.PermissionRequest_OfARerun_GoesToTheRerun_NotTheEarlierCallOfTheSameCommand` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::permission_request_of_a_rerun_goes_to_the_rerun_not_the_earlier_call_of_the_same_command` (1) | 1 |
| `CodexOrderingTests.PermissionRequest_OfARerun_ArrivingAfterItsPreToolUse_StartedBeforeIt_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::permission_request_of_a_rerun_arriving_after_its_pre_tool_use_started_before_it_is_taken` (1) | 1 |
| `CodexOrderingTests.PermissionRequest_OfAFastRerun_ArrivingBeforeItsPreToolUse_KeepsTheChatAsking` | `[Theory]` | 3 | `crates/aipet-core/tests/sessions.rs::codex_ordering::permission_request_of_a_fast_rerun_arriving_before_its_pre_tool_use_keeps_the_chat_asking` (3) | 3 |
| `CodexOrderingTests.PermissionRequest_OfAFastRerun_ArrivingAfterItsPreToolUse_IsTaken` | `[Theory]` | 3 | `crates/aipet-core/tests/sessions.rs::codex_ordering::permission_request_of_a_fast_rerun_arriving_after_its_pre_tool_use_is_taken` (3) | 3 |
| `CodexOrderingTests.SameCommandAgain_AfterTheAskingCallsPostToolUse_IsTaken` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::same_command_again_after_the_asking_calls_post_tool_use_is_taken` (1) | 1 |
| `CodexOrderingTests.PairedRequest_DoesNotMoveTheLatestBack` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::paired_request_does_not_move_the_latest_back` (1) | 1 |
| `CodexOrderingTests.LateUserPromptSubmit_AfterItsTurnsStop_StillNamesTheChat` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::late_user_prompt_submit_after_its_turns_stop_still_names_the_chat` (1) | 1 |
| `CodexOrderingTests.PermissionRequest_BehindAnotherCall_GoesByAt` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::permission_request_behind_another_call_goes_by_at` (1) | 1 |
| `CodexOrderingTests.TwoIdenticalCalls_FarApart_TheSecondsRequestGoesWithTheSecond` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::two_identical_calls_far_apart_the_seconds_request_goes_with_the_second` (1) | 1 |
| `CodexOrderingTests.PendingRequest_IsNotTakenByTheSameCommandLongAfter` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::pending_request_is_not_taken_by_the_same_command_long_after` (1) | 1 |
| `CodexOrderingTests.ManualCompact_EndsItsTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::manual_compact_ends_its_turn` (1) | 1 |
| `CodexOrderingTests.ManualCompact_PreCompactArrivingLast_IsStale` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::manual_compact_pre_compact_arriving_last_is_stale` (1) | 1 |
| `CodexOrderingTests.AutoCompact_StaysInItsTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::auto_compact_stays_in_its_turn` (1) | 1 |
| `CodexOrderingTests.SubAgentsCompact_DoesNotEndTheChatsTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::sub_agents_compact_does_not_end_the_chats_turn` (1) | 1 |
| `CodexOrderingTests.ClockSetBack_NewTurnsAreTaken_AndTheOldTurnStaysStale` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::clock_set_back_new_turns_are_taken_and_the_old_turn_stays_stale` (1) | 1 |
| `CodexOrderingTests.NewChat_StartsFromItsFirstEventsTurn` | `[Fact]` | 1 | `crates/aipet-core/tests/sessions.rs::codex_ordering::new_chat_starts_from_its_first_events_turn` (1) | 1 |

## PluginHooksTests.cs

`--print-plugin-hooks` is aipet-hook's own test; check-plugin.sh's comparison is rehosted in the sandbox.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `PluginHooksTests.PrintPluginHooks_IsThePluginsHookFile` | `[Theory]` | 3 | `crates/aipet-hook/tests/registration.rs::print_plugin_hooks_is_the_plugin_files` (5) | 5 |
| `PluginHooksTests.PrintPluginHooks_WithoutAnAgent_PrintsUsage` | `[Theory]` | 2 | `crates/aipet-hook/tests/registration.rs::print_plugin_hooks_is_the_plugin_files` (5)<br>`crates/aipet-hook/tests/registration.rs::a_mode_without_an_agent_is_a_usage_error` (4) | 9 |
| `PluginHooksTests.CheckPlugin_ComparesTheHookFilesWithTheHook` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/plugin_hooks.rs::check_plugin_compares_the_hook_files_with_the_hook` (1) | 1 |

## PresetTests.cs

Settings' preset files (`aipet_core::presets`).

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `PresetTests.AFullPreset_SetsEveryField_AndKeepsTheUsersOwn` | `[Fact]` | 1 | `crates/aipet-core/tests/presets.rs::a_full_preset_sets_every_field_and_keeps_the_users_own` (1) | 1 |
| `PresetTests.APartialPreset_ChangesOnlyWhatItHas` | `[Fact]` | 1 | `crates/aipet-core/tests/presets.rs::a_partial_preset_changes_only_what_it_has` (1) | 1 |
| `PresetTests.BlankFields_MeanTheDefaults_AsSettingsSaveDoes` | `[Fact]` | 1 | `crates/aipet-core/tests/presets.rs::blank_fields_mean_the_defaults_as_settings_save_does` (1) | 1 |
| `PresetTests.NoSections_IsAValidPresetWithNothingInIt` | `[Fact]` | 1 | `crates/aipet-core/tests/presets.rs::no_sections_is_a_valid_preset_with_nothing_in_it` (1) | 1 |
| `PresetTests.TokensAndEmail_AreNeverRead` | `[Fact]` | 1 | `crates/aipet-core/tests/presets.rs::tokens_and_email_are_never_read` (1) | 1 |
| `PresetTests.AFileThatIsntAPreset_IsRefusedWithAReason` | `[Theory]` | 12 | `crates/aipet-core/tests/presets.rs::a_file_that_isnt_a_preset_is_refused_with_a_reason` (15) | 15 |
| `PresetTests.APresetThatPointsJiraElsewhere_IsAMove` | `[Theory]` | 5 | `crates/aipet-core/tests/presets.rs::a_preset_that_points_jira_elsewhere_is_a_move` (5) | 5 |
| `PresetTests.APresetThatPointsGitHubElsewhere_IsAMove` | `[Theory]` | 4 | `crates/aipet-core/tests/presets.rs::a_preset_that_points_github_elsewhere_is_a_move` (4) | 4 |
| `PresetTests.AFirstSite_IsAMoveToo` | `[Fact]` | 1 | `crates/aipet-core/tests/presets.rs::a_first_site_is_a_move_too` (1) | 1 |

## RegistrationTests.cs

The cases the C# ran in-process run the built hook in the sandbox (`--install`, `--uninstall`, `--doctor`), as the installers run it. The golden generator's recorded cases are aipet-hook's tests/registration.rs, codex.rs and doctor.rs.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `RegistrationTests.Rewrites_KeepTheFilesMode` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::rewrites_keep_the_files_mode` (2) | 2 |
| `RegistrationTests.NewFiles_AreTheUsersOnly` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::new_files_are_the_users_only` (1) | 1 |
| `RegistrationTests.ALeftoverTempFile_DoesntPassItsModeOn` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::a_leftover_temp_file_doesnt_pass_its_mode_on` (1) | 1 |
| `RegistrationTests.HooksJson_IsReplacedAtOnce` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::hooks_json_is_replaced_at_once` (1) | 1 |
| `RegistrationTests.InstallClaude_WithThePluginEnabled_RegistersNone` | `[Fact]` | 1 | `crates/aipet-testkit/tests/registration.rs::install_claude_with_the_plugin_enabled_registers_none` (1) | 1 |
| `RegistrationTests.InstallClaude_WithThePluginEnabledButNotInstalled_Registers` | `[Fact]` | 1 | `crates/aipet-testkit/tests/registration.rs::install_claude_with_the_plugin_enabled_but_not_installed_registers` (1) | 1 |
| `RegistrationTests.InstallCodex_WithThePluginEnabled_RegistersNone` | `[Fact]` | 1 | `crates/aipet-testkit/tests/registration.rs::install_codex_with_the_plugin_enabled_registers_none` (1) | 1 |
| `RegistrationTests.InstallClaude_WithAPluginThatCantRun_Registers` | `[WindowsFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::install_claude_with_a_plugin_that_cant_run_registers` (1) | 1 |
| `RegistrationTests.GitBash_IsFoundWhereClaudeLooks` | `[Fact]` | 1 | `crates/aipet-hook/src/claude.rs::tests::git_bash_is_found_where_claude_looks` (1) | 1 |
| `RegistrationTests.CodexPlugin_IsWhatCodexRuns` | `[Fact]` | 1 | `crates/aipet-testkit/tests/registration.rs::codex_plugin_is_what_codex_runs` (10) | 10 |
| `RegistrationTests.PluginEvents_AreThePluginsHooks` | `[Fact]` | 1 | `crates/aipet-testkit/tests/registration.rs::plugin_events_are_the_plugins_hooks` (1)<br>`crates/aipet-hook/tests/registration.rs::print_plugin_hooks_is_the_plugin_files` (5) | 6 |
| `RegistrationTests.IsOurs_KnowsEveryFormOfAiPetsCommand` | `[Fact]` | 1 | `crates/aipet-hook/src/codex.rs::tests::helpers_answer_as_the_csharps` (43) | 43 |
| `RegistrationTests.Install_UnderDotnet_Refuses` | `[Fact]` | 1 | `crates/aipet-testkit/tests/registration.rs::install_under_another_name_refuses` (1) | 1 |
| `RegistrationTests.Doctor_KnowsLegacyCodexHooks` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::doctor_knows_legacy_codex_hooks` (1) | 1 |
| `RegistrationTests.Doctor_WithoutHooksList_KnowsThePlugin` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::doctor_without_hooks_list_knows_the_plugin` (1) | 1 |
| `RegistrationTests.Doctor_ChecksThePluginsHooksAsThePlugins` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::doctor_checks_the_plugins_hooks_as_the_plugins` (1) | 1 |
| `RegistrationTests.Doctor_PluginAndDirectHooks_AreReportedTwice` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::doctor_plugin_and_direct_hooks_are_reported_twice` (1) | 1 |
| `RegistrationTests.Doctor_PluginWithoutAHookForThisSystem_Fails` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/registration.rs::doctor_plugin_without_a_hook_for_this_system_fails` (1) | 1 |

## ReleaseWorkflowTests.cs

release.yml's steps, rehosted in the sandbox and run as the runner runs them; gh, ssh and readelf are stand-ins.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `ReleaseWorkflowTests.LatestCheck_ByVersionAndPin` | `[UnixTheory]` | 7 | `crates/aipet-testkit/tests/release_workflow.rs::latest_check_by_version_and_pin` (7) | 7 |
| `ReleaseWorkflowTests.Pin_RunsOnlyForTheLatestRelease` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::pin_runs_only_for_the_latest_release` (1) | 1 |
| `ReleaseWorkflowTests.Check_ReleasesFromMain_DryRunsFromAnyBranch` | `[UnixTheory]` | 3 | `crates/aipet-testkit/tests/release_workflow.rs::check_releases_from_main_dry_runs_from_any_branch` (3) | 3 |
| `ReleaseWorkflowTests.DryRun_PushesAndPublishesNothing` | `[UnixTheory]` | 2 | `crates/aipet-testkit/tests/release_workflow.rs::dry_run_pushes_and_publishes_nothing` (2) | 2 |
| `ReleaseWorkflowTests.GlibcGate_RefusesABinaryThatNeedsANewerGlibc` | `[UnixTheory]` | 4 | `crates/aipet-testkit/tests/release_workflow.rs::glibc_gate_refuses_a_binary_that_needs_a_newer_glibc` (4) | 4 |
| `ReleaseWorkflowTests.Tarball_KeepsTheLayout` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::tarball_keeps_the_layout` (1) | 1 |
| `ReleaseWorkflowTests.Release_BuildsTheRustPet` | `[Fact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::release_builds_the_rust_pet` (1) | 1 |
| `ReleaseWorkflowTests.Notes_StableAfterAPrerelease_ComparesWithTheLatestStableRelease` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::notes_stable_after_a_prerelease_compares_with_the_latest_stable_release` (1) | 1 |
| `ReleaseWorkflowTests.Notes_Prerelease_ComparesWithTheLatestStableRelease` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::notes_prerelease_compares_with_the_latest_stable_release` (1) | 1 |
| `ReleaseWorkflowTests.Notes_UnchangedHooks_NoWarning` | `[UnixFact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::notes_unchanged_hooks_no_warning` (1) | 1 |
| `ReleaseWorkflowTests.Check_PluginRepositoryWithoutABranch_FailsEarly` | `[UnixTheory]` | 4 | `crates/aipet-testkit/tests/release_workflow.rs::check_plugin_repository_without_a_branch_fails_early` (4) | 4 |
| `ReleaseWorkflowTests.Plugin_SumsTheReleaseBeforeItPushes` | `[Fact]` | 1 | `crates/aipet-testkit/tests/release_workflow.rs::plugin_sums_the_release_before_it_pushes` (1) | 1 |
| `ReleaseWorkflowTests.Publish_ReleasesOnlyTheSummedFiles` | `[UnixTheory]` | 4 | `crates/aipet-testkit/tests/release_workflow.rs::publish_releases_only_the_summed_files` (4) | 4 |

## ResourceTests.cs

The Windows exes' icon, manifest and version resource, read from the PE file and by Windows. Only a Windows build has them: skipped elsewhere.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `ResourceTests.Version_NamesTheExe` | `[Theory]` | 2 | `crates/aipet-testkit/tests/resources.rs::version_names_the_exe` (2) | 2 |
| `ResourceTests.Icon_IsTheAppIcon` | `[Theory]` | 2 | `crates/aipet-testkit/tests/resources.rs::icon_is_the_app_icon` (2) | 2 |
| `ResourceTests.Manifest_IsEmbedded` | `[Theory]` | 2 | `crates/aipet-testkit/tests/resources.rs::manifest_is_embedded` (2) | 2 |
| `ResourceTests.Windows_ReadsTheVersion` | `[Theory]` | 2 | `crates/aipet-testkit/tests/resources.rs::windows_reads_the_version` (2) | 2 |
| `ResourceTests.RustHook_CarriesTheDotnetHooksResources` | `[RustHookFact]` | 1 | `crates/aipet-testkit/tests/resources.rs::the_hook_has_the_dotnet_hooks_resources_and_no_others` (1)<br>`crates/aipet-testkit/tests/resources.rs::version_names_the_exe` (2)<br>`crates/aipet-testkit/tests/resources.rs::icon_is_the_app_icon` (2)<br>`crates/aipet-testkit/tests/resources.rs::manifest_is_embedded` (2) | 7 |

## SingleInstanceTests.cs

The Linux lock (`single::take`), in src/single.rs's tests, whose pets are the test binary run again (`contender`).

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `SingleInstanceTests.PetsStartedTogether_OneRuns` | `[LinuxTheory]` | 3 | `crates/aipet/src/single.rs::tests::unix::pets_in_other_processes_with_other_runtime_folders_are_kept_out` (3)<br>`crates/aipet/src/single.rs::tests::unix::pets_started_at_the_same_moment_give_one` (1) | 4 |
| `SingleInstanceTests.ALockTakenTheContractsWay_AndThePetsLock_KeepEachOtherOut` | `[LinuxFact]` | 1 | `crates/aipet/src/single.rs::tests::unix::a_lock_the_csharp_took_keeps_this_pet_out_and_the_other_way_round` (1) | 1 |
| `SingleInstanceTests.ACrashedPetsLock_DoesntKeepTheNextOneOut` | `[LinuxFact]` | 1 | `crates/aipet/src/single.rs::tests::unix::a_crashed_pets_lock_doesnt_keep_the_next_one_out` (1) | 1 |
| `SingleInstanceTests.ALockFileThatCantBeOpened_KeepsEveryPetOut` | `[LinuxTheory]` | 2 | `crates/aipet/src/single.rs::tests::unix::a_link_where_the_lock_goes_is_an_error` (1)<br>`crates/aipet/src/single.rs::tests::unix::a_folder_where_the_lock_goes_is_an_error` (1) | 2 |
| `SingleInstanceTests.PetsStartedTogether_OnTheDefaultLock_OneRuns` | `[UserNamespaceTheory]` | 3 | `crates/aipet/src/single.rs::tests::unix::pets_on_the_default_lock_in_a_namespace_of_their_own_give_one` (3) | 3 |
| `SingleInstanceTests.TheLock_IsNextToTheSocket` | `[LinuxFact]` | 1 | `crates/aipet/src/single.rs::tests::unix::the_lock_is_the_sockets_with_lock_for_sock` (1)<br>`crates/aipet/src/single.rs::tests::unix::the_lock_follows_the_socket_whatever_xdg_runtime_dir_says` (1) | 2 |

## StarvedPoolTests.cs

Rust has no thread pool to starve: the ports keep every CPU busy instead (see the deviations).

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `StarvedPoolTests.Listeners_AnswerHooks_WhileThePoolIsStarved` | `[Fact]` | 1 | `crates/aipet-core/tests/server.rs::listeners_answer_hooks_while_every_cpu_is_busy` (1) | 1 |
| `StarvedPoolTests.SilentClients_AreCutOff_AndStopReturns_WhileThePoolIsStarved` | `[UnixFact]` | 1 | `crates/aipet-core/tests/server.rs::silent_clients_are_cut_off_and_stop_returns_while_every_cpu_is_busy` (1) | 1 |

## VelopackTests.cs

VelopackTests and HookCleanupTests: which configs name the installed hook, the update texts, and release.yml, the installers and `main` in step.

| C# test | Attribute | C# cases | Rust | Rust cases |
|---|---|---|---|---|
| `VelopackTests.Names_ThisInstallsHook` | `[Theory]` | 6 | `crates/aipet-core/tests/data.rs::hook_cleanup_names_the_hook_as_the_app_does` (40) | 40 |
| `VelopackTests.DoesntName_AnotherHook` | `[Theory]` | 7 | `crates/aipet-core/tests/data.rs::hook_cleanup_names_the_hook_as_the_app_does` (40) | 40 |
| `VelopackTests.Names_CaseMattersWhereTheFileSystemSaysSo` | `[Fact]` | 1 | `crates/aipet-core/tests/data.rs::hook_cleanup_names_the_hook_as_the_app_does` (40) | 40 |
| `VelopackTests.Names_TheShortAndTheQuotedForms` | `[Fact]` | 1 | `crates/aipet-core/tests/data.rs::hook_cleanup_names_the_hook_as_the_app_does` (40) | 40 |
| `VelopackTests.Status_Texts` | `[Fact]` | 1 | `crates/aipet-core/tests/data.rs::update_status_texts_are_the_apps` (28) | 28 |
| `VelopackTests.Channel_IsTheOneReleaseYmlPacks` | `[Fact]` | 1 | `crates/aipet-testkit/tests/velopack.rs::channel_is_the_one_release_yml_packs` (1) | 1 |
| `VelopackTests.VpkVersion_IsThePackageVersion` | `[Fact]` | 1 | `crates/aipet-testkit/tests/velopack.rs::vpk_version_is_the_package_version` (1) | 1 |
| `VelopackTests.Repo_IsTheOneTheInstallersUse` | `[Fact]` | 1 | `crates/aipet-testkit/tests/velopack.rs::repo_is_the_one_the_installers_use` (1) | 1 |
| `VelopackTests.Installed_IsTheCopyWithUpdateExeAbove` | `[Fact]` | 1 | `crates/aipet-core/src/update_status.rs::tests::installed_is_the_copy_with_update_exe_above` (1) | 1 |
| `VelopackTests.Main_RunsVelopackFirst` | `[Fact]` | 1 | `crates/aipet/src/updates.rs::tests::velopack_runs_before_anything_else_in_main` (1) | 1 |
| `VelopackTests.PendingUpdate_IsInstalledByTheOnlyPet` | `[Fact]` | 1 | `crates/aipet-testkit/tests/velopack.rs::pending_update_is_installed_by_the_only_pet` (1)<br>`crates/aipet/src/updates.rs::tests::an_update_downloaded_earlier_installs_first_and_the_pet_exits` (1) | 2 |
| `HookCleanupTests.Files_AreTheOnesTheHookRegistersIn` | `[Fact]` | 1 | `crates/aipet-core/src/cleanup.rs::tests::the_configs_are_where_the_hook_registers` (1) | 1 |
| `HookCleanupTests.Agents_WhoseConfigNamesTheHook` | `[Fact]` | 1 | `crates/aipet-core/tests/data.rs::hook_cleanup_finds_the_agents_as_the_app_does` (5) | 5 |
| `HookCleanupTests.Agents_FindsCodexHooksJson` | `[Fact]` | 1 | `crates/aipet-core/tests/data.rs::hook_cleanup_finds_the_agents_as_the_app_does` (5) | 5 |
| `HookCleanupTests.Run_UninstallsFromTheAgentsThatNameTheHook` | `[UnixFact]` | 1 | `crates/aipet-core/src/cleanup.rs::tests::run_uninstalls_from_the_agents_that_name_the_hook` (1) | 1 |
| `HookCleanupTests.Run_StopsAHookThatHangs` | `[UnixFact]` | 1 | `crates/aipet-core/src/cleanup.rs::tests::run_stops_a_hook_that_hangs` (1) | 1 |

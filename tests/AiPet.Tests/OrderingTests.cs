using System.Text.Json.Nodes;
using Xunit;

namespace AiPet.Tests;

/// Claude's events, placed by `at` (when the hook started) and Rank for the same tick (AgentSessions.Stale), and a
/// call's PreToolUse and PermissionRequest by each other (ClaudePair).
public class ClaudeOrderingTests
{
    readonly AgentSessions sessions = new();
    readonly string sid = Guid.NewGuid().ToString();
    readonly double t0 = Board.Unix;

    string Apply(string ev, double at, JsonObject extra = null) =>
        sessions.Apply(Events.Envelope("claude", at, Events.Payload(sid, ev, extra: extra))).Outcome;

    AgentSessions.Entry Chat => sessions.Snapshot().Single(e => e.Id == "claude:" + sid);

    [Fact]
    public void PreToolUse_AfterTheStopItPreceded_IsStale()
    {
        Assert.Equal("thinking", Apply("UserPromptSubmit", t0, new JsonObject { ["prompt"] = "fix it" }));
        Assert.Equal("done", Apply("Stop", t0 + 2));
        Assert.Equal("stale", Apply("PreToolUse", t0 + 1, Events.Bash("ls")));
        Assert.Equal("stale", Apply("PostToolUse", t0 + 1.5, Events.Bash("ls")));
        Assert.Equal(("done", "Done"), (Chat.State, Chat.Detail));
        Assert.Equal(t0 + 2, Chat.Ts);
    }

    [Fact]
    public void SameTick_GoesByRank()
    {
        Apply("UserPromptSubmit", t0);
        // a PreToolUse and its PermissionRequest can start in the same millisecond; the request is later in a turn
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1));
        Assert.Equal("stale", Apply("PreToolUse", t0 + 1, Events.Bash("ls")));
        // within half a millisecond is the same tick
        Assert.Equal("stale", Apply("PreToolUse", t0 + 1.0004, Events.Bash("ls")));
        Assert.Equal("attention", Chat.State);
        // the same rank isn't older: a Stop in that tick is taken
        Assert.Equal("done", Apply("Stop", t0 + 1));
    }

    [Fact]
    public void SameTick_InTurnOrder_IsTaken()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("working", Apply("PreToolUse", t0 + 1, Events.Bash("ls")));
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1));
        Assert.Equal("attention", Chat.State);
    }

    [Fact]
    public void SessionEnd_ThenALateEvent_DoesNotBringTheChatBack()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("removed", Apply("SessionEnd", t0 + 2));
        Assert.Equal("stale", Apply("PreToolUse", t0 + 1, Events.Bash("ls")));
        Assert.Equal("stale", Apply("Stop", t0 + 2));
        Assert.Equal("stale", Apply("SessionEnd", t0 + 1.5));
        Assert.Equal(t0 + 2, Chat.Ended);
        Assert.Null(Chat.State);
        // a chat taken up again after it ended starts over
        Assert.Equal("idle", Apply("SessionStart", t0 + 3));
        Assert.Null(Chat.Ended);
        Assert.Equal("idle", Chat.State);
    }

    [Fact]
    public void ClockSetBack_TakesTheNextEvent()
    {
        // recorded while the clock was an hour ahead: it can't order what comes after the clock is set right
        Assert.Equal("done", Apply("Stop", t0 + 3600));
        Assert.Equal("thinking", Apply("UserPromptSubmit", t0));
        Assert.Equal("thinking", Chat.State);
        Assert.Equal(t0, Chat.Ts);
        Assert.Equal("stale", Apply("PreToolUse", t0 - 1, Events.Bash("ls")));
    }

    [Fact]
    public void ClockSetBack_AfterSessionEnd_TakesTheChatUpAgain()
    {
        Assert.Equal("removed", Apply("SessionEnd", t0 + 3600));
        Assert.Equal("idle", Apply("SessionStart", t0));
        Assert.Null(Chat.Ended);
    }

    [Fact]
    public void PermissionRequest_StartedJustBeforeItsPreToolUse_ArrivingAfterIt_IsTaken()
    {
        Apply("UserPromptSubmit", t0);
        // a plugin install's shell and wrapper started the PreToolUse's hook a few ms later than the request's
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.003, Events.Bash("rm -rf build", "toolu_1")));
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1.001, Events.Bash("rm -rf build", description: "clean")));
        Assert.Equal(("attention", "Needs your permission"), (Chat.State, Chat.Detail));
    }

    [Fact]
    public void PreToolUse_StartedJustAfterItsPermissionRequest_ArrivingAfterIt_IsStale()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1.001, Events.Bash("rm -rf build", description: "clean")));
        // later by at (the request's hook started first), but the request for its call is in
        Assert.Equal("stale", Apply("PreToolUse", t0 + 1.003, Events.Bash("rm -rf build", "toolu_1")));
        Assert.Equal("attention", Chat.State);
        // its PostToolUse moves the chat on, and the same command run again right after is a call of its own
        Assert.Equal("thinking", Apply("PostToolUse", t0 + 1.4, Events.Bash("rm -rf build", "toolu_1")));
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.5, Events.Bash("rm -rf build", "toolu_2")));
    }

    [Fact]
    public void SameCommandAgain_AfterAPairedRequest_IsTaken()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("working", Apply("PreToolUse", t0 + 1, Events.Bash("npm test", "toolu_1")));
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1.001, Events.Bash("npm test")));
        Assert.Equal("thinking", Apply("PostToolUse", t0 + 1.4, Events.Bash("npm test", "toolu_1")));
        // within a second of the request, but that request has its PreToolUse already
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.5, Events.Bash("npm test", "toolu_2")));
    }

    [Fact]
    public void PermissionRequest_BehindAnotherCallsPreToolUse_GoesByAt()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.003, Events.Bash("rm -rf build", "toolu_1")));
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.010, Events.Bash("ls", "toolu_2")));
        // its own PreToolUse isn't the latest: something else started after it
        Assert.Equal("stale", Apply("PermissionRequest", t0 + 1.001, Events.Bash("rm -rf build")));
        Assert.Equal("working", Chat.State);
    }

    [Fact]
    public void PreToolUse_OfAnotherCommand_OrLongAfter_ARequest_IsTaken()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1, Events.Bash("rm -rf build")));
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.002, Events.Bash("ls", "toolu_1")));
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 3, Events.Bash("make")));
        // more than a second apart isn't the same call: at decides
        Assert.Equal("working", Apply("PreToolUse", t0 + 4.5, Events.Bash("make", "toolu_2")));
    }

    [Fact]
    public void PairedRequest_DoesNotMoveTheLatestBack()
    {
        Apply("UserPromptSubmit", t0);
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.000, Events.Bash("ls", "toolu_a")));
        Assert.Equal("working", Apply("PreToolUse", t0 + 1.004, Events.Bash("rm -rf build", "toolu_b")));
        Assert.Equal("attention", Apply("PermissionRequest", t0 + 1.001, Events.Bash("rm -rf build")));
        // started between the request and its PreToolUse: older than the latest, which is still that PreToolUse
        Assert.Equal("stale", Apply("PostToolUse", t0 + 1.0025, Events.Bash("ls", "toolu_a")));
        Assert.Equal(("attention", "Needs your permission"), (Chat.State, Chat.Detail));
        Assert.Equal("thinking", Apply("PostToolUse", t0 + 1.5, Events.Bash("rm -rf build", "toolu_b")));
    }

    [Fact]
    public void UnknownEvent_IsIgnored_AndMakesNoChat()
    {
        Assert.Equal("ignored", Apply("SomethingNew", t0));
        Assert.DoesNotContain(sessions.Snapshot(), e => e.Id == "claude:" + sid);
    }
}

/// Codex's events, placed by its own ids first (AgentSessions.CodexStale): turn_id (UUIDv7), tool_use_id.
public class CodexOrderingTests
{
    readonly AgentSessions sessions = new();
    readonly string sid = Guid.NewGuid().ToString();
    readonly double t0 = Board.Unix;
    // real v7 ids, one second apart, so their text sorts by when the turn started
    readonly string turn1, turn2, turn3;

    public CodexOrderingTests()
    {
        var start = DateTimeOffset.UtcNow.AddMinutes(-1);
        turn1 = Guid.CreateVersion7(start).ToString();
        turn2 = Guid.CreateVersion7(start.AddSeconds(1)).ToString();
        turn3 = Guid.CreateVersion7(start.AddSeconds(2)).ToString();
        Assert.Equal('7', turn1[14]);
        Assert.True(string.CompareOrdinal(turn1, turn2) < 0 && string.CompareOrdinal(turn2, turn3) < 0);
    }

    string Apply(string ev, string turn, double at, JsonObject extra = null) =>
        sessions.Apply(Events.Envelope("codex", at, Events.Payload(sid, ev, turn, extra))).Outcome;

    AgentSessions.Entry Chat => sessions.Snapshot().Single(e => e.Id == "codex:" + sid);

    static JsonObject Helper(string agentId, JsonObject extra = null)
    {
        var o = extra ?? new JsonObject();
        o["agent_id"] = agentId;
        return o;
    }

    [Fact]
    public void EarlierTurn_AfterTheNextTurnStarted_IsStale_EvenWithALaterAt()
    {
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn1, t0));
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn2, t0 + 1, new JsonObject { ["prompt"] = "second" }));
        // PowerShell started these hooks late: their at is after turn 2's, their ids say turn 1
        Assert.Equal("stale", Apply("PreToolUse", turn1, t0 + 5, Events.Bash("ls", "call_1")));
        Assert.Equal("stale", Apply("Stop", turn1, t0 + 6));
        Assert.Equal(("thinking", "Thinking", "Second"), (Chat.State, Chat.Detail, Chat.Title));
    }

    [Fact]
    public void NothingOfATurn_IsTakenAfterItsStop()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("done", Apply("Stop", turn1, t0 + 1));
        Assert.Equal("stale", Apply("PreToolUse", turn1, t0 + 2, Events.Bash("ls", "call_1")));
        Assert.Equal("stale", Apply("PermissionRequest", turn1, t0 + 3, Events.Bash("ls")));
        Assert.Equal("stale", Apply("PostToolUse", turn1, t0 + 4, Events.Bash("ls", "call_1")));
        Assert.Equal("done", Chat.State);
    }

    [Fact]
    public void NothingOfATurn_IsTakenAfterItsInterrupt()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("idle", Apply("Interrupt", turn1, t0 + 1));
        Assert.Equal("stale", Apply("PreToolUse", turn1, t0 + 2, Events.Bash("ls", "call_1")));
        Assert.Equal("stale", Apply("Stop", turn1, t0 + 3));
        Assert.Equal(("idle", "Interrupted"), (Chat.State, Chat.Detail));
    }

    [Fact]
    public void PreToolUse_AfterItsOwnPostToolUse_IsStale()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("thinking", Apply("PostToolUse", turn1, t0 + 2, Events.Bash("ls", "call_1")));
        // later by at, but its call has come further already
        Assert.Equal("stale", Apply("PreToolUse", turn1, t0 + 3, Events.Bash("ls", "call_1")));
        Assert.Equal("thinking", Chat.State);
        // another call of the same turn is taken
        Assert.Equal("working", Apply("PreToolUse", turn1, t0 + 4, Events.Bash("ls", "call_2")));
    }

    [Fact]
    public void PreToolUse_AfterItsPermissionRequest_IsStale()
    {
        Apply("UserPromptSubmit", turn1, t0);
        // PermissionRequest has no tool_use_id, and Codex adds a description to a shell command's input
        Assert.Equal("attention", Apply("PermissionRequest", turn1, t0 + 2, Events.Bash("rm -rf build", description: "clean")));
        Assert.Equal("stale", Apply("PreToolUse", turn1, t0 + 3, Events.Bash("rm -rf build", "call_1")));
        Assert.Equal("attention", Chat.State);
        // the call now has its id, so its PostToolUse goes with it
        Assert.Equal("thinking", Apply("PostToolUse", turn1, t0 + 4, Events.Bash("rm -rf build", "call_1")));
        Assert.Equal("stale", Apply("PreToolUse", turn1, t0 + 5, Events.Bash("rm -rf build", "call_1")));
    }

    [Fact]
    public void PreToolUse_OfAnotherCommand_AfterAPermissionRequest_IsTaken()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Apply("PermissionRequest", turn1, t0 + 2, Events.Bash("rm -rf build"));
        Assert.Equal("working", Apply("PreToolUse", turn1, t0 + 3, Events.Bash("ls", "call_2")));
    }

    [Fact]
    public void OwnNewTurn_IsTaken_EvenWhenItsAtIsBeforeThePreviousStop()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("done", Apply("Stop", turn1, t0 + 5));
        // turn 1's Stop hook started late; turn 2 began before it did
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn2, t0 + 1, new JsonObject { ["prompt"] = "go on" }));
        Assert.Equal("working", Apply("PreToolUse", turn2, t0 + 2, Events.Bash("make", "call_9")));
        Assert.Equal("done", Apply("Stop", turn2, t0 + 1.5));
        Assert.Equal("done", Chat.State);
        Assert.Equal("Go on", Chat.Title);
    }

    [Fact]
    public void SubAgent_AfterTheChatsStop_IsStale_UntilTheNextTurn()
    {
        const string agent = "019a0000-0000-7000-8000-00000000a1a1";
        var sub1 = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-30)).ToString();
        var sub2 = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-10)).ToString();
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("working", Apply("PreToolUse", sub1, t0 + 1, Helper(agent, Events.Bash("ls", "sub_1"))));
        Assert.Equal("done", Apply("Stop", turn1, t0 + 2));
        Assert.Equal("stale", Apply("PostToolUse", sub1, t0 + 3, Helper(agent, Events.Bash("ls", "sub_1"))));
        Assert.Equal("stale", Apply("PreToolUse", sub2, t0 + 4, Helper(agent, Events.Bash("pwd", "sub_2"))));
        Assert.Equal("done", Chat.State);
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn2, t0 + 5));
        Assert.Equal("working", Apply("PreToolUse", sub2, t0 + 6, Helper(agent, Events.Bash("pwd", "sub_3"))));
    }

    [Fact]
    public void NonV7TurnIds_OnlyTellASeenTurnFromANewOne()
    {
        string a = Guid.NewGuid().ToString(), b = Guid.NewGuid().ToString(), c = Guid.NewGuid().ToString();
        Assert.Equal('4', a[14]);
        Assert.Equal("thinking", Apply("UserPromptSubmit", a, t0));
        Assert.Equal("thinking", Apply("UserPromptSubmit", b, t0 + 1));
        Assert.Equal("stale", Apply("PreToolUse", a, t0 + 2, Events.Bash("ls", "call_1")));
        Assert.Equal("done", Apply("Stop", b, t0 + 3));
        Assert.Equal("stale", Apply("PreToolUse", b, t0 + 4, Events.Bash("ls", "call_2")));
        // a turn never seen is a new one
        Assert.Equal("thinking", Apply("UserPromptSubmit", c, t0 + 5));
        Assert.Equal("stale", Apply("Stop", b, t0 + 6));
        Assert.Equal("thinking", Chat.State);
    }

    [Fact]
    public void EventsWithoutATurn_GoByAt()
    {
        Apply("UserPromptSubmit", turn1, t0 + 1);
        // SessionStart has no turn: older by at is stale, and a newer one doesn't reset a busy chat
        Assert.Equal("stale", Apply("SessionStart", null, t0));
        Assert.Equal("thinking", Apply("SessionStart", null, t0 + 2));
    }

    [Fact]
    public void LateUserPromptSubmit_OfASeenTurn_KeepsTheState_ButNamesTheChat()
    {
        Apply("UserPromptSubmit", turn1, t0, new JsonObject { ["prompt"] = "first" });
        Apply("Stop", turn1, t0 + 1);
        // PowerShell started turn 2's prompt hook last: its call and the call's request came in first
        Assert.Equal("working", Apply("PreToolUse", turn2, t0 + 2, Events.Bash("cargo test", "call_a")));
        Assert.Equal("attention", Apply("PermissionRequest", turn2, t0 + 2.5, Events.Bash("cargo test")));
        Assert.Equal("stale", Apply("UserPromptSubmit", turn2, t0 + 3.5, new JsonObject { ["prompt"] = "run the tests" }));
        Assert.Equal(("attention", "Needs your permission", "Run the tests"), (Chat.State, Chat.Detail, Chat.Title));
        Assert.Equal(t0 + 2.5, Chat.Ts);
        // nor did it move the chat's latest event: a call that started before it is still newer than the request
        Assert.Equal("working", Apply("PreToolUse", turn2, t0 + 3, Events.Bash("ls", "call_b")));
    }

    [Fact]
    public void UserPromptSubmit_ThenItsTurnsEvents_AreTaken()
    {
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn1, t0, new JsonObject { ["prompt"] = "first" }));
        Assert.Equal("working", Apply("PreToolUse", turn1, t0 + 1, Events.Bash("ls", "call_1")));
        Apply("Stop", turn1, t0 + 2);
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn2, t0 + 3, new JsonObject { ["prompt"] = "second" }));
        Assert.Equal("working", Apply("PreToolUse", turn2, t0 + 4, Events.Bash("ls", "call_2")));
        Assert.Equal("Second", Chat.Title);
    }

    [Fact]
    public void NewChat_FirstEventIsItsPrompt_IsTaken()
    {
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn1, t0, new JsonObject { ["prompt"] = "hello" }));
        Assert.Equal(("thinking", "Hello"), (Chat.State, Chat.Title));
    }

    [Fact]
    public void PermissionRequest_OfARerun_GoesToTheRerun_NotTheEarlierCallOfTheSameCommand()
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        // ran in the sandbox without approval; on Windows no PostToolUse is registered, so it stays at its PreToolUse
        Assert.Equal("working", Apply("PreToolUse", turn1, b + 1, Events.Bash("npm test", "call_1")));
        // the rerun with escalated permissions: its request's hook started before its PreToolUse's
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + 21, Events.Bash("npm test", description: "rerun")));
        Assert.Equal("stale", Apply("PreToolUse", turn1, b + 22.5, Events.Bash("npm test", "call_2")));
        Assert.Equal(("attention", "Needs your permission"), (Chat.State, Chat.Detail));
    }

    [Fact]
    public void PermissionRequest_OfARerun_ArrivingAfterItsPreToolUse_StartedBeforeIt_IsTaken()
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        Apply("PreToolUse", turn1, b + 1, Events.Bash("npm test", "call_1"));
        Assert.Equal("working", Apply("PreToolUse", turn1, b + 22.5, Events.Bash("npm test", "call_2")));
        // goes with call_2, whose PreToolUse started closest to it; that PreToolUse is all that's newer
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + 21, Events.Bash("npm test")));
        Assert.Equal("attention", Chat.State);
        // call_2 has come further, so its PreToolUse coming again is stale
        Assert.Equal("stale", Apply("PreToolUse", turn1, b + 22.6, Events.Bash("npm test", "call_2")));
    }

    // Codex reruns a command that failed in the sandbox with approval a second or two later, and PowerShell starts
    // each hook 1-4 s late, so the rerun's request can start before or after its PreToolUse and arrive either side
    [Theory]
    [InlineData(3.5, 4.0)]
    [InlineData(2.2, 3.9)]
    [InlineData(4.8, 3.1)]
    public void PermissionRequest_OfAFastRerun_ArrivingBeforeItsPreToolUse_KeepsTheChatAsking(double request, double rerun)
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        // no PostToolUse on Windows: call_1 stays at its PreToolUse
        Assert.Equal("working", Apply("PreToolUse", turn1, b + 1, Events.Bash("npm test", "call_1")));
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + request, Events.Bash("npm test", description: "rerun")));
        Assert.Equal("stale", Apply("PreToolUse", turn1, b + rerun, Events.Bash("npm test", "call_2")));
        Assert.Equal(("attention", "Needs your permission"), (Chat.State, Chat.Detail));
        // it came late all the same: coming again is stale too
        Assert.Equal("stale", Apply("PreToolUse", turn1, b + rerun + 0.1, Events.Bash("npm test", "call_2")));
    }

    [Theory]
    [InlineData(3.5, 4.0)]
    [InlineData(2.2, 3.9)]
    [InlineData(4.8, 3.1)]
    public void PermissionRequest_OfAFastRerun_ArrivingAfterItsPreToolUse_IsTaken(double request, double rerun)
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        Apply("PreToolUse", turn1, b + 1, Events.Bash("npm test", "call_1"));
        Assert.Equal("working", Apply("PreToolUse", turn1, b + rerun, Events.Bash("npm test", "call_2")));
        // goes with call_2, the latest of that command; call_1 is closer to it in some of these
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + request, Events.Bash("npm test", description: "rerun")));
        Assert.Equal(("attention", "Needs your permission"), (Chat.State, Chat.Detail));
        Assert.Equal("stale", Apply("PreToolUse", turn1, b + rerun + 0.1, Events.Bash("npm test", "call_2")));
    }

    [Fact]
    public void SameCommandAgain_AfterTheAskingCallsPostToolUse_IsTaken()
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        Apply("PreToolUse", turn1, b + 1, Events.Bash("npm test", "call_1"));
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + 1.2, Events.Bash("npm test")));
        // approved and run at once (a direct install outside Windows has PostToolUse): the request was call_1's
        Assert.Equal("thinking", Apply("PostToolUse", turn1, b + 2, Events.Bash("npm test", "call_1")));
        Assert.Equal("working", Apply("PreToolUse", turn1, b + 3, Events.Bash("npm test", "call_2")));
    }

    [Fact]
    public void PairedRequest_DoesNotMoveTheLatestBack()
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        Apply("PreToolUse", turn1, b + 1, Events.Bash("ls", "call_a"));
        Assert.Equal("working", Apply("PreToolUse", turn1, b + 3, Events.Bash("rm -rf build", "call_b")));
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + 2, Events.Bash("rm -rf build")));
        // started between the request and its PreToolUse, which is still the chat's latest event
        Assert.Equal("stale", Apply("PostToolUse", turn1, b + 2.5, Events.Bash("ls", "call_a")));
        Assert.Equal(("attention", "Needs your permission"), (Chat.State, Chat.Detail));
    }

    [Fact]
    public void LateUserPromptSubmit_AfterItsTurnsStop_StillNamesTheChat()
    {
        Apply("UserPromptSubmit", turn1, t0, new JsonObject { ["prompt"] = "first" });
        Apply("Stop", turn1, t0 + 1);
        // a quick turn finished before PowerShell started its prompt's hook
        Assert.Equal("working", Apply("PreToolUse", turn2, t0 + 2, Events.Bash("ls", "call_1")));
        Assert.Equal("done", Apply("Stop", turn2, t0 + 3));
        Assert.Equal("stale", Apply("UserPromptSubmit", turn2, t0 + 4, new JsonObject { ["prompt"] = "second" }));
        Assert.Equal(("done", "Done", "Second"), (Chat.State, Chat.Detail, Chat.Title));
        Assert.Equal(t0 + 3, Chat.Ts);
    }

    [Fact]
    public void PermissionRequest_BehindAnotherCall_GoesByAt()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Apply("PreToolUse", turn1, t0 + 2, Events.Bash("rm -rf build", "call_1"));
        Assert.Equal("working", Apply("PreToolUse", turn1, t0 + 3, Events.Bash("ls", "call_2")));
        Assert.Equal("stale", Apply("PermissionRequest", turn1, t0 + 1.5, Events.Bash("rm -rf build")));
    }

    [Fact]
    public void TwoIdenticalCalls_FarApart_TheSecondsRequestGoesWithTheSecond()
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        Apply("PreToolUse", turn1, b + 1, Events.Bash("npm test", "call_1"));
        Apply("PreToolUse", turn1, b + 60, Events.Bash("npm test", "call_2"));
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + 60.5, Events.Bash("npm test")));
        Assert.Equal("thinking", Apply("PostToolUse", turn1, b + 70, Events.Bash("npm test", "call_2")));
        // the request went with call_2, so call_2 has come further
        Assert.Equal("stale", Apply("PreToolUse", turn1, b + 71, Events.Bash("npm test", "call_2")));
    }

    [Fact]
    public void PendingRequest_IsNotTakenByTheSameCommandLongAfter()
    {
        // well before now: a time more than 5 s ahead reads as a clock set back (see Stale)
        double b = t0 - 100;
        Apply("UserPromptSubmit", turn1, b);
        // a request whose PreToolUse never came waits on its own, but only for HookSpread
        Assert.Equal("attention", Apply("PermissionRequest", turn1, b + 1, Events.Bash("npm test")));
        Assert.Equal("working", Apply("PreToolUse", turn1, b + 30, Events.Bash("npm test", "call_3")));
    }

    [Fact]
    public void ManualCompact_EndsItsTurn()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("done", Apply("Stop", turn1, t0 + 1));
        // /compact is a turn of its own, with no Stop after it
        var manual = new JsonObject { ["trigger"] = "manual" };
        Assert.Equal("thinking", Apply("PreCompact", turn2, t0 + 2, manual));
        Assert.Equal("Compacting the conversation", Chat.Detail);
        Assert.Equal("done", Apply("PostCompact", turn2, t0 + 3, manual));
        Assert.Equal(("done", "Compacted"), (Chat.State, Chat.Detail));
        Assert.Equal((turn2, true), (Chat.Turn, Chat.TurnEnded));
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn3, t0 + 4));
    }

    [Fact]
    public void ManualCompact_PreCompactArrivingLast_IsStale()
    {
        Apply("UserPromptSubmit", turn1, t0);
        Apply("Stop", turn1, t0 + 1);
        var manual = new JsonObject { ["trigger"] = "manual" };
        Assert.Equal("done", Apply("PostCompact", turn2, t0 + 3, manual));
        Assert.Equal("stale", Apply("PreCompact", turn2, t0 + 4, manual));
        Assert.Equal(("done", "Compacted"), (Chat.State, Chat.Detail));
    }

    [Fact]
    public void AutoCompact_StaysInItsTurn()
    {
        Apply("UserPromptSubmit", turn1, t0);
        var auto = new JsonObject { ["trigger"] = "auto" };
        Assert.Equal("thinking", Apply("PreCompact", turn1, t0 + 1, auto));
        Assert.Equal("thinking", Apply("PostCompact", turn1, t0 + 2, auto));
        Assert.Equal(("thinking", "Thinking", false), (Chat.State, Chat.Detail, Chat.TurnEnded));
        Assert.Equal("working", Apply("PreToolUse", turn1, t0 + 3, Events.Bash("ls", "call_1")));
        Assert.Equal("done", Apply("Stop", turn1, t0 + 4));
    }

    [Fact]
    public void SubAgentsCompact_DoesNotEndTheChatsTurn()
    {
        const string agent = "019a0000-0000-7000-8000-00000000a1a1";
        var sub = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-30)).ToString();
        Apply("UserPromptSubmit", turn1, t0);
        Assert.Equal("thinking", Apply("PostCompact", sub, t0 + 1, Helper(agent, new JsonObject { ["trigger"] = "manual" })));
        Assert.Equal(("thinking", "Thinking", false), (Chat.State, Chat.Detail, Chat.TurnEnded));
        Assert.Equal("working", Apply("PreToolUse", sub, t0 + 2, Helper(agent, Events.Bash("ls", "sub_1"))));
        Assert.Equal("working", Apply("PreToolUse", turn1, t0 + 3, Events.Bash("ls", "call_1")));
    }

    [Fact]
    public void ClockSetBack_NewTurnsAreTaken_AndTheOldTurnStaysStale()
    {
        // turn 1 began while the clock was an hour ahead, so its id sorts after every turn made once it was set right
        var ahead = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddHours(1)).ToString();
        var after = Guid.CreateVersion7(DateTimeOffset.UtcNow).ToString();
        Assert.True(string.CompareOrdinal(after, ahead) < 0);
        Apply("UserPromptSubmit", ahead, t0 + 3600);
        Assert.Equal("done", Apply("Stop", ahead, t0 + 3601));
        Assert.Equal("thinking", Apply("UserPromptSubmit", after, t0, new JsonObject { ["prompt"] = "after" }));
        Assert.Equal("working", Apply("PreToolUse", after, t0 + 1, Events.Bash("ls", "call_1")));
        Assert.Equal("done", Apply("Stop", after, t0 + 2));
        Assert.Equal("After", Chat.Title);
        // a late event of the old turn is still stale: it's a turn seen before
        Assert.Equal("stale", Apply("PreToolUse", ahead, t0 + 3602, Events.Bash("ls", "call_0")));
        Assert.Equal((after, "done"), (Chat.Turn, Chat.State));
    }

    [Fact]
    public void NewChat_StartsFromItsFirstEventsTurn()
    {
        // the first event to arrive is turn 2's: turn 1's events after it are stale
        Assert.Equal("working", Apply("PreToolUse", turn2, t0 + 2, Events.Bash("ls", "call_1")));
        Assert.Equal("stale", Apply("UserPromptSubmit", turn1, t0 + 3));
        Assert.Equal("done", Apply("Stop", turn2, t0 + 4));
        Assert.Equal("thinking", Apply("UserPromptSubmit", turn3, t0 + 5));
    }
}

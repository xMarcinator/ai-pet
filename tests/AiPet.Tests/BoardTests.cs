using System.Globalization;
using System.IO;
using System.Text.Json.Nodes;
using Xunit;

namespace AiPet.Tests;

/// A Codex chat as both the hook and Codex's session files (CodexWatcher) report it: Board orders the two by turn
/// before time, because the hook's `at` is PowerShell's late start on Windows and the log's is Codex's own.
public class BoardMergeTests
{
    readonly AgentSessions hooks = new();
    readonly string sid = Guid.NewGuid().ToString();
    // now minus a little, so a done bubble isn't old enough yet to count as idle
    readonly double t = Board.Unix - 8;
    readonly string turnX = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-20)).ToString();
    readonly string turnY = Guid.CreateVersion7(DateTimeOffset.UtcNow.AddSeconds(-10)).ToString();
    readonly string rollout;

    public BoardMergeTests()
    {
        var now = DateTime.UtcNow;
        var dir = Directory.CreateDirectory(Path.Combine(TestEnv.CodexHome, "sessions", now.ToString("yyyy", CultureInfo.InvariantCulture),
            now.ToString("MM", CultureInfo.InvariantCulture), now.ToString("dd", CultureInfo.InvariantCulture))).FullName;
        rollout = Path.Combine(dir, $"rollout-{now.ToString("yyyy-MM-dd'T'HH-mm-ss", CultureInfo.InvariantCulture)}-{sid}.jsonl");
    }

    string Hook(string ev, string turn, double at, JsonObject extra = null)
    {
        var o = extra ?? new JsonObject();
        o["transcript_path"] = rollout;
        return hooks.Apply(Events.Envelope("codex", at, Events.Payload(sid, ev, turn, o))).Outcome;
    }

    /// Writes the chat's rollout file and has a CodexWatcher read it once.
    CodexWatcher Log(params (double Ts, string Type, string Turn)[] events)
    {
        var lines = new List<string>
        {
            new JsonObject { ["type"] = "session_meta", ["payload"] = new JsonObject { ["id"] = sid, ["cwd"] = "/work/app", ["originator"] = "codex-tui" } }.ToJsonString(),
        };
        foreach (var (ts, type, turn) in events)
        {
            var payload = new JsonObject { ["type"] = type };
            if (turn != null) payload["turn_id"] = turn;
            var when = DateTimeOffset.FromUnixTimeMilliseconds((long)Math.Round(ts * 1000)).ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture);
            lines.Add(new JsonObject { ["timestamp"] = when, ["type"] = "event_msg", ["payload"] = payload }.ToJsonString());
        }
        File.WriteAllLines(rollout, lines);
        var watcher = new CodexWatcher();
        watcher.Start();
        try
        {
            var until = DateTime.UtcNow.AddSeconds(15);
            while (!watcher.Sessions.Any(s => s.Id == "codex:" + sid))
            {
                Assert.True(DateTime.UtcNow < until, "the watcher didn't read the rollout file");
                Thread.Sleep(50);
            }
        }
        finally { watcher.Stop(); }
        return watcher;
    }

    Session Merged(CodexWatcher watcher)
    {
        var board = new Board();
        board.Refresh(hooks, new JiraWatcher(new NoSecrets()), new GitHubWatcher(new NoSecrets()), null, false, watcher);
        return board.Find("codex:" + sid);
    }

    [Fact]
    public void LogsInterrupt_BeatsALaterStartedHookOfThatTurn()
    {
        Hook("UserPromptSubmit", turnX, t - 3);
        // Codex sent it just before the user pressed Esc; PowerShell started its hook after
        Assert.Equal("working", Hook("PreToolUse", turnX, t + 2, Events.Bash("rm -rf build", "call_1")));
        var s = Merged(Log((t - 3.5, "task_started", turnX), (t, "turn_aborted", turnX)));
        Assert.Equal(("log", "idle", "Interrupted"), (s.Source, s.Eff, s.Detail));
        // it keeps its own time, so it doesn't look freshly changed
        Assert.Equal(t, s.Ts, 3);
    }

    [Fact]
    public void LogsErrorEnd_BeatsTheHooksLatePrompt()
    {
        // the turn ended in an error (no Stop hook) before the prompt's hook had even started
        Assert.Equal("thinking", Hook("UserPromptSubmit", turnX, t + 2, new JsonObject { ["prompt"] = "go" }));
        var s = Merged(Log((t, "task_started", turnX), (t + 0.5, "task_complete", turnX)));
        Assert.Equal(("log", "done", "Done"), (s.Source, s.Eff, s.Detail));
        // the hook's first prompt still names it
        Assert.Equal("Go", s.Name);
    }

    [Fact]
    public void HookOfALaterTurn_BeatsTheLogsOlderEnd()
    {
        Hook("UserPromptSubmit", turnX, t - 5);
        Assert.Equal("thinking", Hook("UserPromptSubmit", turnY, t));
        // written after the hook's at, but of the turn before
        var s = Merged(Log((t - 5.5, "task_started", turnX), (t + 1, "turn_aborted", turnX)));
        Assert.Equal(("hook", "thinking"), (s.Source, s.Eff));
        // the hook's own time: the chat showed "Thinking" already, so turn 2's prompt didn't move it
        Assert.Equal(t - 5, s.Ts, 3);
    }

    [Fact]
    public void BothHaveTheTurnsEnd_TheNewerReportWins()
    {
        Hook("UserPromptSubmit", turnX, t - 2);
        Assert.Equal("done", Hook("PostCompact", turnX, t + 2, new JsonObject { ["trigger"] = "manual" }));
        var s = Merged(Log((t - 2.5, "task_started", turnX), (t + 1, "task_complete", turnX)));
        Assert.Equal(("hook", "done", "Compacted"), (s.Source, s.Eff, s.Detail));
    }

    [Fact]
    public void WithoutTurnIds_TheNewerReportWins()
    {
        // an older Codex whose log names no turns
        Hook("UserPromptSubmit", turnX, t - 3);
        Hook("PreToolUse", turnX, t + 2, Events.Bash("ls", "call_1"));
        var s = Merged(Log((t - 3.5, "task_started", null), (t, "turn_aborted", null)));
        Assert.Equal(("hook", "working"), (s.Source, s.Eff));
    }

    [Fact]
    public void LogOfTheSameTurnStillGoing_GoesByTime()
    {
        Hook("UserPromptSubmit", turnX, t - 3);
        Hook("PreToolUse", turnX, t + 2, Events.Bash("ls", "call_1"));
        var s = Merged(Log((t - 3.5, "task_started", turnX)));
        Assert.Equal(("hook", "working"), (s.Source, s.Eff));
    }
}

/// The reviews stack: Jira issues, GitHub review requests and the watchers' errors.
public class BoardReviewTests
{
    readonly JiraWatcher jira = new(new NoSecrets());
    readonly GitHubWatcher github = new(new NoSecrets());
    readonly double now = Board.Unix;

    public BoardReviewTests()
    {
        // whatever settings another test saved, these start off
        jira.Config.Enabled = false;
        github.Config.Enabled = false;
    }

    /// Sets what a watcher's poll would have (the setters are private).
    static void Set(object watcher, string property, object value) => watcher.GetType().GetProperty(property).SetValue(watcher, value);

    void Issues(int n, string status = "In Progress") =>
        Set(jira, nameof(JiraWatcher.Issues), Enumerable.Range(1, n)
            .Select(i => new JiraWatcher.Issue($"ABC-{i}", "Task " + i, status, now - i, $"https://example.atlassian.net/browse/ABC-{i}")).ToList());

    Board Refresh()
    {
        var board = new Board();
        board.Refresh(new AgentSessions(), jira, github, null, false);
        return board;
    }

    /// The reviews header's count (MainWindow.LayoutCards).
    static int Header(Board board) => board.Cards.Count(x => x.Kind is "jira" or "github") + board.Extra["reviews"];

    [Fact]
    public void WatchersError_IsShownFirst_AndNotCountedAsAReview()
    {
        Issues(4);
        github.Config.Enabled = true;
        Set(github, nameof(GitHubWatcher.LastError), "GitHub didn't accept the token");
        var board = Refresh();
        var reviews = board.Cards.Where(c => c.Section == "reviews").ToList();
        Assert.Equal(4, reviews.Count);
        Assert.Equal("gh:_error", reviews[0].Id);
        Assert.Equal(1, board.Extra["reviews"]);
        Assert.Equal(4, Header(board));
    }

    [Fact]
    public void BothWatchersErrors_AreShown_AndOnlyReviewsOverflow()
    {
        Issues(5);
        jira.Config.Enabled = true;
        Set(jira, nameof(JiraWatcher.LastError), "Jira didn't accept the email/API token");
        github.Config.Enabled = true;
        Set(github, nameof(GitHubWatcher.LastError), "GitHub didn't accept the token");
        var board = Refresh();
        var reviews = board.Cards.Where(c => c.Section == "reviews").ToList();
        Assert.Equal(4, reviews.Count);
        Assert.Equal(new[] { "github-error", "jira-error" }, reviews.Take(2).Select(c => c.Kind).OrderBy(k => k));
        Assert.Equal(3, board.Extra["reviews"]);
        Assert.Equal(5, Header(board));
    }

    [Fact]
    public void FewReviews_AndAnError_AllShow()
    {
        Issues(2);
        github.Config.Enabled = true;
        Set(github, nameof(GitHubWatcher.LastError), "GitHub didn't accept the token");
        var board = Refresh();
        Assert.Equal(3, board.Cards.Count(c => c.Section == "reviews"));
        Assert.Equal(0, board.Extra["reviews"]);
        Assert.Equal(2, Header(board));
    }

    [Fact]
    public void JiraIssue_SaysItsStatus_NotThatAReviewWasRequested()
    {
        Issues(2, "In Progress");
        Set(github, nameof(GitHubWatcher.PrForKey), new Dictionary<string, string> { ["ABC-1"] = "https://github.com/owner/app/pull/12" });
        var board = Refresh();
        Assert.Equal("In Progress · PR app#12", board.Find("jira:ABC-1").Detail);
        Assert.Equal("In Progress", board.Find("jira:ABC-2").Detail);
    }
}

/// No saved tokens.
sealed class NoSecrets : ISecretStore
{
    public string Read(string key) => null;
    public void Write(string key, string user, string secret) { }
    public void Delete(string key) { }
}

using Xunit;

namespace AiPet.Tests;

/// "Import defaults…" in Settings reads a team's preset file: Jira and GitHub settings, never credentials.
public class PresetTests
{
    const string Full = """
        {
          "version": 1,
          "jira": { "site": " team.atlassian.net ", "jql": "Reviewer = currentUser() AND status = Review", "enabled": true },
          "github": { "host": "github.example.com", "orgs": ["acme", " tools ", ""], "jiraProjects": ["ABC"], "enabled": true }
        }
        """;

    static JiraWatcher.Settings Mine() => new() { Enabled = false, Site = "old.atlassian.net", Email = "me@example.com", Jql = "project = X", PollSeconds = 300 };
    static GitHubWatcher.Settings MyGitHub() => new() { Enabled = false, Host = "github.com", Orgs = new[] { "mine" }, JiraProjects = new[] { "OLD" }, PollSeconds = 600 };

    [Fact]
    public void AFullPreset_SetsEveryField_AndKeepsTheUsersOwn()
    {
        var p = Preset.Parse(Full);
        Assert.True(p.HasJira);
        Assert.True(p.HasGitHub);

        var j = p.ApplyTo(Mine());
        Assert.Equal((true, "team.atlassian.net", "Reviewer = currentUser() AND status = Review"), (j.Enabled, j.Site, j.Jql));
        Assert.Equal(("me@example.com", 300), (j.Email, j.PollSeconds));

        var g = p.ApplyTo(MyGitHub());
        Assert.Equal((true, "github.example.com"), (g.Enabled, g.Host));
        Assert.Equal(new[] { "acme", "tools" }, g.Orgs);
        Assert.Equal(new[] { "ABC" }, g.JiraProjects);
        Assert.Equal(600, g.PollSeconds);
        Assert.Equal("Jira (site, search, on) and GitHub (host, 2 organisations, 1 Jira project, on)", p.Describe());
    }

    [Fact]
    public void APartialPreset_ChangesOnlyWhatItHas()
    {
        var p = Preset.Parse("""{"version":1,"github":{"orgs":["acme"]},"jira":null}""");
        Assert.False(p.HasJira);
        Assert.True(p.HasGitHub);
        var g = p.ApplyTo(MyGitHub());
        Assert.Equal((false, "github.com", 600), (g.Enabled, g.Host, g.PollSeconds));
        Assert.Equal(new[] { "acme" }, g.Orgs);
        Assert.Equal(new[] { "OLD" }, g.JiraProjects);
        Assert.Equal("GitHub (1 organisation)", p.Describe());

        var j = Preset.Parse("""{"version":1,"jira":{"enabled":false}}""").ApplyTo(Mine());
        Assert.Equal((false, "old.atlassian.net", "me@example.com", "project = X"), (j.Enabled, j.Site, j.Email, j.Jql));
    }

    [Fact]
    public void BlankFields_MeanTheDefaults_AsSettingsSaveDoes()
    {
        var p = Preset.Parse("""{"version":1,"jira":{"jql":"  "},"github":{"host":""}}""");
        Assert.Equal(JiraWatcher.DefaultJql, p.ApplyTo(Mine()).Jql);
        Assert.Equal("github.com", p.ApplyTo(new GitHubWatcher.Settings { Host = "ghe.example.com" }).Host);
    }

    [Fact]
    public void NoSections_IsAValidPresetWithNothingInIt()
    {
        var p = Preset.Parse("""{"version":1,"jira":{},"theme":"dark"}""");
        Assert.False(p.HasJira || p.HasGitHub);
        Assert.Equal("nothing", p.Describe());
    }

    [Fact]
    public void TokensAndEmail_AreNeverRead()
    {
        // even of a type that would be an error for a field that is read
        var p = Preset.Parse("""
            {"version":1,"token":"t0",
             "jira":{"site":"team.atlassian.net","email":"boss@example.com","token":"t1","apiToken":42},
             "github":{"token":"ghp_x","accessToken":["x"],"orgs":[]}}
            """);
        var j = p.ApplyTo(Mine());
        Assert.Equal("me@example.com", j.Email);
        Assert.Empty(p.ApplyTo(MyGitHub()).Orgs);
        // nothing in a preset could hold one
        var names = typeof(Preset).GetMembers().Select(m => m.Name.ToLowerInvariant());
        Assert.DoesNotContain(names, n => n.Contains("token") || n.Contains("email") || n.Contains("secret"));
    }

    [Theory]
    [InlineData("""{"jira":{"site":"x"}}""", "no \"version\"")]
    [InlineData("""{"version":2}""", "version 2")]
    [InlineData("""{"version":"1"}""", "\"version\"")]
    [InlineData("""{"version":1.5}""", "\"version\"")]
    [InlineData("""[1]""", "isn't an AiPet preset")]
    [InlineData("""{"version":1,""", "valid JSON")]
    [InlineData("""{"version":1,"jira":[]}""", "\"jira\"")]
    [InlineData("""{"version":1,"jira":{"site":5}}""", "\"jira.site\"")]
    [InlineData("""{"version":1,"jira":{"enabled":"yes"}}""", "\"jira.enabled\"")]
    [InlineData("""{"version":1,"github":{"orgs":"acme, tools"}}""", "\"github.orgs\"")]
    [InlineData("""{"version":1,"github":{"jiraProjects":["ABC",7]}}""", "\"github.jiraProjects\"")]
    [InlineData("""{"version":1,"github":{"host":true}}""", "\"github.host\"")]
    public void AFileThatIsntAPreset_IsRefusedWithAReason(string json, string reason)
    {
        var ex = Assert.Throws<FormatException>(() => Preset.Parse(json));
        Assert.Contains(reason, ex.Message);
    }

    // Settings removes a saved token before a preset points it at another site or host, so a shared file can't
    // have your credentials sent where it likes.
    [Theory]
    [InlineData("""{"version":1,"jira":{"site":"evil.example"}}""", true)]
    [InlineData("""{"version":1,"jira":{"site":"https://Old.Atlassian.net/"}}""", false)]
    [InlineData("""{"version":1,"jira":{"site":" old.atlassian.net "}}""", false)]
    [InlineData("""{"version":1,"jira":{"site":""}}""", false)]
    [InlineData("""{"version":1,"jira":{"jql":"project = Y","enabled":true}}""", false)]
    public void APresetThatPointsJiraElsewhere_IsAMove(string json, bool moves) =>
        Assert.Equal(moves, Preset.Parse(json).MovesJira(Mine()));

    [Theory]
    [InlineData("""{"version":1,"github":{"host":"evil.example"}}""", true)]
    [InlineData("""{"version":1,"github":{"host":"GitHub.com"}}""", false)]
    [InlineData("""{"version":1,"github":{"host":""}}""", false)]
    [InlineData("""{"version":1,"github":{"orgs":["acme"],"enabled":true}}""", false)]
    public void APresetThatPointsGitHubElsewhere_IsAMove(string json, bool moves) =>
        Assert.Equal(moves, Preset.Parse(json).MovesGitHub(MyGitHub()));

    [Fact]
    public void AFirstSite_IsAMoveToo()
    {
        // a token saved before any site was typed would otherwise go to whatever site the file names
        var p = Preset.Parse("""{"version":1,"jira":{"site":"team.atlassian.net"}}""");
        Assert.True(p.MovesJira(new JiraWatcher.Settings()));
    }
}

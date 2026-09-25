using System.Text.Json;

namespace AiPet;

/// A team's defaults for Jira and GitHub, from a file the user picks in Settings ("Import defaults…"):
///   {"version":1,"jira":{"site":…,"jql":…,"enabled":…},"github":{"host":…,"orgs":[…],"jiraProjects":[…],"enabled":…}}
/// Every section and field is optional and unknown fields are ignored. A preset never carries credentials: tokens and
/// the Jira email are each user's own, so no such field is read, even when a file has one.
public sealed class Preset
{
    public string JiraSite, JiraJql, GitHubHost;
    public bool? JiraEnabled, GitHubEnabled;
    public string[] GitHubOrgs, GitHubJiraProjects;

    public bool HasJira => JiraSite != null || JiraJql != null || JiraEnabled != null;
    public bool HasGitHub => GitHubHost != null || GitHubOrgs != null || GitHubJiraProjects != null || GitHubEnabled != null;

    /// Reads a preset file. Throws FormatException, with a message for the user, when it isn't a preset this app reads.
    public static Preset Parse(string json)
    {
        JsonDocument doc;
        try { doc = JsonDocument.Parse(json, new JsonDocumentOptions { CommentHandling = JsonCommentHandling.Skip, AllowTrailingCommas = true }); }
        catch (JsonException ex) { throw new FormatException($"The file isn't valid JSON (line {(ex.LineNumber ?? 0) + 1})."); }
        using (doc)
        {
            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object) throw new FormatException("The file isn't an AiPet preset.");
            if (!root.TryGetProperty("version", out var v)) throw new FormatException("The file isn't an AiPet preset: it has no \"version\".");
            if (v.ValueKind != JsonValueKind.Number || !v.TryGetInt32(out var version))
                throw new FormatException("\"version\" must be a whole number, e.g. 1.");
            if (version != 1) throw new FormatException($"The preset is version {version}; this AiPet reads version 1.");

            var p = new Preset();
            if (Section(root, "jira") is { } jira)
            {
                p.JiraSite = Text(jira, "jira", "site");
                p.JiraJql = Text(jira, "jira", "jql");
                p.JiraEnabled = Switch(jira, "jira", "enabled");
            }
            if (Section(root, "github") is { } github)
            {
                p.GitHubHost = Text(github, "github", "host");
                p.GitHubOrgs = Names(github, "github", "orgs");
                p.GitHubJiraProjects = Names(github, "github", "jiraProjects");
                p.GitHubEnabled = Switch(github, "github", "enabled");
            }
            return p;
        }
    }

    // A null counts as left out, like a missing field.
    static JsonElement? Get(JsonElement o, string name) =>
        o.TryGetProperty(name, out var e) && e.ValueKind != JsonValueKind.Null ? e : null;

    static JsonElement? Section(JsonElement root, string name) => Get(root, name) switch
    {
        null => null,
        { ValueKind: JsonValueKind.Object } e => e,
        _ => throw new FormatException($"\"{name}\" must be a section in braces ({{ … }})."),
    };

    static string Text(JsonElement o, string section, string name) => Get(o, name) switch
    {
        null => null,
        { ValueKind: JsonValueKind.String } e => e.GetString(),
        _ => throw new FormatException($"\"{section}.{name}\" must be text in quotes."),
    };

    static bool? Switch(JsonElement o, string section, string name) => Get(o, name) switch
    {
        null => null,
        { ValueKind: JsonValueKind.True } => true,
        { ValueKind: JsonValueKind.False } => false,
        _ => throw new FormatException($"\"{section}.{name}\" must be true or false."),
    };

    static string[] Names(JsonElement o, string section, string name)
    {
        if (Get(o, name) is not { } e) return null;
        if (e.ValueKind != JsonValueKind.Array || e.EnumerateArray().Any(x => x.ValueKind != JsonValueKind.String))
            throw new FormatException($"\"{section}.{name}\" must be a list of names in quotes, e.g. [\"one\", \"two\"].");
        return e.EnumerateArray().Select(x => x.GetString().Trim()).Where(x => x.Length > 0).ToArray();
    }

    /// The Jira settings with the preset's fields in place of the current ones, tidied as Settings' Save does.
    /// The email and the poll interval stay the user's.
    public JiraWatcher.Settings ApplyTo(JiraWatcher.Settings current) => new()
    {
        Enabled = JiraEnabled ?? current.Enabled,
        Site = JiraSite?.Trim() ?? current.Site,
        Email = current.Email,
        Jql = JiraJql == null ? current.Jql : string.IsNullOrWhiteSpace(JiraJql) ? JiraWatcher.DefaultJql : JiraJql.Trim(),
        PollSeconds = current.PollSeconds,
    };

    /// The GitHub settings with the preset's fields in place of the current ones, tidied as Settings' Save does.
    public GitHubWatcher.Settings ApplyTo(GitHubWatcher.Settings current) => new()
    {
        Enabled = GitHubEnabled ?? current.Enabled,
        Host = GitHubHost == null ? current.Host : string.IsNullOrWhiteSpace(GitHubHost) ? "github.com" : GitHubHost.Trim(),
        Orgs = GitHubOrgs ?? current.Orgs,
        JiraProjects = GitHubJiraProjects ?? current.JiraProjects,
        PollSeconds = current.PollSeconds,
    };

    /// Whether the preset points Jira at another site than the saved one. A saved token goes wherever the settings
    /// point, so Settings removes it first rather than send it to a site the user never typed in.
    public bool MovesJira(JiraWatcher.Settings current) => JiraSite != null && Moves(ApplyTo(current).Site, current.Site);

    /// Whether the preset points GitHub at another host than the saved one (see MovesJira).
    public bool MovesGitHub(GitHubWatcher.Settings current) => GitHubHost != null && Moves(ApplyTo(current).Host, current.Host);

    // Compared as JiraWatcher reads a site. No site at all sends a token nowhere, so that isn't a move.
    static bool Moves(string to, string from)
    {
        static string Address(string s) => (s ?? "").Trim().Replace("https://", "").Replace("http://", "").TrimEnd('/');
        return Address(to).Length > 0 && !string.Equals(Address(to), Address(from), StringComparison.OrdinalIgnoreCase);
    }

    /// What it sets, for the line Settings shows after an import, e.g. "Jira (site, search) and GitHub (2 organisations)".
    public string Describe()
    {
        static string Count(int n, string one) => n == 1 ? "1 " + one : $"{n} {one}s";
        static string On(bool? v) => v switch { true => "on", false => "off", _ => null };
        string Part(string name, params string[] fields) =>
            fields.Any(f => f != null) ? $"{name} ({string.Join(", ", fields.Where(f => f != null))})" : null;
        var parts = new[]
        {
            Part("Jira", JiraSite != null ? "site" : null, JiraJql != null ? "search" : null, On(JiraEnabled)),
            Part("GitHub", GitHubHost != null ? "host" : null, GitHubOrgs != null ? Count(GitHubOrgs.Length, "organisation") : null,
                 GitHubJiraProjects != null ? Count(GitHubJiraProjects.Length, "Jira project") : null, On(GitHubEnabled)),
        }.Where(x => x != null).ToList();
        return parts.Count == 0 ? "nothing" : string.Join(" and ", parts);
    }
}

using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet;

/// Watches GitHub for pull requests waiting on your review, via GitHub's GraphQL API over HTTPS.
/// Token: only one you saved yourself in Settings (kept in the platform's secret store); no other
/// credentials on the machine are used. Without a saved token, GitHub reviews are just off.
/// Each PR is tagged with the Jira keys in its title, branch, description or commit messages, so the pet can
/// merge it with the matching Jira review; Jira reviews without a review request get their PR looked up too.
public sealed class GitHubWatcher
{
    public sealed class Settings
    {
        /// Off until you turn it on (pasting a token in Settings ticks it).
        public bool Enabled { get; set; }
        /// github.com, or a GitHub Enterprise Server host.
        public string Host { get; set; } = "github.com";
        /// Organisations searched when looking up the PR for a Jira task. With none, only pull requests that involve
        /// you are searched (never all of GitHub).
        public string[] Orgs { get; set; } = Array.Empty<string>();
        /// Jira project keys recognised in PR text (e.g. ABC-123). Keys of watched Jira issues are always recognised.
        public string[] JiraProjects { get; set; } = Array.Empty<string>();
        public int PollSeconds { get; set; } = 120;
    }

    public sealed record PullRequest(string Repo, int Number, string Title, string Url, double Updated, string[] Keys);

    public const string TokenKey = "AiPet:GitHub";
    static string SettingsPath => Path.Combine(Paths.DataDir, "github.json");
    static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(20) };

    public Settings Config { get; private set; } = new();
    public IReadOnlyList<PullRequest> ReviewRequests { get; private set; } = Array.Empty<PullRequest>();
    /// Jira key -> URL of the most relevant PR mentioning it (open PRs first).
    public IReadOnlyDictionary<string, string> PrForKey { get; private set; } = new Dictionary<string, string>();
    public string LastError { get; private set; }
    public event Action Changed;
    public event Action<int> NewReviews;

    /// Supplies the Jira keys currently shown, so their PRs can be looked up.
    public Func<IReadOnlyCollection<string>> JiraKeys { get; set; } = () => Array.Empty<string>();

    readonly ISecretStore secrets;
    readonly HashSet<string> seen = new();
    readonly Dictionary<string, (string Url, DateTime At)> keyCache = new();
    bool firstPoll = true;
    CancellationTokenSource loop;

    public GitHubWatcher(ISecretStore secrets)
    {
        this.secrets = secrets;
        try { Config = JsonSerializer.Deserialize<Settings>(File.ReadAllText(SettingsPath)) ?? new Settings(); } catch { }
    }

    public bool HasSavedToken => !string.IsNullOrEmpty(secrets.Read(TokenKey));

    public void Save(Settings s, string newToken = null)
    {
        Config = s;
        Directory.CreateDirectory(Paths.DataDir);
        File.WriteAllText(SettingsPath, JsonSerializer.Serialize(s, new JsonSerializerOptions { WriteIndented = true }));
        if (!string.IsNullOrEmpty(newToken)) secrets.Write(TokenKey, "github", newToken.Trim());
        firstPoll = true; seen.Clear(); keyCache.Clear();
        Restart();
    }

    /// Remove the saved access token; GitHub reviews stop until a new one is saved.
    public void ForgetToken()
    {
        secrets.Delete(TokenKey);
        firstPoll = true; seen.Clear(); keyCache.Clear();
        Restart();
    }

    public void Restart()
    {
        loop?.Cancel();
        loop = new CancellationTokenSource();
        var ct = loop.Token;
        _ = Task.Run(async () =>
        {
            await Task.Delay(4000, ct);  // let the first Jira poll land so its keys can be looked up
            while (!ct.IsCancellationRequested)
            {
                await PollAsync();
                await Task.Delay(TimeSpan.FromSeconds(Math.Max(60, Config.PollSeconds)), ct);
            }
        }, ct);
    }

    /// Re-run the Jira-key lookups soon (called when the Jira list changes).
    public void JiraChanged() => _ = Task.Run(async () => { await LookupJiraKeysAsync(); Changed?.Invoke(); });

    // ------------------------------------------------------------------ token
    string Token() => secrets.Read(TokenKey) is { Length: > 0 } saved ? saved : null;

    // ------------------------------------------------------------------ polling
    async Task PollAsync()
    {
        var token = Config.Enabled ? Token() : null;
        if (token == null)
        {
            ReviewRequests = Array.Empty<PullRequest>();
            LastError = null;
            Changed?.Invoke();
            return;
        }
        const string query = """
            query($q: String!) {
              search(query: $q, type: ISSUE, first: 30) {
                nodes { ... on PullRequest {
                  number title url body headRefName updatedAt
                  repository { nameWithOwner }
                  commits(last: 30) { nodes { commit { message } } }
                } }
              }
            }
            """;
        var (json, error) = await GraphQLAsync(token, query, new JsonObject { ["q"] = "is:open is:pr review-requested:@me archived:false" });
        LastError = error;
        if (json != null)
        {
            var list = new List<PullRequest>();
            try
            {
                using var doc = JsonDocument.Parse(json);
                foreach (var n in doc.RootElement.GetProperty("data").GetProperty("search").GetProperty("nodes").EnumerateArray())
                {
                    if (!n.TryGetProperty("number", out var num)) continue;
                    var text = new StringBuilder();
                    text.AppendLine(Str(n, "title")).AppendLine(Str(n, "headRefName")).AppendLine(Str(n, "body"));
                    if (n.TryGetProperty("commits", out var commits))
                        foreach (var c in commits.GetProperty("nodes").EnumerateArray())
                            text.AppendLine(c.GetProperty("commit").GetProperty("message").GetString());
                    double updated = DateTimeOffset.TryParse(Str(n, "updatedAt"), out var u) ? u.ToUnixTimeMilliseconds() / 1000.0 : 0;
                    list.Add(new PullRequest(n.GetProperty("repository").GetProperty("nameWithOwner").GetString(),
                        num.GetInt32(), Str(n, "title"), Str(n, "url"), updated, FindKeys(text.ToString())));
                }
            }
            catch (Exception ex) { LastError = "Couldn't read GitHub's answer: " + ex.Message; list = null; }
            if (list != null)
            {
                int fresh = list.Count(p => !seen.Contains(p.Url));
                foreach (var p in list) seen.Add(p.Url);
                ReviewRequests = list;
                if (!firstPoll && fresh > 0) NewReviews?.Invoke(fresh);
                firstPoll = false;
            }
        }
        await LookupJiraKeysAsync();
        Changed?.Invoke();
    }

    /// For Jira tasks with no review-requested PR, search the orgs for a PR that mentions the key. With no orgs set,
    /// the search is limited to PRs that involve you (you wrote it, are assigned to it, were mentioned in it or
    /// commented on it): a bare "KEY in:title,body" would search every public PR on GitHub.
    async Task LookupJiraKeysAsync()
    {
        var token = Config.Enabled ? Token() : null;
        if (token == null) { PrForKey = new Dictionary<string, string>(); return; }
        var map = new Dictionary<string, string>();
        foreach (var pr in ReviewRequests)
            foreach (var k in pr.Keys) map.TryAdd(k, pr.Url);

        var missing = JiraKeys().Where(k => !map.ContainsKey(k)).Distinct().ToList();
        var toFetch = missing.Where(k => !keyCache.TryGetValue(k, out var c) || DateTime.Now - c.At > TimeSpan.FromMinutes(10)).ToList();
        if (toFetch.Count > 0)
        {
            // one GraphQL call with an aliased search per key
            var orgs = string.Join(" ", (Config.Orgs ?? Array.Empty<string>()).Where(o => !string.IsNullOrWhiteSpace(o)).Select(o => "org:" + o.Trim()));
            var scope = orgs.Length > 0 ? orgs : "involves:@me";
            var parts = toFetch.Select((k, i) =>
                $"k{i}: search(query: \"is:pr {k} in:title,body {scope}\", type: ISSUE, first: 5) " +
                "{ nodes { ... on PullRequest { url state updatedAt } } }");
            var (json, _) = await GraphQLAsync(token, "{ " + string.Join(" ", parts) + " }", null);
            if (json != null)
                try
                {
                    using var doc = JsonDocument.Parse(json);
                    var data = doc.RootElement.GetProperty("data");
                    for (int i = 0; i < toFetch.Count; i++)
                    {
                        string url = null;
                        if (data.TryGetProperty("k" + i, out var res))
                            url = res.GetProperty("nodes").EnumerateArray()
                                .Where(n => n.TryGetProperty("url", out _))
                                .OrderBy(n => Str(n, "state") == "OPEN" ? 0 : 1)
                                .ThenByDescending(n => Str(n, "updatedAt"))
                                .Select(n => Str(n, "url")).FirstOrDefault();
                        keyCache[toFetch[i]] = (url, DateTime.Now);
                    }
                }
                catch { }
        }
        foreach (var k in missing)
            if (keyCache.TryGetValue(k, out var c) && c.Url != null) map[k] = c.Url;
        PrForKey = map;
    }

    string[] FindKeys(string text)
    {
        var projects = (Config.JiraProjects ?? Array.Empty<string>()).Concat(JiraKeys().Select(k => k.Split('-')[0]))
            .Select(p => p.Trim().ToUpperInvariant()).Where(p => p.Length > 0).Distinct().ToList();
        if (projects.Count == 0) return Array.Empty<string>();
        var rx = new Regex(@"(?<![A-Z0-9])(" + string.Join("|", projects.Select(Regex.Escape)) + @")-(\d+)", RegexOptions.IgnoreCase);
        return rx.Matches(text ?? "").Select(m => m.Groups[1].Value.ToUpperInvariant() + "-" + m.Groups[2].Value).Distinct().ToArray();
    }

    static string Str(JsonElement e, string name) =>
        e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : "";

    /// Check a host and token without saving anything: who they sign in as, and how many pull requests wait on that
    /// account's review (the Test button in Settings). Uses the saved token when none is typed.
    public async Task<(bool Ok, string Message)> TestAsync(string host, string typedToken)
    {
        var token = !string.IsNullOrWhiteSpace(typedToken) ? typedToken.Trim() : Token();
        if (token == null) return (false, "Paste an access token first.");
        const string query = """
            { viewer { login }
              search(query: "is:open is:pr review-requested:@me archived:false", type: ISSUE, first: 1) { issueCount } }
            """;
        var (json, error) = await GraphQLAsync(host, token, query, null);
        if (json == null) return (false, error);
        try
        {
            using var doc = JsonDocument.Parse(json);
            var data = doc.RootElement.GetProperty("data");
            var login = data.GetProperty("viewer").GetProperty("login").GetString();
            int waiting = data.GetProperty("search").GetProperty("issueCount").GetInt32();
            return (true, $"Connected as {login}. {waiting} pull request(s) waiting on your review.");
        }
        catch (Exception ex) { return (false, "Couldn't read GitHub's answer: " + ex.Message); }
    }

    Task<(string Json, string Error)> GraphQLAsync(string token, string query, JsonObject variables) =>
        GraphQLAsync(Config.Host, token, query, variables);

    /// POST a GraphQL query. Returns the response JSON, or null and a readable error.
    static async Task<(string Json, string Error)> GraphQLAsync(string hostName, string token, string query, JsonObject variables)
    {
        // read as Jira reads its site (and Preset compares hosts): a typed http:// is dropped too, never kept in the name
        var host = string.IsNullOrWhiteSpace(hostName) ? "github.com" : hostName.Trim().Replace("https://", "").Replace("http://", "").TrimEnd('/');
        var endpoint = host == "github.com" ? "https://api.github.com/graphql" : $"https://{host}/api/graphql";
        var body = new JsonObject { ["query"] = query };
        if (variables != null) body["variables"] = variables;
        try
        {
            using var req = new HttpRequestMessage(HttpMethod.Post, endpoint)
            { Content = new StringContent(body.ToJsonString(), Encoding.UTF8, "application/json") };
            req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
            req.Headers.UserAgent.ParseAdd("AiPet/1.0");
            using var resp = await Http.SendAsync(req);
            var text = await resp.Content.ReadAsStringAsync();
            if (resp.StatusCode == System.Net.HttpStatusCode.Unauthorized)
                return (null, "GitHub didn't accept the token");
            if (!resp.IsSuccessStatusCode) return (null, $"GitHub returned {(int)resp.StatusCode} {resp.ReasonPhrase}");
            using (var doc = JsonDocument.Parse(text))
                if (doc.RootElement.TryGetProperty("errors", out var errs) && errs.GetArrayLength() > 0 &&
                    !doc.RootElement.TryGetProperty("data", out _))
                    return (null, "GitHub search failed: " + errs[0].GetProperty("message").GetString());
            return (text, null);
        }
        catch (TaskCanceledException) { return (null, "GitHub didn't answer in time"); }
        catch (HttpRequestException ex) { return (null, "Couldn't reach GitHub: " + ex.Message); }
        catch (Exception ex) { return (null, "GitHub search failed: " + ex.Message); }
    }
}

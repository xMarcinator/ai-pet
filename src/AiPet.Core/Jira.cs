using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;

namespace AiPet;

/// Watches Jira Cloud for the issues a JQL search finds (the ones waiting on you) and exposes them to the pet.
/// Settings live in Paths.DataDir/jira.json; the API token lives in the platform's secret store
/// (Credential Manager on Windows, the libsecret keyring on Linux).
/// The defaults are neutral: no site (so it stays off until you enter one) and a JQL for your own open issues.
/// A saved jira.json always wins, so existing settings are kept as they are.
public sealed class JiraWatcher
{
    public sealed class Settings
    {
        public bool Enabled { get; set; }
        /// Your Jira Cloud address, e.g. your-team.atlassian.net. Empty until you enter one.
        public string Site { get; set; } = "";
        public string Email { get; set; } = "";
        public string Jql { get; set; } = DefaultJql;
        public int PollSeconds { get; set; } = 120;
    }

    public sealed record Issue(string Key, string Summary, string Status, double Updated, string Url);

    /// Uses only standard fields, so it works on every Jira site. A review workflow can use something like
    /// "Reviewer = currentUser() AND status = Review ORDER BY updated DESC" instead.
    public const string DefaultJql = "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";
    public const string CredTarget = "AiPet:Jira";
    static string SettingsPath => Path.Combine(Paths.DataDir, "jira.json");

    public Settings Config { get; private set; } = new();
    public IReadOnlyList<Issue> Issues { get; private set; } = Array.Empty<Issue>();
    public string LastError { get; private set; }
    public DateTime? LastChecked { get; private set; }

    /// Raised on the thread pool after each poll.
    public event Action Changed;
    /// Raised with the number of issues that weren't in the previous result.
    public event Action<int> NewReviews;

    static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(20) };
    readonly HashSet<string> seen = new();
    bool firstPoll = true;
    CancellationTokenSource loop;

    readonly ISecretStore secrets;

    public JiraWatcher(ISecretStore secrets)
    {
        this.secrets = secrets;
        try { Config = JsonSerializer.Deserialize<Settings>(File.ReadAllText(SettingsPath)) ?? new Settings(); } catch { }
    }

    public bool HasToken => !string.IsNullOrEmpty(secrets.Read(CredTarget));
    public bool IsConfigured => Config.Enabled && !string.IsNullOrWhiteSpace(Config.Site) &&
                                !string.IsNullOrWhiteSpace(Config.Email) && HasToken;

    public void Save(Settings settings, string newToken)
    {
        Config = settings;
        Directory.CreateDirectory(Paths.DataDir);
        File.WriteAllText(SettingsPath, JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true }));
        if (!string.IsNullOrEmpty(newToken)) secrets.Write(CredTarget, settings.Email, newToken);
        firstPoll = true;
        seen.Clear();
        Restart();
    }

    /// Remove the saved API token; Jira reviews stop until a new one is saved.
    public void ForgetToken()
    {
        secrets.Delete(CredTarget);
        Restart();
    }

    public void Restart()
    {
        loop?.Cancel();
        loop = new CancellationTokenSource();
        var ct = loop.Token;
        _ = Task.Run(async () =>
        {
            await Task.Delay(1500, ct);
            while (!ct.IsCancellationRequested)
            {
                await PollAsync();
                await Task.Delay(TimeSpan.FromSeconds(Math.Max(30, Config.PollSeconds)), ct);
            }
        }, ct);
    }

    async Task PollAsync()
    {
        if (!IsConfigured)
        {
            Issues = Array.Empty<Issue>();
            LastError = null;
            Changed?.Invoke();
            return;
        }
        var (issues, error) = await SearchAsync(Config, secrets.Read(CredTarget));
        LastChecked = DateTime.Now;
        LastError = error;
        if (issues != null)
        {
            int fresh = issues.Count(i => !seen.Contains(i.Key));
            foreach (var i in issues) seen.Add(i.Key);
            Issues = issues;
            if (!firstPoll && fresh > 0) NewReviews?.Invoke(fresh);
            firstPoll = false;
        }
        Changed?.Invoke();
    }

    /// Runs the JQL search. Returns the issues, or null and a readable error.
    public static async Task<(List<Issue> Issues, string Error)> SearchAsync(Settings s, string token)
    {
        if (string.IsNullOrWhiteSpace(s.Site)) return (null, "Enter your Jira site first (e.g. your-team.atlassian.net)");
        var site = s.Site.Trim().Replace("https://", "").Replace("http://", "").TrimEnd('/');
        var jql = string.IsNullOrWhiteSpace(s.Jql) ? DefaultJql : s.Jql;
        var url = $"https://{site}/rest/api/3/search/jql?jql={Uri.EscapeDataString(jql)}" +
                  "&fields=summary,status,updated&maxResults=25";
        try
        {
            using var req = new HttpRequestMessage(HttpMethod.Get, url);
            req.Headers.Authorization = new AuthenticationHeaderValue("Basic",
                Convert.ToBase64String(Encoding.UTF8.GetBytes($"{s.Email.Trim()}:{token}")));
            req.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
            using var resp = await Http.SendAsync(req);
            var body = await resp.Content.ReadAsStringAsync();
            if (resp.StatusCode == System.Net.HttpStatusCode.Unauthorized)
                return (null, "Jira didn't accept the email/API token");
            if (!resp.IsSuccessStatusCode)
                return (null, JiraError(body) ?? $"Jira returned {(int)resp.StatusCode} {resp.ReasonPhrase}");

            using var doc = JsonDocument.Parse(body);
            var list = new List<Issue>();
            if (doc.RootElement.TryGetProperty("issues", out var arr))
                foreach (var it in arr.EnumerateArray())
                {
                    var key = it.GetProperty("key").GetString();
                    var f = it.GetProperty("fields");
                    string summary = f.TryGetProperty("summary", out var su) ? su.GetString() : "";
                    string status = f.TryGetProperty("status", out var st) && st.TryGetProperty("name", out var sn) ? sn.GetString() : "";
                    double updated = f.TryGetProperty("updated", out var up) && DateTimeOffset.TryParse(up.GetString(), out var dto)
                        ? dto.ToUnixTimeMilliseconds() / 1000.0 : 0;
                    list.Add(new Issue(key, summary, status, updated, $"https://{site}/browse/{key}"));
                }
            return (list, null);
        }
        catch (TaskCanceledException) { return (null, "Jira didn't answer in time"); }
        catch (HttpRequestException ex) { return (null, "Couldn't reach Jira: " + ex.Message); }
        catch (Exception ex) { return (null, "Jira search failed: " + ex.Message); }
    }

    static string JiraError(string body)
    {
        try
        {
            using var doc = JsonDocument.Parse(body);
            if (doc.RootElement.TryGetProperty("errorMessages", out var em) && em.GetArrayLength() > 0)
                return em[0].GetString();
        }
        catch { }
        return null;
    }
}

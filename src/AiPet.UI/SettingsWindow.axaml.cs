using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.VisualTree;

namespace AiPet;

/// What the settings window needs from the pet: its services, its switches, and the avatar it wears.
sealed class SettingsHost
{
    public sealed record Switch(string Label, string Hint, Func<bool> Get, Action<bool> Set);

    public IPlatform Platform;
    public JiraWatcher Jira;
    public GitHubWatcher GitHub;
    public string DefaultEmail;
    public List<Switch> Switches = new();
    public Action ResetPosition;
    public Func<Avatar> CurrentAvatar;
    public Action<Avatar> ApplyAvatar;
}

/// Settings, one page per topic: General (the pet's switches), Avatars, Jira and GitHub. Styled like the pet's
/// menus. Tokens are typed by the user and kept in the platform's secret store; they're never shown again.
public partial class SettingsWindow : Window
{
    readonly SettingsHost host;
    static readonly IBrush Ok = new SolidColorBrush(Color.Parse("#8FE3A8"));
    static readonly IBrush Bad = new SolidColorBrush(Color.Parse("#FF8A8A"));
    static readonly IBrush Muted = new SolidColorBrush(Color.Parse("#9A9AA2"));

    static readonly Dictionary<string, (string Title, string Hint)> Pages = new()
    {
        ["general"] = ("General", "How the pet behaves on your desktop."),
        ["avatars"] = ("Avatars", "Pick how the pet looks. Its colour also tints check marks and buttons."),
        ["jira"] = ("Jira", "A bubble for every issue waiting on your review, and a wave when a new one arrives."),
        ["github"] = ("GitHub", "Pull requests waiting on your review. A PR that mentions a Jira key (in its title, branch, description or commits) joins that Jira bubble, which then gets a button to open it."),
    };

    public SettingsWindow() => InitializeComponent();  // designer

    internal SettingsWindow(SettingsHost host, string page)
    {
        this.host = host;
        InitializeComponent();
        SetAccent(host.CurrentAvatar().Accent);

        foreach (var nav in new[] { NavGeneral, NavAvatars, NavJira, NavGitHub })
            nav.Click += (_, _) => ShowPage((string)nav.Tag);
        foreach (var handle in new Control[] { Sidebar, Header })
            handle.PointerPressed += (_, e) =>
            {
                if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed && (e.Source as Visual)?.FindAncestorOfType<Button>(true) == null)
                    BeginMoveDrag(e);
            };
        KeyDown += (_, e) => { if (e.Key == Key.Escape) Close(); };
        CloseBtn.Click += (_, _) => Close();

        BuildGeneral();
        BuildAvatars();
        LoadJira();
        LoadGitHub();
        ShowPage(page);
    }

    void SetAccent(uint argb)
    {
        var c = Color.FromUInt32(argb);
        Resources["Accent"] = new SolidColorBrush(c);
        Resources["SystemAccentColor"] = c;
    }

    internal void ShowPage(string page)
    {
        if (!Pages.ContainsKey(page)) page = "general";
        GeneralPage.IsVisible = page == "general";
        AvatarsPage.IsVisible = page == "avatars";
        JiraPage.IsVisible = page == "jira";
        GitHubPage.IsVisible = page == "github";
        foreach (var nav in new[] { NavGeneral, NavAvatars, NavJira, NavGitHub })
            nav.Classes.Set("selected", (string)nav.Tag == page);
        (PageTitle.Text, PageHint.Text) = Pages[page];
    }

    // ------------------------------------------------------------------ General
    void BuildGeneral()
    {
        ToggleRows.Children.Clear();
        for (int i = 0; i < host.Switches.Count; i++)
        {
            var sw = host.Switches[i];
            var toggle = new ToggleSwitch { IsChecked = sw.Get(), VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(12, 0, -10, 0) };
            toggle.IsCheckedChanged += (_, _) => sw.Set(toggle.IsChecked == true);
            var text = new StackPanel
            {
                VerticalAlignment = VerticalAlignment.Center,
                Children =
                {
                    new TextBlock { Text = sw.Label, FontWeight = FontWeight.SemiBold },
                    new TextBlock { Text = sw.Hint, Classes = { "hint" } },
                },
            };
            var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto"), Margin = new Thickness(0, i == 0 ? 0 : 8, 0, 0) };
            Grid.SetColumn(toggle, 1);
            row.Children.Add(text);
            row.Children.Add(toggle);
            if (i > 0) ToggleRows.Children.Add(new Rectangle { Height = 1, Fill = new SolidColorBrush(Color.Parse("#26FFFFFF")), Margin = new Thickness(0, 8, 0, 0) });
            ToggleRows.Children.Add(row);
        }
        ResetPositionBtn.Click += (_, _) => host.ResetPosition();
        DataFolderText.Text = Paths.DataDir;
        OpenDataBtn.Click += (_, _) => host.Platform.OpenFolder(Paths.DataDir);
    }

    // ------------------------------------------------------------------ Avatars
    void BuildAvatars()
    {
        ReloadAvatarsBtn.Click += (_, _) => FillAvatars();
        OpenAvatarsBtn.Click += (_, _) => { Directory.CreateDirectory(Avatar.CustomDir); host.Platform.OpenFolder(Avatar.CustomDir); };
        FillAvatars();
    }

    void FillAvatars()
    {
        AvatarGrid.Children.Clear();
        var current = host.CurrentAvatar()?.Name;
        foreach (var a in Avatar.All())
        {
            var (body, glow) = Render(a);
            var glowImg = new Image
            {
                Source = glow, Stretch = Stretch.Fill, IsHitTestVisible = false,
                Effect = new DropShadowDirectionEffect { ShadowDepth = 0, BlurRadius = 8, Color = Color.FromUInt32(a.Glow), Opacity = 0.9 },
            };
            RenderOptions.SetBitmapInterpolationMode(glowImg, BitmapInterpolationMode.None);
            var bodyImg = new Image { Source = body, Stretch = Stretch.Fill };
            RenderOptions.SetBitmapInterpolationMode(bodyImg, BitmapInterpolationMode.None);
            var sprite = new Panel { Width = 104, Height = 96, HorizontalAlignment = HorizontalAlignment.Center, Children = { bodyImg, glowImg } };
            var name = new StackPanel
            {
                Orientation = Orientation.Horizontal, Spacing = 7, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(0, 8, 0, 0),
                Children =
                {
                    new Ellipse { Width = 9, Height = 9, Fill = new SolidColorBrush(Color.FromUInt32(a.Accent)), VerticalAlignment = VerticalAlignment.Center },
                    new TextBlock { Text = a.Name, FontWeight = FontWeight.SemiBold, VerticalAlignment = VerticalAlignment.Center },
                },
            };
            var tile = new Border { Classes = { "avatar" }, Child = new StackPanel { Children = { sprite, name } } };
            if (a.Name == current) tile.Classes.Add("selected");
            ToolTip.SetTip(tile, $"Wear {a.Name}");
            tile.PointerReleased += (_, e) =>
            {
                if (e.InitialPressMouseButton != MouseButton.Left) return;
                host.ApplyAvatar(a);
                SetAccent(a.Accent);
                foreach (var t in AvatarGrid.Children.OfType<Border>()) t.Classes.Set("selected", t == tile);
            };
            AvatarGrid.Children.Add(tile);
        }
    }

    /// A still of the avatar, drawn by the pet itself (body and glowing eyes are separate layers, as on the desktop).
    static (WriteableBitmap Body, WriteableBitmap Glow) Render(Avatar a)
    {
        var pet = new Pet();
        pet.SetAvatar(a);
        var input = new PetInput { State = "idle" };
        PetFrame f = null;
        for (int i = 1; i <= 3; i++) f = pet.Update(input, i * 0.05);
        return (ToBitmap(f.Body), ToBitmap(f.Glow));
    }

    static WriteableBitmap ToBitmap(uint[] px)
    {
        var bmp = new WriteableBitmap(new PixelSize(Pet.GW, Pet.GH), new Vector(96, 96), PixelFormat.Bgra8888, AlphaFormat.Premul);
        using var fb = bmp.Lock();
        var src = (int[])(object)px;
        for (int y = 0; y < Pet.GH; y++) Marshal.Copy(src, y * Pet.GW, fb.Address + y * fb.RowBytes, Pet.GW);
        return bmp;
    }

    // ------------------------------------------------------------------ Jira
    void LoadJira()
    {
        var jira = host.Jira;
        var c = jira.Config;
        EnabledBox.IsChecked = c.Enabled || !jira.HasToken;
        SiteBox.Text = c.Site;
        EmailBox.Text = string.IsNullOrEmpty(c.Email) ? host.DefaultEmail : c.Email;
        JqlBox.Text = c.Jql;
        TokenBox.Text = "";
        TokenHint.Text = jira.HasToken ? "A token is saved. Leave this empty to keep it." : "Create one at id.atlassian.com, then paste it here";
        JiraForgetBtn.IsVisible = jira.HasToken;
        if (jira.LastError != null) ShowJira(jira.LastError, Bad);

        CreateTokenBtn.Click += (_, _) => host.Platform.OpenUrl("https://id.atlassian.com/manage-profile/security/api-tokens");
        TestBtn.Click += async (_, _) => await TestJiraAsync();
        JiraSaveBtn.Click += (_, _) =>
        {
            jira.Save(JiraSettings(), TokenBox.Text);
            TokenBox.Text = "";
            TokenHint.Text = jira.HasToken ? "A token is saved. Leave this empty to keep it." : "Create one at id.atlassian.com, then paste it here";
            JiraForgetBtn.IsVisible = jira.HasToken;
            ShowJira("Saved. The pet checks Jira every couple of minutes.", Ok);
        };
        JiraForgetBtn.Click += (_, _) =>
        {
            jira.ForgetToken();
            JiraForgetBtn.IsVisible = false;
            TokenHint.Text = "Create one at id.atlassian.com, then paste it here";
            ShowJira("The saved token was removed. Jira reviews are off until you save a new one.", Muted);
        };
    }

    JiraWatcher.Settings JiraSettings() => new()
    {
        Enabled = EnabledBox.IsChecked == true,
        Site = (SiteBox.Text ?? "").Trim(), Email = (EmailBox.Text ?? "").Trim(),
        Jql = string.IsNullOrWhiteSpace(JqlBox.Text) ? JiraWatcher.DefaultJql : JqlBox.Text.Trim(),
        PollSeconds = host.Jira.Config.PollSeconds,
    };

    void ShowJira(string text, IBrush color)
    {
        JiraStatusText.Text = text;
        JiraStatusText.Foreground = color;
        JiraStatusBox.IsVisible = true;
    }

    async Task TestJiraAsync()
    {
        var token = !string.IsNullOrEmpty(TokenBox.Text) ? TokenBox.Text : host.Platform.Secrets.Read(JiraWatcher.CredTarget);
        if (string.IsNullOrEmpty(token)) { ShowJira("Paste an API token first.", Bad); return; }
        ShowJira("Searching…", Muted);
        var (issues, error) = await JiraWatcher.SearchAsync(JiraSettings(), token);
        if (error != null) ShowJira(error, Bad);
        else if (issues.Count == 0) ShowJira("Connected. No issues are waiting on your review right now.", Ok);
        else ShowJira($"Connected. {issues.Count} issue(s) waiting on your review, e.g. {issues[0].Key}: {issues[0].Summary}", Ok);
    }

    // ------------------------------------------------------------------ GitHub
    void LoadGitHub()
    {
        var github = host.GitHub;
        var g = github.Config;
        GitHubBox.IsChecked = g.Enabled;
        GitHubHostBox.Text = g.Host;
        GitHubOrgsBox.Text = string.Join(", ", g.Orgs);
        GitHubTokenBox.Text = "";
        UpdateGitHubStatus();

        GitHubTokenBox.TextChanged += (_, _) => { if (!string.IsNullOrWhiteSpace(GitHubTokenBox.Text)) GitHubBox.IsChecked = true; };
        CreateGitHubTokenBtn.Click += (_, _) =>
        {
            var h = string.IsNullOrWhiteSpace(GitHubHostBox.Text) ? "github.com" : GitHubHostBox.Text.Trim();
            host.Platform.OpenUrl($"https://{h}/settings/personal-access-tokens/new");
        };
        GitHubSaveBtn.Click += (_, _) =>
        {
            github.Save(new GitHubWatcher.Settings
            {
                Enabled = GitHubBox.IsChecked == true,
                Host = string.IsNullOrWhiteSpace(GitHubHostBox.Text) ? "github.com" : GitHubHostBox.Text.Trim(),
                Orgs = (GitHubOrgsBox.Text ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries),
                JiraProjects = github.Config.JiraProjects,
                PollSeconds = github.Config.PollSeconds,
            }, GitHubTokenBox.Text);
            GitHubTokenBox.Text = "";
            UpdateGitHubStatus("Saved. The pet checks GitHub every couple of minutes.");
        };
        GitHubTestBtn.Click += async (_, _) =>
        {
            GitHubStatus.Text = "Connecting…";
            GitHubStatus.Foreground = Muted;
            var (ok, message) = await github.TestAsync(GitHubHostBox.Text, GitHubTokenBox.Text);
            GitHubStatus.Text = message;
            GitHubStatus.Foreground = ok ? Ok : Bad;
        };
        GitHubForgetBtn.Click += (_, _) =>
        {
            github.ForgetToken();
            UpdateGitHubStatus("The saved token was removed. GitHub reviews are off until you save a new one.");
        };
    }

    void UpdateGitHubStatus(string message = null)
    {
        var github = host.GitHub;
        GitHubTokenHint.Text = github.HasSavedToken
            ? "A token is saved. Leave this empty to keep it."
            : "A fine-grained token with read access to pull requests and metadata.";
        GitHubForgetBtn.IsVisible = github.HasSavedToken;
        GitHubStatus.Text = message ?? (!github.Config.Enabled ? "GitHub reviews are off."
            : github.LastError ?? (github.HasSavedToken
                ? $"Connected with your token. {github.ReviewRequests.Count} pull request(s) waiting on your review."
                : "No token yet, so GitHub reviews are off. Paste a token above."));
        GitHubStatus.Foreground = message == null && github.Config.Enabled && github.LastError != null ? Bad : Muted;
    }
}

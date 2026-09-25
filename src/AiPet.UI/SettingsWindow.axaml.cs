using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
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
    /// Closes the pet, as Quit does but without installing an update itself ("Restart to update" has started that).
    public Action Quit;
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
        ["jira"] = ("Jira", "A bubble for every issue your search finds, and a wave when a new one turns up."),
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
        ImportBtn.Click += async (_, _) => await ImportAsync();
        BuildUpdates();
    }

    /// The version, and where its update stands (Updates): a check on demand, and "Restart to update" once one is in.
    void BuildUpdates()
    {
        VersionText.Text = "AiPet " + Updates.Version;
        UpdateBtn.Click += (_, _) =>
        {
            if (Updates.Status.Kind == UpdateStatus.Kinds.Ready) { if (Updates.Restart()) host.Quit(); }
            else _ = Task.Run(Updates.CheckAsync);
        };
        void Changed() => Dispatcher.UIThread.Post(ShowUpdate);
        Updates.Changed += Changed;
        Closed += (_, _) => Updates.Changed -= Changed;
        ShowUpdate();
    }

    void ShowUpdate()
    {
        var s = Updates.Status;
        UpdateText.Text = s.Text;
        UpdateText.Foreground = s.Kind switch { UpdateStatus.Kinds.Failed => Bad, UpdateStatus.Kinds.Ready => Ok, _ => Muted };
        UpdateBtn.IsVisible = s.Kind != UpdateStatus.Kinds.Off;
        UpdateBtn.IsEnabled = !s.Busy;
        UpdateBtn.Content = s.Kind == UpdateStatus.Kinds.Ready ? "Restart to update" : "Check for updates";
    }

    /// "Import defaults…": a team's preset file sets the Jira and GitHub settings, saved as their pages' Save does.
    /// Tokens and the email are never part of it (see Preset).
    async Task ImportAsync()
    {
        if (!StorageProvider.CanOpen) { ShowImport("This desktop has no file picker to choose a preset with.", Bad); return; }
        // the picker is the desktop's (a portal over D-Bus on Linux), and an error from it would take the pet down
        IReadOnlyList<IStorageFile> files;
        try
        {
            files = await StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
            {
                Title = "Import defaults", AllowMultiple = false,
                FileTypeFilter = new[] { new FilePickerFileType("AiPet preset") { Patterns = new[] { "*.json" } } },
            });
        }
        catch (Exception ex) { ShowImport("Couldn't open a file picker: " + ex.Message, Bad); return; }
        if (files == null || files.Count == 0) return;
        Preset preset;
        try
        {
            await using var stream = await files[0].OpenReadAsync();
            using var reader = new StreamReader(stream);
            preset = Preset.Parse(await reader.ReadToEndAsync());
        }
        catch (FormatException ex) { ShowImport("Nothing was imported. " + ex.Message, Bad); return; }
        catch (Exception ex) { ShowImport("Couldn't read the file: " + ex.Message, Bad); return; }
        if (!preset.HasJira && !preset.HasGitHub) { ShowImport("Nothing was imported: the file has no Jira or GitHub settings.", Muted); return; }
        // A file someone shared must not send your saved token to a site of its choosing: when it points Jira or GitHub
        // somewhere else, that token is removed before Save restarts the watcher, and you paste it again if it belongs there.
        var moved = new List<string>();
        try
        {
            if (preset.HasJira)
            {
                var jira = preset.ApplyTo(host.Jira.Config);
                if (host.Jira.HasToken && preset.MovesJira(host.Jira.Config)) { host.Jira.ForgetToken(); moved.Add("Jira at " + jira.Site); }
                host.Jira.Save(jira, null);
                FillJira();
            }
            if (preset.HasGitHub)
            {
                var github = preset.ApplyTo(host.GitHub.Config);
                if (host.GitHub.HasSavedToken && preset.MovesGitHub(host.GitHub.Config)) { host.GitHub.ForgetToken(); moved.Add("GitHub at " + github.Host); }
                host.GitHub.Save(github);
                FillGitHub();
            }
        }
        catch (Exception ex) { ShowImport("Couldn't save the settings: " + ex.Message, Bad); return; }
        if (moved.Count == 0) ShowImport($"Imported {preset.Describe()}. Your tokens and email stay as they were.", Ok);
        else ShowImport($"Imported {preset.Describe()}. It points {string.Join(" and ", moved)}, so " +
                        (moved.Count == 1 ? "the token you saved for the old address was removed: paste it" : "the tokens you saved for the old addresses were removed: paste them") +
                        " again if you trust the new one. Your email stays as it was.", Muted);
    }

    void ShowImport(string text, IBrush color)
    {
        ImportStatus.Text = text;
        ImportStatus.Foreground = color;
        ImportStatus.IsVisible = true;
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
            // a custom avatar the pet can't draw gets no tile, rather than taking Settings down
            WriteableBitmap body, glow;
            try { (body, glow) = Render(a); }
            catch (Exception ex) { Log.Write($"avatar {a.Name}: can't draw it: {ex.Message}"); continue; }
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
        FillJira();
        TokenBox.Text = "";
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
            ShowJira("The saved token was removed. Jira stays off until you save a new one.", Muted);
        };
    }

    /// The Jira page's fields from the saved settings (the token box is left alone).
    void FillJira()
    {
        var jira = host.Jira;
        var c = jira.Config;
        EnabledBox.IsChecked = c.Enabled || !jira.HasToken;
        SiteBox.Text = c.Site;
        EmailBox.Text = string.IsNullOrEmpty(c.Email) ? host.DefaultEmail : c.Email;
        JqlBox.Text = c.Jql;
        TokenHint.Text = jira.HasToken ? "A token is saved. Leave this empty to keep it." : "Create one at id.atlassian.com, then paste it here";
        JiraForgetBtn.IsVisible = jira.HasToken;
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
        else if (issues.Count == 0) ShowJira("Connected. The search finds no issues right now.", Ok);
        else ShowJira($"Connected. The search finds {issues.Count} issue(s), e.g. {issues[0].Key}: {issues[0].Summary}", Ok);
    }

    // ------------------------------------------------------------------ GitHub
    void LoadGitHub()
    {
        var github = host.GitHub;
        FillGitHub();
        GitHubTokenBox.Text = "";

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

    /// The GitHub page's fields from the saved settings (the token box is left alone).
    void FillGitHub()
    {
        var g = host.GitHub.Config;
        GitHubBox.IsChecked = g.Enabled;
        GitHubHostBox.Text = g.Host;
        GitHubOrgsBox.Text = string.Join(", ", g.Orgs ?? Array.Empty<string>());
        UpdateGitHubStatus();
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

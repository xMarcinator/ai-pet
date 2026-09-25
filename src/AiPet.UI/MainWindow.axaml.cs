using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Controls.Shapes;
using Avalonia.Input;
using Avalonia.Input.Platform;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.Threading;
using Avalonia.VisualTree;
using AiPet.Platforms;
using Path = Avalonia.Controls.Shapes.Path;

namespace AiPet;

public partial class MainWindow : Window
{
    // ------------------------------------------------------------------ services
    readonly IPlatform platform = PlatformFactory.Create();
    readonly Board board = new();
    readonly Pet pet = new();
    readonly PetInput input = new();
    readonly JiraWatcher jira;
    readonly GitHubWatcher github;
    readonly CodexWatcher codex = new();
    /// The chats as the hooks report them, through the server (while the pet isn't running they report nothing).
    readonly AgentSessions hooks = new();
    readonly HookServer server;
    IMediaPlayer Media => platform.Media;

    readonly Stopwatch clock = Stopwatch.StartNew();
    double Now => clock.Elapsed.TotalSeconds;

    // ------------------------------------------------------------------ config
    sealed class Config
    {
        public int? Left { get; set; }
        public int? Top { get; set; }
        public bool Pills { get; set; } = true;
        /// Only in files from before the toolbar under the pet was removed: whether it showed when the position was
        /// saved (see PlaceWindow). Never written again.
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public bool? Toolbar { get; set; }
        public bool OnTop { get; set; } = true;
        public string Avatar { get; set; } = "Sprout";
        public bool Music { get; set; } = true;
        /// Window height (DIPs) the saved position was taken with, so a taller window keeps the pet in place.
        public double WindowHeight { get; set; } = 420;
    }
    Config cfg = new();

    // ------------------------------------------------------------------ sprite bitmaps
    readonly WriteableBitmap bodyBmp, glowBmp, fxBmp;
    readonly ScaleTransform Squash = new(1, 1), ShadowScale = new(1, 1);
    readonly TranslateTransform Move = new(), ShadowMove = new();

    static WriteableBitmap NewLayer() =>
        new(new PixelSize(Pet.GW, Pet.GH), new Vector(96, 96), PixelFormat.Bgra8888, AlphaFormat.Premul);

    // ------------------------------------------------------------------ interaction state
    bool hover, dragging, moved, pillsVisible = true;
    PixelPoint dragStartScreen, dragStartPos, lastScreen;
    double leftWindowAt = double.MaxValue, lastRegionAt;
    readonly Dictionary<string, bool> expanded = new() { ["chats"] = false, ["reviews"] = false, ["music"] = false };

    public MainWindow()
    {
        InitializeComponent();
        full = new Size(Width, Height);
        if (Environment.GetEnvironmentVariable("AIPET_OPAQUE") == "1")
        {
            // debugging aid for displays without transparent windows (e.g. WSLg): draw on a solid background
            TransparencyLevelHint = new[] { WindowTransparencyLevel.None };
            Background = new SolidColorBrush(Color.Parse("#202024"));
        }
        jira = new JiraWatcher(platform.Secrets);
        github = new GitHubWatcher(platform.Secrets);
        server = new HookServer(hooks);
        bodyBmp = NewLayer(); glowBmp = NewLayer(); fxBmp = NewLayer();
        BodyImg.Source = bodyBmp; GlowImg.Source = glowBmp; FxImg.Source = fxBmp;
        Sprite.RenderTransform = new TransformGroup { Children = { Squash, Move } };
        Shadow.RenderTransform = new TransformGroup { Children = { ShadowScale, ShadowMove } };

        Directory.CreateDirectory(Paths.DataDir);
        try { cfg = JsonSerializer.Deserialize<Config>(File.ReadAllText(Paths.Config)) ?? new Config(); } catch { }
        var avatars = Avatar.All();
        ApplyAvatar(avatars.FirstOrDefault(a => a.Name == cfg.Avatar) ?? avatars[0]);
        pillsVisible = cfg.Pills;
        Topmost = cfg.OnTop;

        Opened += (_, _) =>
        {
            PlaceWindow();
            placed = true;
            if (TryGetPlatformHandle()?.Handle is IntPtr h) platform.SetupPetWindow(h);
            UpdateInputRegion(force: true);
        };
        // a fitted window's content moved in it: its shape follows once the layout is done
        Resized += (_, _) => Dispatcher.UIThread.Post(() => UpdateInputRegion(force: true), DispatcherPriority.Render);

        // pet: drag, boop, hover
        Sprite.PointerPressed += Sprite_Pressed;
        Sprite.PointerMoved += Sprite_Moved;
        Sprite.PointerReleased += Sprite_Released;
        Sprite.PointerEntered += (_, _) =>
        {
            if (!hover && !dragging && Now > input.WaveUntil + 3) input.WaveUntil = Now + 1.4;
            hover = true;
        };
        Sprite.PointerExited += (_, _) => { if (!dragging) { hover = false; input.Mouse = null; } };
        Root.PointerEntered += (_, _) => leftWindowAt = double.MaxValue;
        Root.PointerExited += (_, _) => leftWindowAt = Now;
        Root.ContextMenu = BuildMenu();

        // watchers
        void Later() => Dispatcher.UIThread.Post(Refresh);
        jira.Changed += () => { github.JiraChanged(); Later(); };
        jira.NewReviews += _ => Dispatcher.UIThread.Post(() => input.AlertUntil = Now + 6);
        jira.Restart();
        github.JiraKeys = () => jira.Issues.Select(i => i.Key).ToList();
        github.Changed += Later;
        github.NewReviews += _ => Dispatcher.UIThread.Post(() => input.AlertUntil = Now + 6);
        github.Restart();
        codex.Start();
        // the app's single-instance check has passed (Program), so this is the only pet taking the name
        server.Changed += Later;
        server.Start();
        if (Media != null) Media.Changed += Later;

        Timer(250, Refresh);
        Timer(2000, KeepOnTop);
        Timer(1000, () => { if (cfg.Music) Media?.Poll(); });
        KeepOnTop();
        Refresh();
        RequestAnimationFrame(OnFrame);
        Closed += (_, _) => { SaveConfig(); server.Stop(); };
    }

    static void Timer(int ms, Action tick)
    {
        var t = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(ms) };
        t.Tick += (_, _) => tick();
        t.Start();
    }

    // ------------------------------------------------------------------ window placement & config
    PixelRect WorkArea => (Screens.ScreenFromWindow(this) ?? Screens.Primary)?.WorkingArea ?? new PixelRect(0, 0, 1920, 1080);
    double Scale => RenderScaling;

    /// Height (DIPs, margins included) of the toolbar row that used to sit under the pet.
    const double OldToolbarRow = 42;

    void PlaceWindow()
    {
        var wa = WorkArea;
        int w = (int)(Width * Scale), h = (int)(Height * Scale);
        int x = cfg.Left ?? wa.Right - w - 24;
        // The pet sits at the bottom of the window, so a window taller than the saved one moves up by the difference.
        // A position saved with the old toolbar showing had the pet a toolbar row higher, as if the window were shorter.
        double savedHeight = cfg.WindowHeight - (cfg.Toolbar == true ? OldToolbarRow : 0);
        int y = cfg.Top.HasValue ? cfg.Top.Value - (int)((Height - savedHeight) * Scale) : wa.Bottom - h;
        var all = Screens.All.Select(s => s.Bounds).ToList();
        bool visible = all.Any(b => x + w - 80 > b.X && x + 80 < b.Right && y + h - 120 > b.Y && y + h - 60 < b.Bottom);
        if (!visible) { x = wa.Right - w - 24; y = wa.Bottom - h; }
        Position = new PixelPoint(x, y);
    }

    void ResetPosition()
    {
        var wa = WorkArea;
        var whole = new PixelPoint(wa.Right - FullPixels.Width - 24, wa.Bottom - FullPixels.Height);
        Position = whole + FittedOffset();
        SaveConfig();
    }

    /// The whole window's size in pixels.
    PixelSize FullPixels => new((int)Math.Round(full.Width * Scale), (int)Math.Round(full.Height * Scale));

    /// How far the corner of a window fitted to this size (pixels) is inward of where the whole window's would be:
    /// fitted, it keeps the pet's bottom-centre where the whole window has it (see FitWindow). Every move goes by this
    /// one integer formula, so no rounding pixel ever moves the pet.
    PixelPoint OffsetFor(int width, int height) => new((FullPixels.Width - width) / 2, FullPixels.Height - height);

    /// The same for the window as it is now; zero while it has its full size.
    PixelPoint FittedOffset() => OffsetFor((int)Math.Round(ClientSize.Width * Scale), (int)Math.Round(ClientSize.Height * Scale));

    void SaveConfig()
    {
        // saved as the whole window, so the next start places it the same way whatever size it has now
        var whole = Position - FittedOffset();
        cfg.Left = whole.X; cfg.Top = whole.Y; cfg.WindowHeight = full.Height; cfg.Toolbar = null;
        cfg.Pills = pillsVisible; cfg.OnTop = Topmost;
        try { File.WriteAllText(Paths.Config, JsonSerializer.Serialize(cfg)); } catch { }
    }

    void KeepOnTop()
    {
        if (Topmost && TryGetPlatformHandle()?.Handle is IntPtr h) platform.KeepOnTop(h);
    }

    // ------------------------------------------------------------------ avatar / theme
    static Color ToColor(uint argb) => Color.FromUInt32(argb);

    void ApplyAvatar(Avatar a)
    {
        pet.SetAvatar(a);
        if (GlowImg.Effect is DropShadowDirectionEffect glow) glow.Color = ToColor(a.Glow);
        var accent = ToColor(a.Accent);
        Application.Current.Resources["Accent"] = new SolidColorBrush(accent);
        Application.Current.Resources["SystemAccentColor"] = accent;
        Resources["Accent"] = new SolidColorBrush(accent);
        cfg.Avatar = a.Name;
    }

    // ------------------------------------------------------------------ menu
    static Path Check() => new() { Classes = { "check" } };

    ContextMenu BuildMenu()
    {
        // Above the pet, where the window has room for it: on X11 menus are drawn inside their window (see Program), so
        // the window's edge cuts off what reaches past it. No submenu for the same reason (it would open sideways, out
        // of the window): the avatars are in Settings.
        var menu = new ContextMenu { PlacementTarget = Sprite, Placement = PlacementMode.Top };
        // a window fitted to less (or about to be: a change on its way) has no room for it yet: it grows first, then
        // FitWindow opens the menu
        menu.Opening += (_, e) =>
        {
            if (!platform.FitsPetWindow) return;
            var (roomW, roomH) = Room();
            bool small = ClientSize.Width < roomW - 1 || ClientSize.Height < roomH - 1;
            bool shrinking = requestedAt >= 0 && (requested.Width < roomW * Scale - 1 || requested.Height < roomH * Scale - 1);
            if (!small && !shrinking) return;
            e.Cancel = true;
            menuWanted = true;
        };
        // where the window takes the mouse (and, for compositors that honour it, what it shows) includes the menu
        menu.Opened += (_, _) => Dispatcher.UIThread.Post(() => UpdateInputRegion(), DispatcherPriority.Background);
        MenuItem Toggle(string text, Func<bool> get, Action<bool> set)
        {
            var mi = new MenuItem { Header = text };
            void Show() => mi.Icon = get() ? Check() : null;
            Show();
            mi.Click += (_, _) => { set(!get()); Show(); SaveConfig(); };
            menu.Opening += (_, _) => Show();
            return mi;
        }
        // the quick ones; everything else is in Settings
        menu.Items.Add(Toggle("Show chats", () => pillsVisible, v => { pillsVisible = v; Refresh(); }));
        menu.Items.Add(Toggle("Always on top", () => Topmost, v => Topmost = v));

        var avatarsItem = new MenuItem { Header = "Avatars…" };
        avatarsItem.Click += (_, _) => OpenSettings("avatars");
        menu.Items.Add(avatarsItem);

        var settingsItem = new MenuItem { Header = "Settings…" };
        settingsItem.Click += (_, _) => OpenSettings();
        menu.Items.Add(settingsItem);
        menu.Items.Add(new Separator());
        var quit = new MenuItem { Header = "Quit" };
        quit.Click += (_, _) => { Updates.InstallOnQuit(); Close(); };
        menu.Items.Add(quit);
        return menu;
    }

    SettingsWindow settings;

    /// The settings window, on a page: "general", "avatars", "jira" or "github". Only one is open at a time.
    void OpenSettings(string page = "general")
    {
        if (settings != null) { settings.ShowPage(page); settings.Activate(); return; }
        var host = new SettingsHost
        {
            Platform = platform, Jira = jira, GitHub = github, DefaultEmail = "",
            ResetPosition = ResetPosition,
            Quit = Close,
            CurrentAvatar = () => pet.Avatar,
            ApplyAvatar = a => { ApplyAvatar(a); SaveConfig(); },
            Switches =
            {
                new("Show chat bubbles", "The stacked bubbles above the pet: your chats, reviews and what's playing.",
                    () => pillsVisible, v => { pillsVisible = v; Refresh(); SaveConfig(); }),
                new("Always on top", "Keep the pet above other windows.", () => Topmost, v => { Topmost = v; SaveConfig(); }),
            },
        };
        if (Media != null)
            host.Switches.Add(new($"Listen along with {Media.Name}", "Show what's playing in a bubble, and the pet bops along.",
                () => cfg.Music, v => { cfg.Music = v; Refresh(); SaveConfig(); }));
        settings = new SettingsWindow(host, page);
        settings.Closed += (_, _) => { settings = null; Refresh(); };
        settings.Show();
    }

    // ------------------------------------------------------------------ state
    string lastState;

    void Refresh()
    {
        board.Refresh(hooks, jira, github, Media, cfg.Music, codex);
        if (board.State != lastState)
        {
            lastState = board.State;
            input.State = board.State;
            input.StateSince = Now;
        }
        input.Prop = board.Prop;
        SyncCards();
    }

    // ------------------------------------------------------------------ cards
    static readonly Color Amber = Color.Parse("#FFCF3F");

    sealed class Card
    {
        public string Id, Kind = "chat", Section = "chats", State, TicketUrl, PrUrl, Where;
        public Panel Root, Host;
        public Border Body;
        public Control Content;
        public TextBlock Title, Detail;
        public Ellipse Dot, Pulse;
        public StackPanel Actions;
        public Button Close, Stop, Open, Pr, Prev, PlayPause, Next;
        public Path PlayIcon;
        public ScaleTransform Scale = new(1, 1);
        public TranslateTransform Move = new(0, 14);
        public bool Removing, Hover, Pulsing, Highlight;
        public uint? AppColor;
        public double Y = 14, S = 1, O = 0, TY, TS = 1, TO = 1, BornAt, RemovedAt;
    }

    readonly Dictionary<string, Card> cardViews = new();

    Path MakeIcon(string data, bool filled = false, double thickness = 1.6) => new()
    {
        Classes = { "icon" }, Width = 16, Height = 16, StrokeThickness = thickness, Data = Geometry.Parse(data),
        Fill = filled ? (IBrush)Application.Current.Resources["IconBrush"] : null,
    };

    Button Round(string tip, Control content)
    {
        var b = new Button { Classes = { "round" }, Content = content };
        ToolTip.SetTip(b, tip);
        return b;
    }

    Card MakeCard(string id)
    {
        var c = new Card { Id = id, BornAt = Now };
        c.Title = new TextBlock { FontSize = 13.5, FontWeight = FontWeight.SemiBold, Foreground = Brushes.White, TextTrimming = TextTrimming.CharacterEllipsis, MaxWidth = 210 };
        c.Detail = new TextBlock { FontSize = 12, Foreground = new SolidColorBrush(Color.Parse("#A8A8AE")), TextTrimming = TextTrimming.CharacterEllipsis, MaxWidth = 210, Margin = new Thickness(0, 1, 0, 0) };
        var texts = new StackPanel { VerticalAlignment = VerticalAlignment.Center, Children = { c.Title, c.Detail } };

        c.Dot = new Ellipse { Width = 8, Height = 8 };
        c.Pulse = new Ellipse { Width = 8, Height = 8, Opacity = 0, RenderTransform = new ScaleTransform(1, 1) };
        var dotBox = new Panel { Width = 18, Height = 18, Margin = new Thickness(0, 0, 8, 0), VerticalAlignment = VerticalAlignment.Center, Children = { c.Pulse, c.Dot } };

        c.Open = Round("Open in Jira", MakeIcon("M9,3 H13 V7 M13,3 L7.5,8.5 M11,9.5 V12.5 A0.5,0.5 0 0 1 10.5,13 H3.5 A0.5,0.5 0 0 1 3,12.5 V5.5 A0.5,0.5 0 0 1 3.5,5 H6.5"));
        c.Open.Click += (_, _) => { if (c.TicketUrl != null) platform.OpenUrl(c.TicketUrl); };
        c.Pr = Round("Open pull request", MakeIcon("M4.5,2.5 A1.5,1.5 0 1 1 4.49,2.5 M4.5,5.5 V13.5 M11.5,10.5 A1.5,1.5 0 1 1 11.49,10.5 M11.5,10.5 V6.5 A2,2 0 0 0 9.5,4.5 H7 M8.5,3 L7,4.5 L8.5,6", thickness: 1.5));
        c.Pr.Click += (_, _) => { if (c.PrUrl != null) platform.OpenUrl(c.PrUrl); };
        c.Prev = Round("Previous", MakeIcon("M4,3.5 V12.5 M12.5,3.5 L6.5,8 L12.5,12.5 Z", filled: true));
        c.PlayIcon = MakeIcon("M5.5,3.5 V12.5 M10.5,3.5 V12.5");
        c.PlayPause = Round("Play / pause", c.PlayIcon);
        c.Next = Round("Next", MakeIcon("M12,3.5 V12.5 M3.5,3.5 L9.5,8 L3.5,12.5 Z", filled: true));
        c.Prev.Click += (_, _) => Media?.Previous();
        c.PlayPause.Click += (_, _) => Media?.PlayPause();
        c.Next.Click += (_, _) => Media?.Next();
        c.Stop = Round("Stop", new Rectangle { Width = 10, Height = 10, RadiusX = 2, RadiusY = 2, Fill = (IBrush)Application.Current.Resources["IconBrush"] });
        c.Stop.Click += (_, _) =>
        {
            // Escape stops whichever chat the app has open, so it's only pressed when that must be this one. Otherwise
            // the app just comes forward, on the chat itself when it has a link, for you to stop it there.
            var s = board.Find(c.Id);
            if (s == null) return;
            if (!OnlyDesktopChat(s)) OpenItem(s.Id);
            else if (platform.FocusAgent(s.Agent, s.Link)) _ = platform.SendEscape();
        };
        c.Actions = new StackPanel
        {
            Orientation = Orientation.Horizontal, Margin = new Thickness(12, 0, 0, 0), VerticalAlignment = VerticalAlignment.Center, IsVisible = false,
            Children = { c.Open, c.Pr, c.Prev, c.PlayPause, c.Next, c.Stop },
        };

        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto") };
        Grid.SetColumn(texts, 1);
        Grid.SetColumn(c.Actions, 2);
        row.Children.Add(dotBox); row.Children.Add(texts); row.Children.Add(c.Actions);
        c.Content = row;

        c.Body = new Border
        {
            CornerRadius = new CornerRadius(25), Height = 50, MinWidth = 240, MaxWidth = 350, Padding = new Thickness(14, 0, 9, 0),
            Child = row, Cursor = new Cursor(StandardCursorType.Hand),
        };
        StyleBody(c, false);

        c.Close = new Button
        {
            Classes = { "close" }, HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(-7, -7, 0, 0), IsVisible = false,
            Content = new Path { Stroke = Brushes.White, StrokeThickness = 1.6, StrokeLineCap = PenLineCap.Round, Data = Geometry.Parse("M0,0 L7,7 M7,0 L0,7"), Width = 7, Height = 7 },
        };
        ToolTip.SetTip(c.Close, "Dismiss");
        c.Close.Click += (_, _) => { board.Dismiss(c.Id); Refresh(); };

        c.Root = new Panel
        {
            HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Bottom,
            RenderTransformOrigin = new RelativePoint(0.5, 1, RelativeUnit.Relative), Opacity = 0,
            RenderTransform = new TransformGroup { Children = { c.Scale, c.Move } },
            Children = { c.Body, c.Close },
        };
        c.Root.PointerEntered += (_, _) => { c.Hover = true; UpdateChrome(c); };
        c.Root.PointerExited += (_, _) => { c.Hover = false; UpdateChrome(c); };
        c.Body.PointerReleased += (_, e) =>
        {
            if (e.InitialPressMouseButton != MouseButton.Left) return;
            if ((e.Source as Visual)?.FindAncestorOfType<Button>(includeSelf: true) != null) return;
            // a collapsed stack spreads out on the first click; after that a click opens the item. Only one stack is
            // spread out at a time: two are taller than the row above the pet (see LayoutCards).
            if (!expanded[c.Section] && board.Cards.Count(x => x.Section == c.Section) > 1)
            {
                foreach (var sec in expanded.Keys.ToList()) expanded[sec] = sec == c.Section;
                LayoutCards();
            }
            else OpenItem(c.Id);
        };
        return c;
    }

    /// Chat bubbles have a border in the colour of the app they run in; one that needs you also glows amber
    /// (animated in OnFrame), so you can see both at once.
    static void StyleBody(Card c, bool highlight)
    {
        c.Highlight = highlight;
        var app = c.AppColor is { } argb ? Color.FromUInt32(argb) : (Color?)null;
        c.Body.BorderThickness = new Thickness(app != null ? 1.5 : highlight ? 1.5 : 1);
        c.Body.BorderBrush = new SolidColorBrush(app is { } a ? Color.FromArgb(0xB8, a.R, a.G, a.B)
            : highlight ? Color.FromArgb(0xE0, Amber.R, Amber.G, Amber.B) : Color.Parse("#26FFFFFF"));
        c.Body.Background = new SolidColorBrush(highlight ? Color.Parse("#F72E291E") : Color.Parse("#F5252528"));
        c.Title.Foreground = highlight ? new SolidColorBrush(Color.Parse("#FFE9A8")) : Brushes.White;
        c.Body.BoxShadow = BoxShadows.Parse("0 4 18 0 #61000000");
    }

    /// Keys sent to an agent's app land in whichever chat it has open. That can only be this chat when it runs in
    /// the desktop app and is the only chat of that agent the pet knows about. The pet only knows the chats it heard
    /// from since it started, so for its first 15 minutes (as long as a busy chat can stay quiet before Board shows
    /// it idle) another chat may be busy unseen.
    bool OnlyDesktopChat(Session s) =>
        Now > 900 && s.Where == "desktop" && board.All.Count(x => x.Kind == "chat" && x.Agent == s.Agent) == 1;

    void OpenItem(string id)
    {
        var s = board.Find(id);
        if (s?.Kind is "jira-error" or "github-error") { OpenSettings(s.Kind == "jira-error" ? "jira" : "github"); return; }
        if (s?.Kind == "music") { Media?.Focus(); return; }
        if (s?.Kind is "jira" or "github" && s.Url != null) { platform.OpenUrl(s.Url); return; }
        // a deep link opens the chat itself (and brings its app forward); otherwise just bring the app forward
        if (s?.Link != null) platform.OpenUrl(s.Link);
        else platform.FocusAgent(s?.Agent ?? "claude");
    }

    bool IsFront(Card c) => c.Id == "_ghost" || board.Cards.FirstOrDefault(x => x.Section == c.Section)?.Id == c.Id;

    void UpdateChrome(Card c)
    {
        bool interactive = c.Hover && (expanded[c.Section] || IsFront(c)) && !c.Removing;
        c.Actions.IsVisible = interactive;
        c.Close.IsVisible = interactive;
        // the pet can only stop a chat through its agent's desktop app, so terminal, VS Code, SDK and exec chats get none
        c.Stop.IsVisible = c.Kind == "chat" && c.State is ("working" or "thinking") && c.Where == "desktop";
        if (c.Stop.IsVisible && board.Find(c.Id) is { } s)
            ToolTip.SetTip(c.Stop, OnlyDesktopChat(s) ? "Stop" : $"Open {Board.AgentLabel(s.Agent)} to stop it");
        c.Open.IsVisible = c.TicketUrl != null;
        c.Pr.IsVisible = c.PrUrl != null;
        c.Prev.IsVisible = c.PlayPause.IsVisible = c.Next.IsVisible = c.Kind == "music";
        c.Title.MaxWidth = c.Detail.MaxWidth = interactive ? 190 : 250;
    }

    void SyncCards()
    {
        var show = pillsVisible ? board.Cards : new List<Session>();
        foreach (var sec in expanded.Keys.ToList())
        {
            if (show.Count(x => x.Section == sec) <= 1) expanded[sec] = false;
            if (expanded[sec] && Now - leftWindowAt > 2.5) expanded[sec] = false;  // restack after you move away
        }
        var ghost = show.Count == 0 && pillsVisible && hover && !dragging;
        var ids = show.Select(s => s.Id).ToList();
        if (ghost) ids.Add("_ghost");

        foreach (var c in cardViews.Values.Where(c => !ids.Contains(c.Id) && !c.Removing).ToList())
        {
            c.Removing = true; c.RemovedAt = Now; c.TO = 0; c.TY = c.Y + 10;
        }

        double t = Now;
        bool multiApp = show.Where(x => x.Kind == "chat").Select(x => Board.AppOf(x).Label).Distinct().Count() > 1;
        for (int i = 0; i < ids.Count; i++)
        {
            if (!cardViews.TryGetValue(ids[i], out var c) || c.Removing)
            {
                if (c != null) { c.Host?.Children.Remove(c.Root); cardViews.Remove(c.Id); }
                c = MakeCard(ids[i]);
                cardViews[c.Id] = c;
                c.Section = ids[i] == "_ghost" ? "chats" : show[i].Section;
                c.Host = Stacks.First(x => x.Section == c.Section).Panel;
                c.Host.Children.Add(c.Root);
            }
            string st, title, detail, tip;
            uint? appColor = null;
            if (ids[i] == "_ghost")
            {
                st = board.State == "sleep" ? "sleep" : "idle";
                title = "Claude Code";
                detail = board.State == "sleep" ? "Napping" : "All caught up";
                tip = title;
            }
            else
            {
                var s = show[i];
                st = s.Eff; title = s.Name; detail = s.Detail;
                c.Kind = s.Kind; c.TicketUrl = s.TicketUrl; c.PrUrl = s.PrUrl; c.Where = s.Where;
                if (s.Kind == "music")
                {
                    c.PlayIcon.Data = Geometry.Parse(s.Eff == "music" ? "M5.5,3.5 V12.5 M10.5,3.5 V12.5" : "M5,3 L13,8 L5,13 Z");
                    c.PlayIcon.Fill = s.Eff == "music" ? null : (IBrush)Application.Current.Resources["IconBrush"];
                }
                tip = title;
                if (s.Kind == "chat")
                {
                    var app = Board.AppOf(s);
                    appColor = app.Color;
                    tip = $"{title}\nIn the {app.Label}";
                    if (multiApp || app.Label is not ("Claude app" or "Claude")) detail = app.Label + " · " + detail;
                }
                if (st == "thinking") detail = detail.TrimEnd('.', '…') + new string('.', 1 + (int)(t * 2.5) % 3);
                bool lastInSection = !show.Skip(i + 1).Any(x => x.Section == c.Section);
                if (lastInSection && board.Extra[c.Section] > 0) detail += $"  ·  +{board.Extra[c.Section]} more";
            }
            c.Title.Text = title;
            c.Detail.Text = detail;
            ToolTip.SetTip(c.Root, tip);
            if (c.State != st || c.Dot.Fill == null || c.AppColor != appColor)
            {
                c.State = st;
                c.AppColor = appColor;
                var col = ToColor(c.Kind == "github" ? Board.GitHubPurple : Board.StatusColor.GetValueOrDefault(st, 0xFF888888));
                c.Dot.Fill = new SolidColorBrush(col);
                c.Pulse.Fill = new SolidColorBrush(col);
                c.Pulsing = st is "working" or "attention" or "thinking" or "music";
                if (!c.Pulsing) c.Pulse.Opacity = 0;
                StyleBody(c, st == "attention");
            }
            UpdateChrome(c);
        }
        LayoutCards();
    }

    /// Each section's panel, and the gap (DIPs) above it while it holds bubbles, so an empty stack takes no room.
    /// The stacks share the 452 DIPs above the pet (the window's 600, less Root's margins, the pet's row and Sections'
    /// margin). A bottom-aligned StackPanel taller than that spills down behind the pet, so only one stack is spread
    /// out at a time. The tallest case, with Board.MaxCards' 4 chats, 4 reviews and music: one of them spread out
    /// (4 reviews 224, or 4 chats 14 + 224), the other collapsed (68, or 14 + 68), music 64 and the reviews header
    /// about 31: 401 DIPs. Raise a cap or add a stack and this needs checking again.
    (string Section, Panel Panel, double Gap)[] Stacks => new[] { ("reviews", Reviews, 0.0), ("chats", Pills, 14.0), ("music", Music, 14.0) };

    void LayoutCards()
    {
        var order = board.Cards.Select(s => s.Id).Append("_ghost").ToList();
        foreach (var (sec, panel, gap) in Stacks)
        {
            var active = cardViews.Values.Where(c => !c.Removing && c.Section == sec).OrderBy(c => order.IndexOf(c.Id)).ToList();
            bool open = expanded[sec];
            for (int i = 0; i < active.Count; i++)
            {
                var c = active[i];
                if (open) { c.TY = -i * 58; c.TS = 1; c.TO = 1; c.Content.Opacity = 1; }
                else { c.TY = -i * 9; c.TS = 1 - 0.06 * i; c.TO = i < 3 ? 1 - 0.22 * i : 0; c.Content.Opacity = i == 0 ? 1 : 0; }
                c.Root.ZIndex = 100 - i;
                c.Root.IsHitTestVisible = open || i == 0;
                UpdateChrome(c);
            }
            int n = active.Count;
            panelTarget[panel] = n == 0 ? 0 : gap + (open ? 50 + (n - 1) * 58 : 50 + 9 * (Math.Min(n, 3) - 1));
        }
        // Jira's search may find your own issues rather than reviews, so the count doesn't call them reviews
        int waiting = board.Cards.Count(x => x.Kind is "jira" or "github") + board.Extra["reviews"];
        ReviewsHeader.IsVisible = pillsVisible && cardViews.Values.Any(c => !c.Removing && c.Section == "reviews");
        ReviewsHeaderText.Text = waiting == 0 ? "Jira and GitHub" : $"{waiting} waiting on you";
    }

    readonly Dictionary<Panel, double> panelTarget = new();

    // ------------------------------------------------------------------ dragging the pet
    void Sprite_Pressed(object sender, PointerPressedEventArgs e)
    {
        if (!e.GetCurrentPoint(Sprite).Properties.IsLeftButtonPressed) return;
        dragStartScreen = lastScreen = this.PointToScreen(e.GetPosition(this));
        // where the whole window is, which a fitted window changing size (a change already on its way) doesn't move
        dragStartPos = Position - FittedOffset();
        dragging = true; moved = false;
        e.Pointer.Capture(Sprite);
        e.Handled = true;
    }

    void Sprite_Moved(object sender, PointerEventArgs e)
    {
        var p = e.GetPosition(Sprite);
        input.Mouse = (p.X, p.Y);
        if (!dragging) return;
        var cur = this.PointToScreen(e.GetPosition(this));
        int dx = cur.X - dragStartScreen.X, dy = cur.Y - dragStartScreen.Y;
        if (!moved && Math.Abs(dx) + Math.Abs(dy) > 4) moved = true;
        if (!moved) return;
        Position = new PixelPoint(dragStartPos.X + dx, dragStartPos.Y + dy) + FittedOffset();
        int ddx = cur.X - lastScreen.X;
        if (Math.Abs(ddx) >= 2) { input.Facing = ddx > 0 ? 1 : -1; input.LastMove = Now; }
        else if (Math.Abs(cur.Y - lastScreen.Y) > 2) input.LastMove = Now;
        lastScreen = cur;
    }

    void Sprite_Released(object sender, PointerReleasedEventArgs e)
    {
        if (!dragging) return;
        e.Pointer.Capture(null);
        if (!moved) input.PokeUntil = Now + 1.2;
        else { input.LandUntil = Now + 0.45; SaveConfig(); }
        dragging = moved = false;
        if (!Sprite.IsPointerOver) { hover = false; input.Mouse = null; }
    }

    // ------------------------------------------------------------------ frame loop
    double lastT;

    void OnFrame(TimeSpan _)
    {
        double t = Now, dt = Math.Clamp(t - lastT, 0.001, 0.05);
        lastT = t;

        // the pet
        input.Hover = hover;
        input.Dragging = dragging; input.Moved = moved;
        input.Music = cfg.Music && Media?.Playing == true;
        var f = pet.Update(input, t);
        if (f.PixelsChanged)
        {
            Blit(bodyBmp, f.Body, BodyImg);
            Blit(glowBmp, f.Glow, GlowImg);
            Blit(fxBmp, f.Fx, FxImg);
        }
        Squash.ScaleX = f.ScaleX; Squash.ScaleY = f.ScaleY;
        Move.X = f.X; Move.Y = f.Y;
        ShadowScale.ScaleX = ShadowScale.ScaleY = f.ShadowScale;
        ShadowMove.X = f.X;
        Shadow.Opacity = f.ShadowOpacity;

        // cards: ease towards their targets
        double k = 1 - Math.Exp(-dt * 13);
        foreach (var c in cardViews.Values.ToList())
        {
            c.Y += (c.TY - c.Y) * k; c.S += (c.TS - c.S) * k; c.O += (c.TO - c.O) * (1 - Math.Exp(-dt * 16));
            c.Move.Y = c.Y; c.Scale.ScaleX = c.Scale.ScaleY = c.S; c.Root.Opacity = c.O;
            if (c.Pulsing)
            {
                double ph = (t - c.BornAt) % 1.3 / 1.3, ease = 1 - Math.Pow(1 - ph, 3);
                ((ScaleTransform)c.Pulse.RenderTransform).ScaleX = ((ScaleTransform)c.Pulse.RenderTransform).ScaleY = 1 + 1.6 * ease;
                c.Pulse.Opacity = 0.55 * (1 - ph);
            }
            if (c.Highlight)
            {
                byte a = (byte)(0x40 + 0x90 * (0.5 + 0.5 * Math.Sin(t * 3.5)));
                c.Body.BoxShadow = new BoxShadows(new BoxShadow { Blur = 22, Color = Color.FromArgb(a, Amber.R, Amber.G, Amber.B) });
            }
            if (c.Removing && t - c.RemovedAt > 0.25) { c.Host?.Children.Remove(c.Root); cardViews.Remove(c.Id); }
        }
        foreach (var (panel, target) in panelTarget)
            panel.Height += (target - panel.Height) * k;

        FitWindow();
        // not while the window is changing size: the layout isn't the new one yet (Resized updates it once it is)
        if (t - lastRegionAt > 0.1 && requestedAt < 0) UpdateInputRegion();
        RequestAnimationFrame(OnFrame);
    }

    // ------------------------------------------------------------------ fitting the window (Linux)
    /// The window's size as the XAML gives it: room for every stack, the menu, and the pet.
    Size full;
    bool placed, menuWanted;
    double shrinkingSince = -1, requestedAt = -1;
    PixelSize requested;

    /// Keeps the window only as big as what it shows, where the platform asks for it (IPlatform.FitsPetWindow): a
    /// compositor that ignores the input region (see UpdateInputRegion) would otherwise let no click through anywhere
    /// in it. The pet's bottom-centre stays put, so the layout (bubbles stacked above the pet at the bottom) needs no
    /// change. It grows at once, for what is about to show, and shrinks once the stacks have settled.
    void FitWindow()
    {
        if (!platform.FitsPetWindow || !placed || dragging || TryGetPlatformHandle()?.Handle is not IntPtr handle) return;
        var (w, h) = FittedSize();
        int wantW = (int)Math.Ceiling(w * Scale), wantH = (int)Math.Ceiling(h * Scale);
        int curW = (int)Math.Round(ClientSize.Width * Scale), curH = (int)Math.Round(ClientSize.Height * Scale);
        // a change still on its way: its size and position aren't in yet (given up on after a moment)
        if (requestedAt >= 0 && Now - requestedAt < 0.5 && (Math.Abs(curW - requested.Width) > 1 || Math.Abs(curH - requested.Height) > 1)) return;
        requestedAt = -1;
        if (menuWanted && Math.Abs(curW - wantW) <= 1 && Math.Abs(curH - wantH) <= 1)
        {
            menuWanted = false;
            Root.ContextMenu?.Open(Root);
            return;
        }
        int newW = Math.Max(wantW, curW), newH = Math.Max(wantH, curH);
        if (wantW < curW - 1 || wantH < curH - 1)
        {
            // smaller only once nothing is still moving in the stacks, and then for a moment
            bool settled = panelTarget.All(p => Math.Abs(p.Key.Height - p.Value) < 0.5) && !cardViews.Values.Any(c => c.Removing);
            if (!settled) shrinkingSince = -1;
            else if (shrinkingSince < 0) shrinkingSince = Now;
            else if (Now - shrinkingSince > 0.4) { newW = wantW; newH = wantH; shrinkingSince = -1; }
        }
        else shrinkingSince = -1;
        if (Math.Abs(newW - curW) <= 1 && Math.Abs(newH - curH) <= 1) return;
        // the whole window takes the mouse until the input region for the new layout is in (see Resized): the old one
        // would leave the pet's new place out meanwhile
        platform.SetInputRegion(handle, new[] { (0, 0, Math.Max(curW, newW), Math.Max(curH, newH)) });
        lastRegion = null;
        requested = new PixelSize(newW, newH);
        requestedAt = Now;
        var corner = Position - OffsetFor(curW, curH) + OffsetFor(newW, newH);
        platform.MoveResize(handle, corner.X, corner.Y, newW, newH);
    }

    /// The size (DIPs) what's shown needs: the widest bubble with its close button and shadow, the stacks as tall as
    /// they are or are growing to, the reviews header and the pet's row, or everything while the menu is open or
    /// about to be. Never more than the screen's work area, which a window manager may hold a window to: a size it
    /// never grants would be asked for again and again, and the menu would wait for it.
    (double W, double H) FittedSize()
    {
        var (maxW, maxH) = Room();
        if (menuWanted || Root.ContextMenu?.IsOpen == true) return (maxW, maxH);
        double wide = 140;  // the pet's panel
        foreach (var c in cardViews.Values) wide = Math.Max(wide, c.Body.Bounds.Width + 2 * 20);
        if (ReviewsHeader.IsVisible) wide = Math.Max(wide, ReviewsHeader.Bounds.Width);
        double stacks = 0;
        foreach (var (_, panel, _) in Stacks) stacks += Math.Max(panel.Height, panelTarget.GetValueOrDefault(panel));
        if (ReviewsHeader.IsVisible) stacks += ReviewsHeader.Bounds.Height + ReviewsHeader.Margin.Bottom;
        // Root's margins and the pet's row; above it the stacks with the gap under them, or room for the pet to hop
        double h = Root.Margin.Top + (stacks > 0 ? stacks + Sections.Margin.Bottom : 20) + 124 + Root.Margin.Bottom;
        return (Math.Min(maxW, wide + Root.Margin.Left + Root.Margin.Right), Math.Min(maxH, h));
    }

    /// The whole window's size (DIPs), held to the screen's work area.
    (double W, double H) Room() => (Math.Min(full.Width, WorkArea.Width / Scale), Math.Min(full.Height, WorkArea.Height / Scale));

    static void Blit(WriteableBitmap bmp, uint[] px, Image img)
    {
        using (var fb = bmp.Lock())
        {
            var src = (int[])(object)px;
            for (int y = 0; y < Pet.GH; y++)
                Marshal.Copy(src, y * Pet.GW, fb.Address + y * fb.RowBytes, Pet.GW);
        }
        img.InvalidateVisual();
    }

    // ------------------------------------------------------------------ click-through
    string lastRegion;

    /// Tell the OS which parts of the window are the pet and its bubbles; the rest clicks through.
    void UpdateInputRegion(bool force = false)
    {
        lastRegionAt = Now;
        if (TryGetPlatformHandle()?.Handle is not IntPtr h) return;
        var rects = new List<(int, int, int, int)>();
        void Add(Control c, double pad)
        {
            if (c == null || !c.IsEffectivelyVisible || c.Bounds.Width <= 0) return;
            var tl = c.TranslatePoint(new Point(0, 0), this);
            var br = c.TranslatePoint(new Point(c.Bounds.Width, c.Bounds.Height), this);
            if (tl is not { } a || br is not { } b) return;
            double s = Scale;
            int x0 = (int)Math.Floor((Math.Min(a.X, b.X) - pad) * s), y0 = (int)Math.Floor((Math.Min(a.Y, b.Y) - pad) * s);
            int x1 = (int)Math.Ceiling((Math.Max(a.X, b.X) + pad) * s), y1 = (int)Math.Ceiling((Math.Max(a.Y, b.Y) + pad) * s);
            rects.Add((x0, y0, x1 - x0, y1 - y0));
        }
        Add(Sprite, 6);
        if (ReviewsHeader.IsVisible) Add(ReviewsHeader, 4);
        foreach (var c in cardViews.Values)
            if (c.Root.Opacity > 0.05) { Add(c.Body, 16); if (c.Close.IsVisible) Add(c.Close, 2); }
        // the menu and tooltips, when they're drawn in the window (overlay popups, on X11: see Program). The layer is
        // found from a control inside it; from the window itself it isn't.
        if (OverlayLayer.GetOverlayLayer(Root) is { } overlay)
            foreach (var popup in overlay.Children) Add(popup, 2);
        if (Root.ContextMenu is { IsOpen: true } menu) Add(menu, 2);
        var key = string.Join(";", rects);
        if (!force && key == lastRegion) return;
        lastRegion = key;
        platform.SetInputRegion(h, rects);
    }
}

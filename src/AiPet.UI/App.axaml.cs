using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;

namespace AiPet;

public partial class App : Application
{
    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            desktop.ShutdownMode = Avalonia.Controls.ShutdownMode.OnMainWindowClose;
            desktop.MainWindow = Environment.GetEnvironmentVariable("AIPET_PLAIN") == "1"
                // diagnostics: an ordinary decorated window, to check the display works at all
                ? new Avalonia.Controls.Window
                {
                    Title = "AiPet display test", Width = 360, Height = 160,
                    Background = Avalonia.Media.Brushes.DarkSlateBlue,
                    Content = new Avalonia.Controls.TextBlock { Text = "AiPet can draw here", FontSize = 22, Margin = new Avalonia.Thickness(20) },
                }
                : new MainWindow();
        }
        base.OnFrameworkInitializationCompleted();
    }
}

using System.Reflection;
using Velopack;
using Velopack.Locators;
using Velopack.Logging;
using Velopack.Sources;

namespace AiPet;

/// Velopack, which installs and updates the Windows app (release.yml packs it with packId AiPetApp into
/// %LOCALAPPDATA%\AiPetApp, channel win). Only that installed copy checks for updates, from the GitHub releases: a
/// few minutes after the pet starts and then every few hours. It downloads in the background, and the update is
/// installed when the user quits the pet, or at once with "Restart to update" in Settings. The portable zip and Linux
/// never check, and use no network here. A failure is only a state Settings shows.
static class Updates
{
    /// The app's version as released: release.yml sets it from the tag (-p:Version).
    public static string Version =>
        typeof(Updates).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion ?? "(unknown version)";

    static UpdateManager manager;
    static VelopackAsset ready;
    static int busy;

    public static UpdateStatus Status { get; private set; } = new(UpdateStatus.Kinds.Off);

    /// Raised on any thread when Status changes.
    public static event Action Changed;

    static void Set(UpdateStatus status)
    {
        Status = status;
        Changed?.Invoke();
    }

    /// Set when Update.exe started this pet after installing an update. Run clears the variable.
    static bool restarted;

    /// Velopack's startup, which Program.Main runs before anything else. The installer and uninstaller start the app
    /// with --veloapp-* arguments: Velopack runs their callback and exits. The uninstaller's removes the hooks
    /// install.ps1 registered with this install's aipet-hook.exe (HookCleanup). Velopack would also install an update
    /// downloaded earlier on any start, but this runs before the single-instance check: in a second pet, Update.exe
    /// would stop the running one before it saved anything. Start does it instead, in the one pet.
    public static VelopackApp App()
    {
        restarted = !string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("VELOPACK_RESTART"));
        var app = VelopackApp.Build().SetAutoApplyOnStartup(false);
        if (OperatingSystem.IsWindows())
            app.OnBeforeUninstallFastCallback(_ => HookCleanup.Run(Path.Combine(AppContext.BaseDirectory, "aipet-hook.exe")));
        if (!OperatingSystem.IsWindows() || !UpdateStatus.Installed(AppContext.BaseDirectory))
            app.SetLocator(new NotInstalled());
        return app;
    }

    /// Any copy Velopack didn't install: the portable zip, `dotnet AiPet.dll`, Linux (install.sh installs the app
    /// there). Velopack's own locator would log that it found no install on every start, to
    /// %LOCALAPPDATA%\velopack\velopack.log on Windows and /tmp/velopack.log on Linux (where it looks for an AppImage).
    sealed class NotInstalled : VelopackLocator
    {
        public override string AppId => null;
        public override string RootAppDir => null;
        public override string PackagesDir => null;
        public override string UpdateExePath => null;
        public override string AppContentDir => null;
        public override string Channel => null;
        public override SemanticVersion CurrentlyInstalledVersion => null;
        public override IProcessImpl Process { get; } = new DefaultProcessImpl(new NullVelopackLogger());
    }

    /// Starts the checks, if Velopack installed this copy; Program.Main calls it once it knows this is the only pet.
    /// False if an update downloaded earlier and not installed yet (the pet wasn't quit, the computer was shut down)
    /// is installed first: the pet then exits at once, and Update.exe installs the update and starts the pet again.
    /// Nothing here touches the network before the first check.
    public static bool Start()
    {
        if (!OperatingSystem.IsWindows()) return true;
        try
        {
            var m = new UpdateManager(new GithubSource(UpdateStatus.Repo, accessToken: null, prerelease: false),
                                      new UpdateOptions { ExplicitChannel = UpdateStatus.Channel });
            if (!m.IsInstalled) return true;
            manager = m;
            // not on the start that follows an update, which would go round again if that update failed
            if (!restarted && m.UpdatePendingRestart is { } pending)
            {
                Log.Write($"updates: installing {pending.Version}, downloaded earlier");
                m.WaitExitThenApplyUpdates(pending, silent: false, restart: true);
                return false;
            }
        }
        catch (Exception ex) { Log.Write("updates: " + ex.Message); }
        if (manager == null) return true;
        Set(new(UpdateStatus.Kinds.Idle));
        _ = Task.Run(async () =>
        {
            // one downloaded before and still not installed: the start that follows an update leaves it
            try
            {
                if (manager.UpdatePendingRestart is { } pending)
                {
                    ready = pending;
                    Set(new(UpdateStatus.Kinds.Ready, pending.Version.ToString()));
                }
            }
            catch (Exception ex) { Log.Write("updates: " + ex.Message); }
            await Task.Delay(UpdateStatus.FirstCheck);
            while (true)
            {
                await CheckAsync();
                await Task.Delay(UpdateStatus.Every);
            }
        });
        return true;
    }

    /// Checks for a newer release now and downloads it. One check at a time; call it off the UI thread.
    public static async Task CheckAsync()
    {
        if (manager == null || Interlocked.Exchange(ref busy, 1) == 1) return;
        string version = null;
        try
        {
            Set(new(UpdateStatus.Kinds.Checking));
            var info = await manager.CheckForUpdatesAsync().ConfigureAwait(false);
            if (info == null)
            {
                Set(Ready() ?? new(UpdateStatus.Kinds.UpToDate));
                return;
            }
            version = info.TargetFullRelease.Version.ToString();
            if (ready?.Version != info.TargetFullRelease.Version)
            {
                Set(new(UpdateStatus.Kinds.Downloading, version));
                await manager.DownloadUpdatesAsync(info, p => Set(new(UpdateStatus.Kinds.Downloading, version, p))).ConfigureAwait(false);
                ready = info.TargetFullRelease;
            }
            Set(Ready());
        }
        catch (Exception ex)
        {
            Log.Write($"updates: {(version == null ? "check" : "download of " + version)} failed: {ex.Message}");
            // an update downloaded earlier is still there to install
            Set(Ready() ?? new(UpdateStatus.Kinds.Failed, version, Error: ex.Message));
        }
        finally { Interlocked.Exchange(ref busy, 0); }
    }

    static UpdateStatus Ready() => ready == null ? null : new(UpdateStatus.Kinds.Ready, ready.Version.ToString());

    /// The user quits the pet: a downloaded update is installed once it has exited (Velopack's Update.exe waits for
    /// that), without a window, and the pet stays closed.
    public static void InstallOnQuit() => Apply(restart: false);

    /// "Restart to update": the same, and Update.exe starts the pet again. True if the caller should now quit the pet.
    public static bool Restart() => Apply(restart: true);

    static bool Apply(bool restart)
    {
        if (manager == null || ready == null) return false;
        try
        {
            manager.WaitExitThenApplyUpdates(ready, silent: !restart, restart: restart);
            return true;
        }
        catch (Exception ex)
        {
            Log.Write("updates: couldn't start the update: " + ex.Message);
            Set(new(UpdateStatus.Kinds.Failed, ready.Version.ToString(), Error: ex.Message));
            return false;
        }
    }
}

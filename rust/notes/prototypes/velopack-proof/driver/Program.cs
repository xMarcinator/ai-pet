// The Velopack proof's update driver (.github/workflows/velopack-proof.yml, results in rust/proofs/velopack.md).
//
// It updates an installed AiPet the way the installed .NET pet does (src/AiPet.UI/Updates.cs): the same Velopack
// library at the same version, its Windows locator on the installed AiPet.exe, the "win" channel, then
// CheckForUpdatesAsync, DownloadUpdatesAsync and WaitExitThenApplyUpdates. Only the source differs: a local feed folder
// in place of the GitHub releases. Update.exe then waits for this process to exit, applies the package and starts the
// new AiPet.exe, as after "Restart to update", but silently: a runner has nobody to answer a dialog.
//
//   velopack-proof-driver <install folder> <feed folder>
//
// It prints Velopack's log and what the check chose: the target, and the base and deltas if it took a delta.
// Exit code 0: the package was downloaded and handed to Update.exe. 1: anything else.
using Velopack;
using Velopack.Locators;
using Velopack.Logging;
using Velopack.Sources;

if (args.Length != 2 || !OperatingSystem.IsWindows())
{
    Console.Error.WriteLine("usage (on Windows): velopack-proof-driver <install folder> <feed folder>");
    return 1;
}
var root = Path.GetFullPath(args[0]);
var feed = new DirectoryInfo(Path.GetFullPath(args[1]));
var log = new ConsoleVelopackLogger();
try
{
    var locator = new WindowsVelopackLocator(new InstalledPet(Path.Combine(root, "current", "AiPet.exe"), log), log);
    // as Updates.Start makes it, with UpdateStatus.Channel
    var manager = new UpdateManager(new SimpleFileSource(feed), new UpdateOptions { ExplicitChannel = "win" }, locator);
    if (!manager.IsInstalled)
    {
        Console.Error.WriteLine($"No Velopack install in {root}");
        return 1;
    }
    Console.WriteLine($"installed: {manager.AppId} {manager.CurrentVersion}");

    var update = await manager.CheckForUpdatesAsync();
    if (update == null)
    {
        Console.Error.WriteLine($"No update in {feed.FullName}");
        return 1;
    }
    Console.WriteLine($"target: {Describe(update.TargetFullRelease)}");
    Console.WriteLine($"base: {(update.BaseRelease is { } b ? Describe(b) : "none")}");
    Console.WriteLine($"deltas: {(update.DeltasToTarget.Length == 0 ? "none" : string.Join(", ", update.DeltasToTarget.Select(Describe)))}");

    await manager.DownloadUpdatesAsync(update);
    manager.WaitExitThenApplyUpdates(update.TargetFullRelease, silent: true, restart: true);
    Console.WriteLine("handed over to Update.exe");
    return 0;
}
catch (Exception ex)
{
    Console.Error.WriteLine("failed: " + ex);
    return 1;
}

static string Describe(VelopackAsset a) => $"{a.FileName} ({a.Type} {a.Version}, {a.Size} bytes)";

/// The installed pet's process, as the locator sees it: the installed AiPet.exe's path, this process's id (the one
/// Update.exe waits for), and Velopack's own way of starting processes and exiting.
sealed class InstalledPet(string exe, IVelopackLogger log) : IProcessImpl
{
    readonly DefaultProcessImpl process = new(log);

    public string GetCurrentProcessPath() => exe;

    public uint GetCurrentProcessId() => process.GetCurrentProcessId();

    public void StartProcess(string exePath, IEnumerable<string> args, string workDir, bool showWindow) =>
        process.StartProcess(exePath, args, workDir, showWindow);

    public void Exit(int exitCode) => process.Exit(exitCode);
}

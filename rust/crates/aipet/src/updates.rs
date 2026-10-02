//! The app's own updates, through Velopack's Rust SDK (`src/AiPet.UI/Updates.cs`).
//!
//! Velopack installs and updates the Windows app (release.yml packs it with packId `AiPetApp` into
//! `%LOCALAPPDATA%\AiPetApp`, channel `win`). Only that installed copy checks for updates, from the GitHub releases: a
//! few minutes after the pet starts and then every few hours. It downloads in the background, and the update is
//! installed when the user quits the pet, or at once with "Restart to update" in Settings. The portable zip, a build
//! from source and Linux never check, and use no network here. A failure is only a state Settings shows.
//!
//! What Velopack's update manager does goes through [`Manager`], so the decisions here are tested with a fake one.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};
use std::thread;
use std::time::Duration;

use aipet_core::update_status::{self, Kind, UpdateStatus};
use velopack::{UpdateCheck, UpdateInfo, VelopackApp, VelopackAsset};

/// Set when the updater started this pet after installing an update (`VELOPACK_RESTART`). [`startup`] reads it
/// before Velopack's startup, which clears the variable.
static RESTARTED: AtomicBool = AtomicBool::new(false);

/// The installed copy's updates, once [`start`] found Velopack's install.
static UPDATER: OnceLock<Arc<Updater>> = OnceLock::new();

/// Velopack's startup (`Updates.App().Run()`), the first thing `main` does. The installer and uninstaller start the
/// app with `--veloapp-*` arguments: Velopack runs their callback and exits. The uninstaller's removes the hooks
/// install.ps1 registered with this install's `aipet-hook.exe` ([`aipet_core::cleanup::run`]). Velopack would also
/// install an update downloaded earlier on any start, but this runs before the single instance: in a second pet, the
/// updater would stop the running one before it saved anything. [`start`] does it instead, in the one pet.
///
/// It leaves no thread running: `main` sets the environment after it.
pub fn startup() {
    startup_with(std::env::args().skip(1).collect(), installed(), hook());
}

fn startup_with(args: Vec<String>, installed: bool, hook: PathBuf) {
    let restarted = std::env::var_os("VELOPACK_RESTART").is_some_and(|v| !v.to_string_lossy().trim().is_empty());
    RESTARTED.store(restarted, Ordering::Relaxed);
    app(args, installed, &hook).run();
}

/// Velopack's startup for these arguments. A copy Velopack didn't install gets a locator that finds no install, as
/// the C#'s `NotInstalled` is, so that Velopack never looks for one.
fn app(args: Vec<String>, installed: bool, hook: &Path) -> VelopackApp<'_> {
    let app = VelopackApp::build().set_args(args).set_auto_apply_on_startup(false);
    #[cfg(windows)]
    let app = app.on_before_uninstall_fast_callback(move |_| aipet_core::cleanup::run(hook));
    #[cfg(not(windows))]
    let _ = hook;
    if installed {
        app
    } else {
        app.set_locator(velopack::locator::VelopackLocatorConfig::default())
    }
}

/// The folder of the app's exe: `%LOCALAPPDATA%\AiPetApp\current` in the installed copy.
fn app_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_owned)
}

/// Whether this is the copy Velopack installed: only on Windows, as the C#'s `App()` and `Start` have it.
fn installed() -> bool {
    cfg!(windows) && app_dir().is_some_and(|dir| UpdateStatus::installed(&dir))
}

/// The hook next to the app, which install.ps1 registered: `current\aipet-hook.exe` (task 14's proof).
fn hook() -> PathBuf {
    app_dir().unwrap_or_default().join("aipet-hook.exe")
}

/// Starts the checks, once this is the only pet, if Velopack installed this copy (`Updates.Start`). False when an
/// update downloaded earlier and not installed yet (the pet wasn't quit, the computer was shut down) is installed
/// first: the pet then exits at once, and the updater installs the update and starts the pet again. Nothing here
/// touches the network before the first check.
pub fn start() -> bool {
    if !installed() {
        return true;
    }
    let source = velopack::sources::GithubSource::new(update_status::REPO, None, false);
    let options = velopack::UpdateOptions {
        ExplicitChannel: Some(update_status::CHANNEL.to_owned()),
        ..velopack::UpdateOptions::default()
    };
    let manager = match velopack::UpdateManager::new(source, Some(options), None) {
        Ok(manager) => manager,
        Err(e) => {
            aipet_core::log::write(&format!("updates: {e}"));
            return true;
        }
    };
    let updater = Arc::new(Updater::new(
        Box::new(Velopack(manager)),
        Box::new(aipet_core::log::write),
    ));
    if !updater.start(RESTARTED.load(Ordering::Relaxed)) {
        return false;
    }
    let _ = UPDATER.set(Arc::clone(&updater));
    let checks = thread::Builder::new().name("updates".into()).spawn(move || {
        updater.run(update_status::FIRST_CHECK, update_status::EVERY, &mut |wait| {
            thread::sleep(wait);
            true
        })
    });
    if let Err(e) = checks {
        aipet_core::log::write(&format!("updates: {e}"));
    }
    true
}

/// The app's version, as released.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Where the update stands now, for Settings: Off in a copy that doesn't update itself.
pub fn status() -> UpdateStatus {
    UPDATER
        .get()
        .map_or_else(|| UpdateStatus::new(Kind::Off), |updater| updater.status())
}

/// Settings' Check for updates: a check now, off the UI thread.
pub fn check() {
    if let Some(updater) = UPDATER.get() {
        let updater = Arc::clone(updater);
        let started = thread::Builder::new()
            .name("update check".into())
            .spawn(move || updater.check());
        if let Err(e) = started {
            aipet_core::log::write(&format!("updates: {e}"));
        }
    }
}

/// Settings' Restart to update: the update that is ready installs once the pet has quit, and the updater starts it
/// again. True when the pet should now quit.
pub fn restart() -> bool {
    UPDATER.get().is_some_and(|updater| updater.apply(true))
}

/// The pet quits: an update that is ready installs once it has exited (the updater waits for that), without a
/// window, and the pet stays closed (`Updates.InstallOnQuit`).
pub fn install_on_quit() {
    if let Some(updater) = UPDATER.get() {
        updater.apply(false);
    }
}

/// What the updates need of Velopack's update manager: the seam tests put a fake behind. Errors are their messages.
trait Manager: Send + Sync {
    /// An update downloaded earlier and not installed yet (`UpdatePendingRestart`).
    fn pending(&self) -> Option<VelopackAsset>;
    /// A newer release, if there is one (`CheckForUpdatesAsync`).
    fn check(&self) -> Result<Option<UpdateInfo>, String>;
    /// Downloads it, saying how far it got in percent (`DownloadUpdatesAsync`).
    fn download(&self, update: &UpdateInfo, progress: &mut dyn FnMut(i16)) -> Result<(), String>;
    /// Starts the updater, which installs the update once this process has exited, and then starts the pet again or
    /// not (`WaitExitThenApplyUpdates`).
    fn apply(&self, update: &VelopackAsset, silent: bool, restart: bool) -> Result<(), String>;
}

/// Velopack's own update manager.
struct Velopack(velopack::UpdateManager);

impl Manager for Velopack {
    fn pending(&self) -> Option<VelopackAsset> {
        self.0.get_update_pending_restart()
    }

    fn check(&self) -> Result<Option<UpdateInfo>, String> {
        match self.0.check_for_updates() {
            Ok(UpdateCheck::UpdateAvailable(update)) => Ok(Some(*update)),
            Ok(UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn download(&self, update: &UpdateInfo, progress: &mut dyn FnMut(i16)) -> Result<(), String> {
        let (said, heard) = mpsc::channel();
        thread::scope(|scope| {
            let download = scope.spawn(move || self.0.download_updates(update, Some(said)));
            // until the download ends, which drops its sender
            for percent in heard {
                progress(percent);
            }
            match download.join() {
                Ok(done) => done.map_err(|e| e.to_string()),
                Err(_) => Err("the download failed".to_owned()),
            }
        })
    }

    fn apply(&self, update: &VelopackAsset, silent: bool, restart: bool) -> Result<(), String> {
        self.0
            .wait_exit_then_apply_updates(update, silent, restart, Vec::<String>::new())
            .map_err(|e| e.to_string())
    }
}

/// The installed copy's updates: the C#'s static `Updates` state, over a [`Manager`].
struct Updater {
    manager: Box<dyn Manager>,
    log: Box<dyn Fn(&str) + Send + Sync>,
    state: Mutex<State>,
    /// A check runs: one at a time.
    busy: AtomicBool,
    /// Every status set, for tests.
    #[cfg(test)]
    history: Mutex<Vec<UpdateStatus>>,
}

struct State {
    status: UpdateStatus,
    /// The update downloaded and ready to install.
    ready: Option<VelopackAsset>,
}

impl Updater {
    fn new(manager: Box<dyn Manager>, log: Box<dyn Fn(&str) + Send + Sync>) -> Self {
        Updater {
            manager,
            log,
            state: Mutex::new(State {
                status: UpdateStatus::new(Kind::Off),
                ready: None,
            }),
            busy: AtomicBool::new(false),
            #[cfg(test)]
            history: Mutex::new(Vec::new()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn status(&self) -> UpdateStatus {
        self.state().status.clone()
    }

    fn set(&self, status: UpdateStatus) {
        #[cfg(test)]
        self.history
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(status.clone());
        self.state().status = status;
    }

    fn ready_update(&self) -> Option<VelopackAsset> {
        self.state().ready.clone()
    }

    /// Ready, with the version, when an update is.
    fn ready(&self) -> Option<UpdateStatus> {
        self.ready_update()
            .map(|ready| with(Kind::Ready, Some(ready.Version), 0, None))
    }

    /// `Start` once the manager is there: an update downloaded earlier installs first (false: the pet exits), but not
    /// on the start that follows an update, which would go round again if that update failed. Then the checks are on.
    fn start(&self, restarted: bool) -> bool {
        if !restarted && let Some(pending) = self.manager.pending() {
            (self.log)(&format!("updates: installing {}, downloaded earlier", pending.Version));
            match self.manager.apply(&pending, false, true) {
                Ok(()) => return false,
                Err(e) => (self.log)(&format!("updates: {e}")),
            }
        }
        self.set(UpdateStatus::new(Kind::Idle));
        true
    }

    /// The checks' thread: one downloaded before and still not installed is ready (the start that follows an update
    /// leaves it), then a check after `first`, and every `every` after that, while `wait` says to go on.
    fn run(&self, first: Duration, every: Duration, wait: &mut dyn FnMut(Duration) -> bool) {
        if let Some(pending) = self.manager.pending() {
            let version = pending.Version.clone();
            self.state().ready = Some(pending);
            self.set(with(Kind::Ready, Some(version), 0, None));
        }
        if !wait(first) {
            return;
        }
        loop {
            self.check();
            if !wait(every) {
                return;
            }
        }
    }

    /// Checks for a newer release now and downloads it (`CheckAsync`). One check at a time.
    fn check(&self) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        // the check is over even if the manager panics
        struct Done<'a>(&'a AtomicBool);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _done = Done(&self.busy);
        let mut version = None;
        if let Err(e) = self.check_and_download(&mut version) {
            let what = version
                .as_ref()
                .map_or_else(|| "check".to_owned(), |version| format!("download of {version}"));
            (self.log)(&format!("updates: {what} failed: {e}"));
            // an update downloaded earlier is still there to install
            self.set(self.ready().unwrap_or_else(|| with(Kind::Failed, version, 0, Some(e))));
        }
    }

    fn check_and_download(&self, version: &mut Option<String>) -> Result<(), String> {
        self.set(UpdateStatus::new(Kind::Checking));
        let Some(update) = self.manager.check()? else {
            self.set(self.ready().unwrap_or_else(|| UpdateStatus::new(Kind::UpToDate)));
            return Ok(());
        };
        let target = update.TargetFullRelease.clone();
        *version = Some(target.Version.clone());
        if self.ready_update().map(|ready| ready.Version) != Some(target.Version.clone()) {
            self.set(with(Kind::Downloading, Some(target.Version.clone()), 0, None));
            self.manager.download(&update, &mut |percent| {
                self.set(with(
                    Kind::Downloading,
                    Some(target.Version.clone()),
                    percent.into(),
                    None,
                ));
            })?;
            self.state().ready = Some(target);
        }
        if let Some(ready) = self.ready() {
            self.set(ready);
        }
        Ok(())
    }

    /// Installs the ready update once the pet has exited (`Apply`): with the updater's window and the pet started
    /// again for Restart to update, silently and the pet left closed at quit. True when the updater started.
    fn apply(&self, restart: bool) -> bool {
        let Some(ready) = self.ready_update() else {
            return false;
        };
        match self.manager.apply(&ready, !restart, restart) {
            Ok(()) => true,
            Err(e) => {
                (self.log)(&format!("updates: couldn't start the update: {e}"));
                self.set(with(Kind::Failed, Some(ready.Version), 0, Some(e)));
                false
            }
        }
    }
}

fn with(kind: Kind, version: Option<String>, percent: i32, error: Option<String>) -> UpdateStatus {
    UpdateStatus {
        kind,
        version,
        percent,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A fake update manager: what each call answers, and the calls made.
    #[derive(Default)]
    struct Fake {
        pending: Mutex<Option<VelopackAsset>>,
        checks: Mutex<VecDeque<Result<Option<UpdateInfo>, String>>>,
        downloads: Mutex<VecDeque<Result<(), String>>>,
        applies: Mutex<VecDeque<Result<(), String>>>,
        calls: Mutex<Vec<String>>,
    }

    impl Fake {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn call(&self, call: String) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl Manager for Arc<Fake> {
        fn pending(&self) -> Option<VelopackAsset> {
            self.call("pending".into());
            self.pending.lock().unwrap().clone()
        }

        fn check(&self) -> Result<Option<UpdateInfo>, String> {
            self.call("check".into());
            self.checks.lock().unwrap().pop_front().unwrap_or(Ok(None))
        }

        fn download(&self, update: &UpdateInfo, progress: &mut dyn FnMut(i16)) -> Result<(), String> {
            self.call(format!("download {}", update.TargetFullRelease.Version));
            let outcome = self.downloads.lock().unwrap().pop_front().unwrap_or(Ok(()));
            progress(40);
            if outcome.is_ok() {
                progress(100);
            }
            outcome
        }

        fn apply(&self, update: &VelopackAsset, silent: bool, restart: bool) -> Result<(), String> {
            self.call(format!("apply {} silent={silent} restart={restart}", update.Version));
            self.applies.lock().unwrap().pop_front().unwrap_or(Ok(()))
        }
    }

    fn asset(version: &str) -> VelopackAsset {
        VelopackAsset {
            PackageId: "AiPetApp".into(),
            Version: version.into(),
            FileName: format!("AiPetApp-{version}-full.nupkg"),
            ..VelopackAsset::default()
        }
    }

    fn release(version: &str) -> UpdateInfo {
        UpdateInfo {
            TargetFullRelease: asset(version),
            ..UpdateInfo::default()
        }
    }

    type Log = Arc<Mutex<Vec<String>>>;

    /// An updater over a fake, and what it logged.
    fn updater() -> (Updater, Arc<Fake>, Log) {
        let fake = Arc::new(Fake::default());
        let logged = Log::default();
        let log = Arc::clone(&logged);
        let updater = Updater::new(
            Box::new(Arc::clone(&fake)),
            Box::new(move |line| log.lock().unwrap().push(line.to_owned())),
        );
        (updater, fake, logged)
    }

    fn texts(updater: &Updater) -> Vec<String> {
        updater.history.lock().unwrap().iter().map(UpdateStatus::text).collect()
    }

    const READY_120: &str = "AiPet 1.2.0 is ready. It's installed when you quit the pet, or restart now.";

    /// Start installs an update downloaded earlier before anything else, with the updater's window and the pet
    /// started again, and the pet exits.
    #[test]
    fn an_update_downloaded_earlier_installs_first_and_the_pet_exits() {
        let (updater, fake, logged) = updater();
        *fake.pending.lock().unwrap() = Some(asset("1.2.0"));
        assert!(!updater.start(false));
        assert_eq!(fake.calls(), ["pending", "apply 1.2.0 silent=false restart=true"]);
        assert_eq!(
            *logged.lock().unwrap(),
            ["updates: installing 1.2.0, downloaded earlier"]
        );
        assert_eq!(updater.status().kind, Kind::Off);
    }

    /// The start that follows an update doesn't install a pending one again (that would go round again if the
    /// update failed): it is ready, to install at quit.
    #[test]
    fn the_start_after_an_update_leaves_a_pending_one_ready() {
        let (updater, fake, _) = updater();
        *fake.pending.lock().unwrap() = Some(asset("1.2.0"));
        assert!(updater.start(true));
        assert_eq!(
            updater.status().text(),
            "Checks for updates a few minutes after the pet starts, then every few hours."
        );
        updater.run(Duration::ZERO, Duration::ZERO, &mut |_| false);
        assert_eq!(fake.calls(), ["pending"]);
        assert_eq!(updater.status().text(), READY_120);
    }

    /// An update that can't start installing at start leaves the pet running, with the checks on.
    #[test]
    fn a_pending_update_that_cant_start_leaves_the_pet_running() {
        let (updater, fake, logged) = updater();
        *fake.pending.lock().unwrap() = Some(asset("1.2.0"));
        fake.applies
            .lock()
            .unwrap()
            .push_back(Err("Update.exe is missing".into()));
        assert!(updater.start(false));
        assert_eq!(updater.status().kind, Kind::Idle);
        assert_eq!(
            *logged.lock().unwrap(),
            [
                "updates: installing 1.2.0, downloaded earlier",
                "updates: Update.exe is missing"
            ]
        );
    }

    /// The first check comes a few minutes after the start, the next ones every few hours.
    #[test]
    fn the_checks_come_a_while_after_the_start_and_then_every_few_hours() {
        let (updater, fake, _) = updater();
        assert!(updater.start(false));
        let mut waits = Vec::new();
        updater.run(update_status::FIRST_CHECK, update_status::EVERY, &mut |wait| {
            waits.push(wait);
            waits.len() < 3
        });
        assert_eq!(
            waits,
            [update_status::FIRST_CHECK, update_status::EVERY, update_status::EVERY]
        );
        assert_eq!(fake.calls(), ["pending", "pending", "check", "check"]);
        assert_eq!(updater.status().text(), "Up to date.");
    }

    /// A check finds a newer release and downloads it, saying how far it got; then it is ready.
    #[test]
    fn a_check_downloads_a_newer_release_and_says_how_far() {
        let (updater, fake, logged) = updater();
        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.2.0"))));
        updater.check();
        assert_eq!(fake.calls(), ["check", "download 1.2.0"]);
        assert_eq!(
            texts(&updater),
            [
                "Checking for updates…",
                "Downloading AiPet 1.2.0… 0%",
                "Downloading AiPet 1.2.0… 40%",
                "Downloading AiPet 1.2.0… 100%",
                READY_120,
            ]
        );
        assert!(logged.lock().unwrap().is_empty());

        // the same release found again isn't downloaded again
        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.2.0"))));
        updater.check();
        assert_eq!(fake.calls(), ["check", "download 1.2.0", "check"]);
        assert_eq!(updater.status().text(), READY_120);
        // and with nothing newer the ready one stays
        updater.check();
        assert_eq!(updater.status().text(), READY_120);
    }

    /// Offline, rate limited or any other failure is the status text, and the log says which step failed.
    #[test]
    fn a_failed_check_or_download_is_the_status_text() {
        let (updater, fake, logged) = updater();
        fake.checks
            .lock()
            .unwrap()
            .push_back(Err("Network error: rate limited".into()));
        updater.check();
        assert_eq!(
            updater.status().text(),
            "Couldn't check for updates: Network error: rate limited"
        );
        assert!(!updater.status().busy());

        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.2.0"))));
        fake.downloads.lock().unwrap().push_back(Err("offline".into()));
        updater.check();
        assert_eq!(updater.status().text(), "Couldn't update to AiPet 1.2.0: offline");
        assert_eq!(
            *logged.lock().unwrap(),
            [
                "updates: check failed: Network error: rate limited",
                "updates: download of 1.2.0 failed: offline"
            ]
        );

        // with an update downloaded earlier, a failed download of a newer one leaves that one ready
        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.2.0"))));
        updater.check();
        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.3.0"))));
        fake.downloads.lock().unwrap().push_back(Err("offline".into()));
        updater.check();
        assert_eq!(updater.status().text(), READY_120);
        assert_eq!(updater.ready_update().unwrap().Version, "1.2.0");
    }

    /// One check at a time: a check asked for while one runs does nothing.
    #[test]
    fn one_check_at_a_time() {
        let (updater, fake, _) = updater();
        updater.busy.store(true, Ordering::Release);
        updater.check();
        assert!(fake.calls().is_empty());
        updater.busy.store(false, Ordering::Release);
        updater.check();
        assert_eq!(fake.calls(), ["check"]);
        assert!(!updater.busy.load(Ordering::Acquire));
    }

    /// Quitting installs the ready update silently and leaves the pet closed; Restart to update shows the updater
    /// and starts the pet again. With nothing ready, neither starts the updater (and Restart doesn't quit the pet).
    #[test]
    fn quitting_and_restarting_install_the_ready_update() {
        let (updater, fake, _) = updater();
        assert!(!updater.apply(false));
        assert!(!updater.apply(true));
        assert!(fake.calls().is_empty());

        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.2.0"))));
        updater.check();
        assert!(updater.apply(false));
        assert!(updater.apply(true));
        assert_eq!(
            fake.calls()[2..],
            [
                "apply 1.2.0 silent=true restart=false",
                "apply 1.2.0 silent=false restart=true"
            ]
        );
    }

    /// An update that can't start leaves the old version running: Restart doesn't quit the pet, and the status says
    /// why.
    #[test]
    fn a_failed_apply_keeps_the_old_version_running() {
        let (updater, fake, logged) = updater();
        fake.checks.lock().unwrap().push_back(Ok(Some(release("1.2.0"))));
        updater.check();
        fake.applies.lock().unwrap().push_back(Err("Access is denied.".into()));
        assert!(!updater.apply(true));
        assert_eq!(
            updater.status().text(),
            "Couldn't update to AiPet 1.2.0: Access is denied."
        );
        assert_eq!(
            *logged.lock().unwrap(),
            ["updates: couldn't start the update: Access is denied."]
        );
    }

    /// Velopack's startup is the first thing `main` does: vpk checks that only for .NET apps (task 14's proof).
    #[test]
    fn velopack_runs_before_anything_else_in_main() {
        let main = include_str!("main.rs");
        let first = main
            .lines()
            .map(str::trim)
            .skip_while(|line| *line != "fn main() -> ExitCode {")
            .skip(1)
            .find(|line| !line.is_empty() && !line.starts_with("//"))
            .expect("main.rs has fn main");
        assert_eq!(first, "updates::startup();");
        assert_eq!(main.matches("updates::startup()").count(), 1);
    }

    /// A copy Velopack didn't install (a build from source, the portable zip, Linux) starts as usual: Velopack's
    /// startup finds no install and returns, and the copy doesn't update itself.
    #[test]
    fn a_copy_velopack_didnt_install_starts_as_usual() {
        app(Vec::new(), false, Path::new("aipet-hook.exe")).run();
        assert!(!installed());
        assert!(start());
        assert_eq!(status().kind, Kind::Off);
        assert!(!restart());
    }

    /// The hook the uninstaller cleans up after is the installed one, next to the app.
    #[test]
    fn the_hook_is_next_to_the_app() {
        let exe = std::env::current_exe().unwrap();
        assert_eq!(hook(), exe.parent().unwrap().join("aipet-hook.exe"));
    }

    /// The uninstaller starts the app with `--veloapp-uninstall`: Velopack's startup runs the hook cleanup for each
    /// agent whose config names the hook, and exits. In a process of its own (it exits), with the agents' configs
    /// and the data folder in a temporary folder; `whoami.exe` stands in for the hook and refuses the arguments.
    #[cfg(windows)]
    #[test]
    fn the_uninstaller_runs_the_hook_cleanup() {
        use std::fs;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        let dir = std::env::temp_dir().join(format!("aipet-updates-uninstall-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (claude, codex, data) = (dir.join("claude"), dir.join("codex"), dir.join("data"));
        for folder in [&claude, &codex, &data] {
            fs::create_dir_all(folder).unwrap();
        }
        let named = stand_in().to_string_lossy().replace('\\', "/");
        let config = format!(r#"{{"command":"{named}"}}"#);
        fs::write(claude.join("settings.json"), &config).unwrap();
        fs::write(codex.join("hooks.json"), &config).unwrap();

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "updates::tests::uninstall_in_a_process_of_its_own",
                "--exact",
                "--include-ignored",
            ])
            .env("AIPET_TEST_UNINSTALL", "1")
            .env("CLAUDE_CONFIG_DIR", &claude)
            .env("CODEX_HOME", &codex)
            .env("AIPET_DATA_DIR", &data)
            .env_remove("VELOPACK_DEBUG")
            .env_remove("VELOPACK_RESTART")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the uninstall hook didn't end within 60 s");
            }
            thread::sleep(Duration::from_millis(50));
        };
        let log = fs::read_to_string(data.join("aipet.log")).unwrap_or_default();
        let _ = fs::remove_dir_all(&dir);
        assert!(status.success(), "{status:?}: {log}");
        for agent in ["claude", "codex"] {
            let line = format!("uninstall: aipet-hook --uninstall {agent} (exit 1): ");
            assert!(log.contains(&line), "{log}");
        }
    }

    /// A program that is surely there, for the hook (as aipet-core's cleanup tests have it).
    #[cfg(windows)]
    fn stand_in() -> PathBuf {
        std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| r"C:\Windows".into())
            .join("System32")
            .join("whoami.exe")
    }

    /// The uninstaller's start, for `the_uninstaller_runs_the_hook_cleanup`: Velopack exits after the hook.
    #[cfg(windows)]
    #[test]
    #[ignore = "runs in a process of its own, started by the_uninstaller_runs_the_hook_cleanup"]
    fn uninstall_in_a_process_of_its_own() {
        if std::env::var_os("AIPET_TEST_UNINSTALL").is_none() {
            return;
        }
        startup_with(vec!["--veloapp-uninstall".into(), "0.3.0".into()], false, stand_in());
        panic!("Velopack's startup returned after the uninstall hook instead of exiting");
    }
}

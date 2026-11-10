//! The pet: one per user, its core on a thread of its own, and a shell for its windows.
//!
//! It starts in this order:
//! 1. Velopack's startup ([`updates::startup`]), before anything else;
//! 2. the single instance ([`single`]), before anything but Velopack's startup, as the .NET pet takes it: when
//!    another pet runs, .NET or Rust, this one quits quietly;
//! 3. the GPU's and the shell's environment, while no other thread reads or writes it;
//! 4. the update checks ([`updates::start`]);
//! 5. the core ([`core_thread`]), with `config.json` read ([`config`]) and the platform's services, and beside its
//!    pipe server the HTTP listener for MCP and Claude's HTTP hooks (`aipet_mcp`), which shares the core's way in for
//!    hook events; when its port is taken, the log says so once and the pet runs without it;
//! 6. the shell, until the pet quits; then the listener and the core stop, and the single instance is let go.
//!
//! The app's side of the pet ([`App`], the pet's [`Host`]) keeps `config.json`, tells the core of dismissed bubbles and
//! of music, places the window ([`placement`]) and does Settings' actions.
//!
//! `AIPET_BACKEND=wayland|desktop` chooses the shell; otherwise the Wayland shell runs when the session is Wayland and
//! its compositor has layer-shell, and the desktop shell everywhere else (through XWayland in a Wayland session).
//! `AIPET_DEBUG=1` says which shell runs and makes it log pointer events and input-region updates to stderr;
//! `AIPET_OPEN_SETTINGS=1` opens Settings at start (both shells), and `AIPET_OPEN_SETTINGS=avatars` (or another
//! page's name) opens it on that page. The pet renders on the low-power GPU unless `WGPU_POWER_PREF` says otherwise,
//! and through GL (Vulkan on Windows) unless `WGPU_BACKEND` does; on Linux, with only Mesa's EGL driver loaded when an
//! Intel or AMD GPU drives the screen ([`mesa`]), unless `__EGL_VENDOR_LIBRARY_FILENAMES` says otherwise.
//!
//! With the `demo` feature it plays the demo's scripted day instead, for development and screenshots: no single
//! instance, no core, and nothing read or written in the data folder.

// a GUI program in a release (Velopack's main exe, started from the Start menu): no console window
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(feature = "demo"))]
mod config;
#[cfg(not(feature = "demo"))]
mod core_thread;
#[cfg(target_os = "linux")]
mod mesa;
#[cfg(not(feature = "demo"))]
mod placement;
#[cfg(not(feature = "demo"))]
mod single;
#[cfg_attr(
    feature = "demo",
    allow(dead_code, reason = "the demo calls only Velopack's startup")
)]
mod updates;

use std::any::Any;
use std::env;
use std::process::ExitCode;
use std::sync::mpsc;

use aipet_ui::{Launch, PetUi, Setup};

/// Whether the desktop shell runs through X11 here: where winit has its X11 backend (Linux and the BSDs).
const X11: bool = cfg!(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
));

/// `AiPet --check-pack <folder or theme file>`: loads the pack as the pet would, with the same limits, and prints `OK
/// <name> (<kind>, <layers> layers, <frames> frames[, themes <names>])` (exit 0) or why it can't (exit 1); a theme file
/// is put on its pet (`OK <pet> in <theme> (a theme)`). It never starts the pet or writes.
fn check_pack(folder: Option<std::ffi::OsString>) -> ExitCode {
    // a GUI app on Windows has no console of its own: print to the one it was started from
    #[cfg(windows)]
    // SAFETY: no arguments to get wrong; failing (no parent console) leaves nothing to print to, which is fine
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
    let Some(folder) = folder else {
        eprintln!("usage: AiPet --check-pack <pack folder, theme file or legacy avatar file>");
        return ExitCode::FAILURE;
    };
    let (line, ok) = aipet_sprite::load::check(std::path::Path::new(&folder));
    println!("{line}");
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn main() -> ExitCode {
    updates::startup();
    cap_malloc_arenas();
    let mut args = env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--check-pack") {
        return check_pack(args.next());
    }
    #[cfg(not(feature = "demo"))]
    let instance = match only_pet() {
        Ok(Some(instance)) => instance,
        // another pet runs: quietly
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aipet: {e}");
            return ExitCode::FAILURE;
        }
    };
    set_gpu_defaults();
    let (wayland, why) = match choose_shell() {
        Ok(choice) => choice,
        Err(e) => {
            eprintln!("aipet: {e}");
            return ExitCode::FAILURE;
        }
    };
    if env::var_os("AIPET_DEBUG").is_some_and(|v| v == "1") {
        eprintln!(
            "aipet: the {} shell ({why})",
            if wayland { "Wayland" } else { "desktop" }
        );
    }
    if !wayland && X11 {
        // X11, even in a Wayland session (see aipet_desktop): winit takes an empty WAYLAND_DISPLAY as none, and
        // libwayland, which would try wayland-0 without one, can't connect
        // SAFETY: no other thread reads or writes the environment: the Wayland probe starts none, and the single
        // instance leaves none that does (`only_pet`)
        unsafe {
            env::set_var("WAYLAND_DISPLAY", "");
            env::remove_var("WAYLAND_SOCKET");
        }
    }
    #[cfg(not(feature = "demo"))]
    let result = pet(wayland, instance);
    #[cfg(feature = "demo")]
    let result = pet(wayland);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aipet: {e}");
            ExitCode::FAILURE
        }
    }
}

/// At most two malloc arenas (glibc's), unless `MALLOC_ARENA_MAX` says otherwise. glibc gives each thread that
/// allocates an arena of its own, up to eight per core, and keeps what is freed in them: the hook server's thread per
/// connection left the pet 5 MB bigger for good after a burst of 3000 hook events, and 2 MB with two arenas, at the
/// same CPU (rust/proofs/memory.md). It runs right after Velopack's startup (which must come first), before any thread
/// starts.
fn cap_malloc_arenas() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if env::var_os("MALLOC_ARENA_MAX").is_none() {
        // SAFETY: mallopt only sets a number glibc's malloc reads; failing leaves the default, which is fine
        unsafe { libc::mallopt(libc::M_ARENA_MAX, 2) };
    }
}

/// The GPU the pet renders on, unless the user said: the low-power one, through GL.
fn set_gpu_defaults() {
    if env::var_os("WGPU_POWER_PREF").is_none() {
        // a desktop pet has no business waking a discrete GPU (iced asks wgpu for the fastest one). It also dodges
        // an NVIDIA quirk: its Wayland surfaces offer an fp16 format first, which iced (with web colours) picks and
        // then fills with sRGB values the compositor takes for linear ones, washing every colour out.
        // SAFETY: no other thread reads or writes the environment: the single instance leaves none that does
        // (`only_pet`)
        unsafe { env::set_var("WGPU_POWER_PREF", "low") };
    }
    if (cfg!(target_os = "linux") || cfg!(windows)) && env::var_os("WGPU_BACKEND").is_none() {
        // On Linux GL only, in either shell: to list Vulkan adapters the loader opens every driver, and NVIDIA's keeps
        // a hybrid laptop's discrete GPU awake the whole time the pet runs, though the pet renders on the integrated
        // one; through EGL the pet renders with the driver of the GPU the display server uses, on a transparent
        // surface (a layer surface, or XWayland's 32-bit window), and shows at once instead of 2 s later.
        // On Windows Vulkan only: through GL, stale copies of the pet stayed on screen where it had been when it
        // jumped or changed size, and Vulkan has none (the owner's run, 2026-11-10). It shows the pet in 4.5 to 7.3 s
        // where GL took 0.9 s and loads the NVIDIA driver, until the pet draws without the GPU (FNT-75). Never DX12,
        // whose window iced 0.14 can't make transparent (rust/proofs/windows.md).
        let backend = if cfg!(windows) { "vulkan" } else { "gl" };
        // SAFETY: no other thread reads or writes the environment: the single instance leaves none that does
        // (`only_pet`)
        unsafe { env::set_var("WGPU_BACKEND", backend) };
    }
    // SAFETY: as above
    #[cfg(target_os = "linux")]
    unsafe {
        mesa::load_only_mesa()
    };
}

/// Whether the Wayland shell runs, and why.
fn choose_shell() -> Result<(bool, &'static str), String> {
    // winit's test (an empty value is none). A session with only WAYLAND_SOCKET isn't probed: that socket connects
    // once, and the probe would use it up; AIPET_BACKEND=wayland takes it.
    let wayland_session = env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    Ok(match env::var("AIPET_BACKEND").ok().as_deref() {
        Some("wayland") => (true, "AIPET_BACKEND"),
        Some("desktop") => (false, "AIPET_BACKEND"),
        Some(other) => return Err(format!("AIPET_BACKEND must be wayland or desktop, not {other:?}")),
        None if !wayland_session => (false, "not a Wayland session"),
        None if aipet_wayland::available() => (true, "the compositor has layer-shell"),
        None => (false, "no layer-shell compositor to connect to"),
    })
}

/// The single instance ([`single`]): the proof while this is the only pet, none when another pet runs, .NET or Rust.
/// A lock that can't be taken for another reason is an error, logged too.
///
/// It leaves no thread that reads or writes the environment, which `main` sets after it: on Windows it starts none,
/// and on Unix the only one, the probe's connect thread, may still be ending, but only connects a socket.
#[cfg(not(feature = "demo"))]
fn only_pet() -> Result<Option<single::Instance>, String> {
    single::take(&single::Names::app()).map_err(|e| {
        let e = format!("can't tell whether another pet runs, so this one doesn't: {e}");
        aipet_core::log::write(&e);
        e
    })
}

/// The pet, the only one while `instance` is held: the update checks, the core, and the shell until the pet quits.
#[cfg(not(feature = "demo"))]
fn pet(wayland: bool, instance: single::Instance) -> Result<(), String> {
    use std::sync::Arc;

    use aipet_sprite::Avatar;
    use aipet_ui::Prefs;
    use aipet_ui::settings::Services;

    // an update downloaded earlier installs first, and the updater starts the pet again
    if !updates::start() {
        return Ok(());
    }
    let data_dir = aipet_ipc::paths::data_dir().to_owned();
    // as MainWindow does; a folder that can't be made shows in what can't be written there
    let _ = std::fs::create_dir_all(&data_dir);
    let config = config::Store::load(aipet_ipc::paths::config());
    let prefs = Prefs::from(config.get());
    let platform = aipet_desktop::platform::new();
    let options = core_thread::Options::app(Arc::clone(&platform), prefs.music);
    let (secrets, http) = (Arc::clone(&options.secrets), options.http.clone());
    let (news, heard) = mpsc::channel();
    let core = core_thread::start(options, news);
    // a pet without the listener (its port taken) still has the pipe and Settings: the start logged why
    let listener = aipet_mcp::Listener::start(aipet_mcp::Options::app(updates::version()), core.ingress()).ok();
    let services = Services {
        data_dir: data_dir.clone(),
        jira: Some(core.jira()),
        github: Some(core.github()),
        secrets,
        http,
        version: updates::version(),
        update_status: updates::status,
    };
    let app = App {
        config,
        core: core.handle(),
        platform: Arc::clone(&platform),
        data_dir: data_dir.clone(),
        restarting: false,
        quitting: false,
    };
    let ui = PetUi::new(Setup {
        host: Box::new(app),
        platform,
        prefs,
        avatars: Avatar::custom_dir(&data_dir),
        services,
    });
    let shown = shell(wayland, Launch { ui, news: heard });
    // the listener and the core stop (its server and watchers with it) before the single instance is let go
    drop(listener);
    drop(core);
    drop(instance);
    shown.inspect_err(|e| aipet_core::log::write(e))
}

/// The demo's scripted day, in the shell: no single instance, no core, nothing read or written in the data folder.
#[cfg(feature = "demo")]
fn pet(wayland: bool) -> Result<(), String> {
    /// The repository's avatars folder, next to the built-ins.
    const AVATARS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../avatars");
    let ui = PetUi::demo(Setup {
        platform: aipet_desktop::platform::new(),
        avatars: AVATARS.into(),
        ..Setup::detached()
    });
    let (_, heard) = mpsc::channel();
    shell(wayland, Launch { ui, news: heard })
}

/// Runs the pet in the Wayland shell or the desktop one, until it quits.
fn shell(wayland: bool, launch: Launch) -> Result<(), String> {
    if wayland {
        // its `run` takes no arguments: the pet is handed over
        launch.hand_over();
        // iced_exwlshell panics where it could fail (without a wgpu adapter for the surface), so a panic is a
        // failure too, said after the panic's own message
        std::panic::catch_unwind(aipet_wayland::run)
            .unwrap_or_else(|panic| Err(panic_text(&*panic).to_owned()))
            .map_err(|e| format!("the Wayland shell failed: {e} (AIPET_BACKEND=desktop runs the pet through XWayland)"))
    } else {
        aipet_desktop::run(launch).map_err(|e| format!("the desktop shell failed: {e}"))
    }
}

/// The app's side of the pet: `config.json`, the core, the window's place, Settings' actions and updates.
#[cfg(not(feature = "demo"))]
struct App {
    config: config::Store,
    core: core_thread::Handle,
    platform: std::sync::Arc<dyn aipet_ui::platform::Platform>,
    data_dir: std::path::PathBuf,
    /// Restart to update started the update: quitting mustn't start it again.
    restarting: bool,
    /// The quit has been heard: the menu's Quit, say, and then the window closing are one quit.
    quitting: bool,
}

/// The pet's preferences into the config the file keeps.
#[cfg(any(test, not(feature = "demo")))]
fn apply_prefs(config: &mut aipet_core::config::Config, prefs: &aipet_ui::Prefs) {
    config.pills = prefs.bubbles;
    config.on_top = prefs.on_top;
    config.music = prefs.music;
    config.avatar = prefs.avatar.clone();
    config.avatar_theme = prefs.avatar_theme.clone();
    config.avatar_style = prefs.avatar_style.clone();
    // a size at 100 % is the default, which the file leaves out
    config.pet_size = (prefs.pet_size != 100).then_some(prefs.pet_size);
    config.bubble_size = (prefs.bubble_size != 100).then_some(prefs.bubble_size);
}

#[cfg(not(feature = "demo"))]
impl aipet_ui::Host for App {
    fn prefs(&mut self, prefs: &aipet_ui::Prefs) {
        let written = self.config.change(|config| apply_prefs(config, prefs));
        if let Err(e) = written {
            aipet_core::log::write(&format!("config: can't write config.json: {e}"));
        }
        self.core.music(prefs.music);
    }

    fn dismiss(&mut self, id: &str) {
        self.core.dismiss(id);
    }

    fn settings(&mut self, action: aipet_ui::settings::Action) -> bool {
        use aipet_ui::settings::Action;
        match action {
            Action::OpenDataFolder => {
                let _ = std::fs::create_dir_all(&self.data_dir);
                self.platform.open_folder(&self.data_dir);
                false
            }
            Action::CheckForUpdates => {
                updates::check();
                false
            }
            Action::RestartToUpdate => {
                self.restarting = updates::restart();
                self.restarting
            }
            // the shell's (it moves the window), and the General page's own
            Action::ResetPosition | Action::ImportDefaults => false,
        }
    }

    fn saved_place(&mut self) -> Option<aipet_ui::Place> {
        placement::load(&self.config)
    }

    fn save_place(&mut self, place: aipet_ui::Place) {
        placement::save(&mut self.config, place);
    }

    fn reset_place(&mut self) {
        placement::reset(&mut self.config);
    }

    fn quit(&mut self) {
        if std::mem::replace(&mut self.quitting, true) {
            return;
        }
        placement::quit(&mut self.config);
        if let Err(e) = self.config.flush() {
            aipet_core::log::write(&format!("config: can't write config.json: {e}"));
        }
        if !self.restarting {
            updates::install_on_quit();
        }
    }
}

/// What a panic said: the message `panic!` and `expect` give it, a `&str` or a `String`.
fn panic_text(payload: &(dyn Any + Send)) -> &str {
    match (payload.downcast_ref::<&str>(), payload.downcast_ref::<String>()) {
        (Some(text), _) => text,
        (_, Some(text)) => text,
        _ => "it panicked",
    }
}

/// Held while a test starts a process: until it has exec'd, the new process holds a copy of every file this one has
/// open, so a test that lets go of a lock and takes it again holds this too (`single.rs`).
#[cfg(test)]
static SPAWNING: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
fn tests_spawning() -> std::sync::MutexGuard<'static, ()> {
    SPAWNING.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Starts a test's process under [`SPAWNING`]; `spawn` returns once the process has exec'd.
#[cfg(test)]
fn tests_spawn<T>(spawn: impl FnOnce() -> T) -> T {
    let _spawning = tests_spawning();
    spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_style_goes_into_config_json_and_comes_back() {
        // FNT-74 R4: the app's own copy of the preferences writes AvatarStyle, only when it is set
        let dir = std::env::temp_dir().join(format!("aipet-style-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut config = aipet_core::config::Config::default();
        let prefs = aipet_ui::Prefs {
            avatar: Some("Hood".into()),
            avatar_style: Some("Vector".into()),
            ..aipet_ui::Prefs::default()
        };
        apply_prefs(&mut config, &prefs);
        config.save_to(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#""Avatar":"Hood","AvatarStyle":"Vector""#), "{text}");
        let (read, _) = aipet_core::config::Config::load_from(&path);
        assert_eq!(aipet_ui::Prefs::from(&read).avatar_style.as_deref(), Some("Vector"));
        apply_prefs(
            &mut config,
            &aipet_ui::Prefs {
                avatar_style: None,
                ..prefs
            },
        );
        config.save_to(&path).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("AvatarStyle"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_panic_says_what_it_panicked_with() {
        let literal: Box<dyn Any + Send> = Box::new("no adapter");
        let formatted: Box<dyn Any + Send> =
            Box::new(format!("Cannot create compositor: {:?}", "GraphicsAdapterNotFound"));
        let other: Box<dyn Any + Send> = Box::new(7);
        assert_eq!(panic_text(&*literal), "no adapter");
        assert_eq!(
            panic_text(&*formatted),
            "Cannot create compositor: \"GraphicsAdapterNotFound\""
        );
        assert_eq!(panic_text(&*other), "it panicked");
    }
}

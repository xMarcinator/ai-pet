//! The pet: one per user, its core on a thread of its own, and a shell for its windows.
//!
//! It starts in this order:
//! 1. Velopack's startup ([`updates::startup`]), before anything else;
//! 2. the GPU's and the shell's environment, while no other thread runs;
//! 3. the single instance ([`single`]): when another pet runs, .NET or Rust, this one quits quietly;
//! 4. the update checks ([`updates::start`]);
//! 5. the core ([`core_thread`]), with `config.json` read ([`config`]) and the platform's services;
//! 6. the shell, until the pet quits; then the core stops.
//!
//! The app's side of the pet ([`App`], the pet's [`Host`]) keeps `config.json`, tells the core of dismissed bubbles and
//! of music, places the window ([`placement`]) and does Settings' actions.
//!
//! `AIPET_BACKEND=wayland|desktop` chooses the shell; otherwise the Wayland shell runs when the session is Wayland and
//! its compositor has layer-shell, and the desktop shell everywhere else (through XWayland in a Wayland session).
//! `AIPET_DEBUG=1` says which shell runs and makes it log pointer events and input-region updates to stderr;
//! `AIPET_OPEN_SETTINGS=1` opens Settings at start (both shells). The pet renders on the low-power GPU unless
//! `WGPU_POWER_PREF` says otherwise, and through GL unless `WGPU_BACKEND` does.
//!
//! With the `demo` feature it plays the demo's scripted day instead, for development and screenshots: no single
//! instance, no core, and nothing read or written in the data folder.

// a GUI program in a release (Velopack's main exe, started from the Start menu): no console window
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(feature = "demo"))]
mod config;
#[cfg(not(feature = "demo"))]
mod core_thread;
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

fn main() -> ExitCode {
    updates::startup();
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
        // SAFETY: nothing else runs yet: no other thread reads or writes the environment (the probe starts none)
        unsafe {
            env::set_var("WAYLAND_DISPLAY", "");
            env::remove_var("WAYLAND_SOCKET");
        }
    }
    let result = pet(wayland);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aipet: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The GPU the pet renders on, unless the user said: the low-power one, through GL.
fn set_gpu_defaults() {
    if env::var_os("WGPU_POWER_PREF").is_none() {
        // a desktop pet has no business waking a discrete GPU (iced asks wgpu for the fastest one). It also dodges
        // an NVIDIA quirk: its Wayland surfaces offer an fp16 format first, which iced (with web colours) picks and
        // then fills with sRGB values the compositor takes for linear ones, washing every colour out.
        // SAFETY: nothing else runs yet: no other thread reads or writes the environment
        unsafe { env::set_var("WGPU_POWER_PREF", "low") };
    }
    if (cfg!(target_os = "linux") || cfg!(windows)) && env::var_os("WGPU_BACKEND").is_none() {
        // GL only, in either shell. On Linux, to list Vulkan adapters the loader opens every driver, and NVIDIA's
        // keeps a hybrid laptop's discrete GPU awake the whole time the pet runs, though the pet renders on the
        // integrated one; through EGL the pet renders with the driver of the GPU the display server uses, on a
        // transparent surface (a layer surface, or XWayland's 32-bit window), and shows at once instead of 2 s
        // later. On Windows GL is transparent too, shows the pet in 0.9 s where Vulkan takes 4.5 to 7.3 s, doesn't
        // load the NVIDIA driver, and never falls back to DX12, whose window iced 0.14 can't make transparent
        // (rust/proofs/windows.md).
        // SAFETY: nothing else runs yet: no other thread reads or writes the environment
        unsafe { env::set_var("WGPU_BACKEND", "gl") };
    }
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

/// The pet: the single instance, the update checks, the core, and the shell until the pet quits.
#[cfg(not(feature = "demo"))]
fn pet(wayland: bool) -> Result<(), String> {
    use std::sync::Arc;

    use aipet_sprite::Avatar;
    use aipet_ui::Prefs;
    use aipet_ui::settings::Services;

    let instance = match single::take(&single::Names::app()) {
        Ok(Some(instance)) => instance,
        // another pet runs: quietly
        Ok(None) => return Ok(()),
        Err(e) => {
            let e = format!("can't tell whether another pet runs, so this one doesn't: {e}");
            aipet_core::log::write(&e);
            return Err(e);
        }
    };
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
    // the core stops (its server and watchers with it) before the single instance is let go
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

#[cfg(not(feature = "demo"))]
impl aipet_ui::Host for App {
    fn prefs(&mut self, prefs: &aipet_ui::Prefs) {
        let written = self.config.change(|config| {
            config.pills = prefs.bubbles;
            config.on_top = prefs.on_top;
            config.music = prefs.music;
            config.avatar = prefs.avatar.clone();
        });
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

#[cfg(test)]
mod tests {
    use super::*;

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

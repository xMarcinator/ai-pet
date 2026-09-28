//! The Velopack proof's stand-in for the Rust pet (`.github/workflows/velopack-proof.yml`, results in
//! `rust/proofs/velopack.md`). Packed as `AiPet.exe` with packId `AiPetApp`, it is the release an installed .NET
//! release updates to. It starts as the pet will, with Velopack first, and notes each run in the data folder:
//! `<data folder>\velopack-proof\<event>.txt`, one line per run. The events:
//! - `start`: an ordinary start, which the stub then ends at once;
//! - `restarted`, `first-run`: what Velopack says about that start (after an update, after an install);
//! - `install`, `updated`, `obsolete`, `uninstall` (Windows only): Velopack's hooks, after which Velopack exits.
//!
//! A note that can't be written is left out, and the workflow then fails on the missing file.

// a GUI exe like the pet, so Velopack starts it as it will start the pet: without a console window
#![windows_subsystem = "windows"]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use velopack::VelopackApp;

fn main() {
    // Velopack first of all, as in the .NET pet (Program.cs): Setup, Update.exe and the uninstaller start the app with
    // --veloapp-* arguments, and run() calls that hook and exits. The pet installs an update downloaded earlier itself,
    // once it knows it is the only pet (Updates.Start), so Velopack's own install on start is off.
    let mut app = VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .on_first_run(|v| note("first-run", &v.to_string()))
        .on_restarted(|v| note("restarted", &v.to_string()));
    #[cfg(windows)]
    {
        app = app
            .on_after_install_fast_callback(|v| note("install", &v.to_string()))
            .on_after_update_fast_callback(|v| note("updated", &v.to_string()))
            .on_before_update_fast_callback(|v| note("obsolete", &v.to_string()))
            // the pet's will run `aipet-hook --uninstall` for the hooks registered with this install (HookCleanup)
            .on_before_uninstall_fast_callback(|v| note("uninstall", &v.to_string()));
    }
    app.run();
    note("start", "-");
}

/// Adds a line to the event's file: when, the version Velopack gave, and which exe ran with which arguments.
fn note(event: &str, version: &str) {
    let dir = aipet_ipc::paths::data_dir().join("velopack-proof");
    let line = format!(
        "{} {event} version={version} pid={} exe={} args={:?}\n",
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        std::process::id(),
        std::env::current_exe().map_or_else(|e| format!("({e})"), |p| p.display().to_string()),
        std::env::args_os().skip(1).collect::<Vec<_>>(),
    );
    let _ = fs::create_dir_all(&dir).and_then(|()| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("{event}.txt")))?
            .write_all(line.as_bytes())
    });
}

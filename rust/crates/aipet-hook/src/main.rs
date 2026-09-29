//! `aipet-hook [--agent claude|codex]`, run by each coding agent on every hook event (`aipet-hook.exe` on Windows).
//!
//! It reads the event from stdin and hands it to the running pet over the socket `aipet-ipc` describes. When the pet
//! isn't running it does nothing at all: it writes nothing anywhere and starts nothing. It only observes: it never
//! prints anything back to the agent, and it always exits 0, so it can't disturb the agent (see [`event`]).
//!
//! Also `aipet-hook --install|--uninstall claude|codex`, `aipet-hook --doctor claude|codex [--probe]` and
//! `aipet-hook --print-plugin-hooks claude|codex`. A port of `src/AiPet.Hook`; those modes are still to be ported.

mod claude;
mod codex;
mod doctor;
mod event;
mod install;
mod json;
mod json_out;
mod plugin_hooks;
mod toml_text;
mod trace;

use std::ffi::OsString;
use std::panic::{AssertUnwindSafe, catch_unwind};

// A panic must unwind, so the event path can catch it and still exit 0.
#[cfg(panic = "abort")]
compile_error!("aipet-hook must be built with panic = \"unwind\": its event path catches every panic");

fn main() {
    // `at`, before anything else and before stdin is read
    let launched = event::Launched::now();
    silently(|| {
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        event::run(&launched, &args);
    })
}

/// Runs the event path so that nothing gets out of it but exit code 0: a panic anywhere (on any thread) prints
/// nothing, since the panic hook is silent, and ends only the part it happened in.
fn silently(body: impl FnOnce()) -> ! {
    std::panic::set_hook(Box::new(|_| {}));
    let _ = catch_unwind(AssertUnwindSafe(body));
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// Set for the child test below, which is a no-op otherwise.
    const CHILD: &str = "AIPET_HOOK_TEST_PANIC";
    /// What the child writes before the panic, so what the test harness writes before it can be told apart.
    const MARK: &str = "--- the wrapped body starts ---";

    #[test]
    fn panicking_child() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        // with --nocapture these go to the process's own stdout and stderr, as the panic's message would
        println!("{MARK}");
        eprintln!("{MARK}");
        std::io::stdout().flush().unwrap();
        super::silently(|| {
            // a panic on another thread too, which the hook's stdin reader could have
            let _ = std::thread::spawn(|| panic!("a panic on a thread")).join();
            panic!("a panic in the hook");
        });
    }

    /// A panic in the wrapped body exits 0 and prints nothing, on stdout or stderr.
    #[test]
    fn a_panic_exits_0_and_prints_nothing() {
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["tests::panicking_child", "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let after_mark = |out: &[u8]| {
            let out = String::from_utf8_lossy(out);
            out.split_once(MARK)
                .map(|(_, rest)| rest.trim_start_matches(['\r', '\n']).to_owned())
        };
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(after_mark(&out.stdout).as_deref(), Some(""), "stdout: {out:?}");
        assert_eq!(after_mark(&out.stderr).as_deref(), Some(""), "stderr: {out:?}");
    }
}

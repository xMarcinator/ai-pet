//! The platforms' logic over fakes of what it calls: the Linux tools' command lines and what they print, and the
//! Windows calls on a made-up desktop. Both run on every OS. Nothing here starts xdotool, xdg-open or a browser, or
//! touches a real window or media key: those are the hands-on passes' (task 22 on Linux, task 27 on Windows). The one
//! real process is this test binary again, as a tool that prints or hangs.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::{Duration, Instant};

use aipet_desktop::platform::linux::{self, Linux, Tools};
use aipet_desktop::platform::windows::{AppCommand, Hwnd, Win32, Windows};
use aipet_ui::platform::{Media, Platform};

/// The fakes' clock, unless a test moves it.
const NOW: f64 = 1_727_780_000.5;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The next of a list of answers: each in turn, then the last again and again.
fn next<T: Clone>(answers: &mut Vec<T>) -> Option<T> {
    if answers.len() > 1 {
        Some(answers.remove(0))
    } else {
        answers.first().cloned()
    }
}

fn lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|&line| line.to_owned()).collect()
}

fn media(name: &str, song: Option<&str>, artist: &str, playing: bool, since: f64) -> Media {
    Media {
        name: name.into(),
        song: song.map(Into::into),
        artist: artist.into(),
        playing,
        track_since: since,
    }
}

// ------------------------------------------------------------------------------------------------------- Linux

/// The Linux tools as a test has them: which are installed, and what each command line answers. It notes every
/// command, start, wait and log line, in order.
struct Script {
    installed: Vec<&'static str>,
    /// Each command line's answers (exit code and output), in turn; a line without any fails, as a missing tool does.
    answers: Mutex<HashMap<String, Vec<(i32, String)>>>,
    seen: Mutex<Vec<String>>,
    /// Told of each command, for the media keys, which run theirs on a thread of their own.
    ran: Mutex<mpsc::Sender<()>>,
    heard: Mutex<mpsc::Receiver<()>>,
}

impl Script {
    fn new(installed: &[&'static str]) -> Script {
        let (ran, heard) = mpsc::channel();
        Script {
            installed: installed.to_vec(),
            answers: Mutex::default(),
            seen: Mutex::default(),
            ran: Mutex::new(ran),
            heard: Mutex::new(heard),
        }
    }

    /// With a command line's answers, in turn.
    fn answer(self, line: &str, answers: &[(i32, &str)]) -> Script {
        let answers = answers.iter().map(|&(code, out)| (code, out.to_owned())).collect();
        lock(&self.answers).insert(line.to_owned(), answers);
        self
    }

    fn seen(&self) -> Vec<String> {
        lock(&self.seen).clone()
    }

    /// Waits for `count` more commands to have run.
    fn wait_for(&self, count: usize) {
        let heard = lock(&self.heard);
        for _ in 0..count {
            heard
                .recv_timeout(Duration::from_secs(10))
                .expect("the command in time");
        }
    }

    fn note(&self, line: String) {
        lock(&self.seen).push(line);
    }
}

impl Tools for Script {
    fn has(&self, tool: &str) -> bool {
        self.installed.contains(&tool)
    }

    fn capture(&self, tool: &str, args: &[&str]) -> (i32, String) {
        let line = std::iter::once(tool)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.note(line.clone());
        let answer = lock(&self.answers).get_mut(&line).and_then(next);
        let _ = lock(&self.ran).send(());
        answer.unwrap_or((-1, String::new()))
    }

    fn start(&self, tool: &str, arg: &OsStr) {
        self.note(format!("start {tool} {}", arg.to_string_lossy()));
    }

    fn sleep(&self, time: Duration) {
        self.note(format!("sleep {}ms", time.as_millis()));
    }

    fn now(&self) -> f64 {
        NOW
    }

    fn log(&self, line: &str) {
        self.note(format!("log {line}"));
    }
}

fn linux(script: Script) -> (Linux, Arc<Script>) {
    let script = Arc::new(script);
    (Linux::with(Arc::clone(&script) as Arc<dyn Tools>), script)
}

#[test]
fn xdotool_brings_an_agents_window_forward_and_the_pet_waits_until_it_is_active() {
    const SEARCH: &str = "xdotool search --onlyvisible --class claude";
    const ACTIVE: &str = "xdotool getactivewindow";
    // the first window it lists comes, on the third look: a look that fails counts for nothing
    let (platform, script) = linux(
        Script::new(&["xdotool", "wmctrl"])
            .answer(SEARCH, &[(0, "\n  60817409 \n60817411\n")])
            .answer(ACTIVE, &[(0, "52428803\n"), (1, "60817409\n"), (0, "60817409\n")]),
    );
    platform.focus_agent("claude", Some("claude://claude.ai/chat/0b5f"));
    assert_eq!(
        script.seen(),
        lines(&[
            SEARCH,
            "xdotool windowactivate 60817409",
            ACTIVE,
            "sleep 15ms",
            ACTIVE,
            "sleep 15ms",
            ACTIVE
        ])
    );

    // the window manager never lets it come: 11 looks and 10 waits, then a log line, and no wmctrl or link after it
    let (platform, script) = linux(
        Script::new(&["xdotool", "wmctrl"])
            .answer(SEARCH, &[(0, "60817409\n")])
            .answer(ACTIVE, &[(0, "52428803\n")]),
    );
    platform.focus_agent("claude", Some("claude://claude.ai/chat/0b5f"));
    let mut seen = lines(&[SEARCH, "xdotool windowactivate 60817409"]);
    for _ in 0..10 {
        seen.extend(lines(&[ACTIVE, "sleep 15ms"]));
    }
    seen.extend(lines(&[ACTIVE, "log couldn't bring claude to the front"]));
    assert_eq!(script.seen(), seen);
}

#[test]
fn wmctrl_is_the_fallback_and_the_chats_link_the_last_resort() {
    type Case = (
        &'static str,
        Option<&'static str>,
        &'static [&'static str],
        &'static [(&'static str, i32, &'static str)],
        &'static [&'static str],
    );
    const LINK: &str = "codex://threads/019a2b3c";
    let cases: [Case; 4] = [
        // Codex's windows are ChatGPT's, else its own: xdotool finds neither, wmctrl brings Codex's
        (
            "codex",
            Some(LINK),
            &["xdotool", "wmctrl"],
            &[
                ("xdotool search --onlyvisible --class chatgpt", 1, ""),
                ("xdotool search --onlyvisible --class codex", 0, " \n"),
                ("wmctrl -x -a codex", 0, ""),
            ],
            &[
                "xdotool search --onlyvisible --class chatgpt",
                "wmctrl -x -a chatgpt",
                "xdotool search --onlyvisible --class codex",
                "wmctrl -x -a codex",
            ],
        ),
        // no window: the chat's link
        (
            "claude",
            Some(LINK),
            &["wmctrl"],
            &[("wmctrl -x -a claude", 1, "")],
            &["wmctrl -x -a claude", "start xdg-open codex://threads/019a2b3c"],
        ),
        (
            "codex",
            Some(LINK),
            &[],
            &[],
            &["start xdg-open codex://threads/019a2b3c"],
        ),
        // no tools and no link: nothing at all
        ("claude", None, &[], &[], &[]),
    ];
    for (agent, link, installed, answers, seen) in cases {
        let script = answers
            .iter()
            .fold(Script::new(installed), |script, &(line, code, out)| {
                script.answer(line, &[(code, out)])
            });
        let (platform, script) = linux(script);
        platform.focus_agent(agent, link);
        assert_eq!(script.seen(), lines(seen), "{agent} with {installed:?}");
    }
}

#[test]
fn xdg_open_opens_links_and_folders() {
    let (platform, script) = linux(Script::new(&[]));
    platform.open_url("https://github.com/o/r/pull/7");
    platform.open_folder(Path::new("/home/me/.local/share/AiPet"));
    assert_eq!(
        script.seen(),
        lines(&[
            "start xdg-open https://github.com/o/r/pull/7",
            "start xdg-open /home/me/.local/share/AiPet"
        ])
    );
}

const PLAYERCTL: &str =
    "playerctl --player=spotify,%any metadata --format {{status}}\t{{artist}}\t{{title}}\t{{playerName}}";

#[test]
fn playerctl_says_the_status_artist_title_and_player() {
    // one player, polled once for each of playerctl's answers in turn
    let polls: [((i32, &str), Media); 7] = [
        (
            (0, "Playing\tRick Astley\tNever Gonna Give You Up\tspotify\n"),
            media("Spotify", Some("Never Gonna Give You Up"), "Rick Astley", true, NOW),
        ),
        // a stream without an artist, in another player, which names the player from then on
        (
            (0, "Paused\t\tRadio Paradise\tvlc\n"),
            media("Vlc", Some("Radio Paradise"), "", false, NOW),
        ),
        (
            (0, "Playing\tBand\tSong\t\n"),
            media("Vlc", Some("Song"), "Band", true, NOW),
        ),
        // nothing plays: no player (exit 1), a blank title, too few fields, nothing printed
        ((1, ""), media("Vlc", None, "", false, NOW)),
        ((0, "Playing\tBand\t \tvlc\n"), media("Vlc", None, "", false, NOW)),
        ((0, "Playing\tBand\tSong\n"), media("Vlc", None, "", false, NOW)),
        ((0, "\n"), media("Vlc", None, "", false, NOW)),
    ];
    let answers: Vec<(i32, &str)> = polls.iter().map(|(answer, _)| *answer).collect();
    let (platform, script) = linux(Script::new(&["playerctl"]).answer(PLAYERCTL, &answers));
    let player = platform.media().expect("a player");
    assert_eq!(player.name(), "Spotify");
    for (answer, plays) in polls {
        assert_eq!(player.poll(), plays, "{answer:?}");
    }
    assert_eq!(script.seen(), vec![PLAYERCTL.to_owned(); 7]);
}

const LIST_NAMES: &str = "dbus-send --session --dest=org.freedesktop.DBus --type=method_call --print-reply \
                          /org/freedesktop/DBus org.freedesktop.DBus.ListNames";

/// ListNames as `dbus-send --print-reply` prints it: VLC's player before Spotify's.
const NAMES: &str = r#"method return time=1727780000.123456 sender=org.freedesktop.DBus -> destination=:1.234 serial=3 reply_serial=2
   array [
      string "org.freedesktop.DBus"
      string ":1.7"
      string "org.mpris.MediaPlayer2.vlc"
      string "org.freedesktop.Notifications"
      string "org.mpris.MediaPlayer2.spotify"
   ]
"#;

/// Spotify's Metadata as `dbus-send --print-reply` prints it, with an album artist that isn't the artist.
const METADATA: &str = r#"method return time=1727780000.234567 sender=:1.42 -> destination=:1.235 serial=1234 reply_serial=2
   variant       array [
         dict entry(
            string "mpris:trackid"
            variant                object path "/com/spotify/track/4uLU6hMCjMI75M1A2tKUQC"
         )
         dict entry(
            string "xesam:albumArtist"
            variant                array [
                  string "Various Artists"
               ]
         )
         dict entry(
            string "xesam:artist"
            variant                array [
                  string "Rick Astley"
               ]
         )
         dict entry(
            string "xesam:title"
            variant                string "Never Gonna Give You Up"
         )
      ]
"#;

/// The dbus-send command line that asks a player for one of its properties.
fn get(bus: &str, property: &str) -> String {
    format!(
        "dbus-send --session --print-reply --dest={bus} /org/mpris/MediaPlayer2 org.freedesktop.DBus.Properties.Get \
         string:org.mpris.MediaPlayer2.Player string:{property}"
    )
}

/// PlaybackStatus as `dbus-send --print-reply` prints it.
fn status(status: &str) -> String {
    format!(
        "method return time=1727780000.345678 sender=:1.42 -> destination=:1.236 serial=7 reply_serial=2\n   \
         variant       string \"{status}\"\n"
    )
}

#[test]
fn dbus_send_finds_spotify_first_and_says_its_track_without_playerctl() {
    const SPOTIFY: &str = "org.mpris.MediaPlayer2.spotify";
    let (metadata, playback) = (get(SPOTIFY, "Metadata"), get(SPOTIFY, "PlaybackStatus"));
    let (platform, script) = linux(
        Script::new(&[])
            .answer(LIST_NAMES, &[(0, NAMES)])
            .answer(&metadata, &[(0, METADATA)])
            .answer(&playback, &[(0, &status("Paused"))]),
    );
    assert_eq!(
        platform.media().expect("a player").poll(),
        media("Spotify", Some("Never Gonna Give You Up"), "Rick Astley", false, NOW)
    );
    assert_eq!(script.seen(), [LIST_NAMES.to_owned(), metadata, playback]);

    // without Spotify, the first player; without a title nothing plays, and its status isn't asked
    const VLC: &str = "org.mpris.MediaPlayer2.vlc";
    let (platform, script) = linux(
        Script::new(&[])
            .answer(
                LIST_NAMES,
                &[(0, &NAMES.replace(SPOTIFY, "org.freedesktop.portal.Desktop"))],
            )
            .answer(
                &get(VLC, "Metadata"),
                &[(0, &METADATA.replace("xesam:title", "xesam:album"))],
            ),
    );
    assert_eq!(
        platform.media().expect("a player").poll(),
        media("Spotify", None, "", false, 0.0)
    );
    assert_eq!(script.seen(), [LIST_NAMES.to_owned(), get(VLC, "Metadata")]);

    // no player on the bus, or no bus
    for (code, names) in [(0, NAMES.replace("org.mpris.", "org.example.")), (1, String::new())] {
        let (platform, script) = linux(Script::new(&[]).answer(LIST_NAMES, &[(code, &names)]));
        assert_eq!(
            platform.media().expect("a player").poll(),
            media("Spotify", None, "", false, 0.0)
        );
        assert_eq!(script.seen(), [LIST_NAMES]);
    }
}

#[test]
fn dbus_send_replies_are_read_as_the_csharps_patterns_read_them() {
    // a quoted name with nothing after the prefix isn't a player's; the name its closing quote opens is
    let names = "   array [\n      string \"org.mpris.MediaPlayer2.\"org.mpris.MediaPlayer2.mpv\"\n   ]\n";
    // the first string after the key, escapes as printed; one broken by a backslash at the end of its line is skipped
    let metadata = concat!(
        "string \"xesam:artist\"\n variant array [\n string \"AC\\\\DC \\\"live\\\n string \"Band\"\n ]\n",
        "string \"xesam:title\"\n variant string \"Say \\\"Hi\\\" \u{1F600}\"\n",
    );
    const MPV: &str = "org.mpris.MediaPlayer2.mpv";
    let (platform, _) = linux(
        Script::new(&[])
            .answer(LIST_NAMES, &[(0, names)])
            .answer(&get(MPV, "Metadata"), &[(0, metadata)])
            .answer(&get(MPV, "PlaybackStatus"), &[(0, &status("Playing"))]),
    );
    assert_eq!(
        platform.media().expect("a player").poll(),
        media("Mpv", Some("Say \\\"Hi\\\" \u{1F600}"), "Band", true, NOW)
    );
}

#[test]
fn media_keys_go_to_playerctl_else_to_the_players_bus() {
    let (platform, script) = linux(Script::new(&["playerctl", "wmctrl"]));
    let player = platform.media().expect("a player");
    player.play_pause();
    script.wait_for(1);
    player.next();
    script.wait_for(1);
    player.previous();
    script.wait_for(1);
    player.focus();
    assert_eq!(
        script.seen(),
        lines(&[
            "playerctl --player=spotify,%any play-pause",
            "playerctl --player=spotify,%any next",
            "playerctl --player=spotify,%any previous",
            "wmctrl -x -a spotify",
        ])
    );

    // without playerctl, the player's own method on its bus; without wmctrl, nothing comes forward
    let (platform, script) = linux(Script::new(&[]).answer(LIST_NAMES, &[(0, NAMES)]));
    let player = platform.media().expect("a player");
    player.play_pause();
    script.wait_for(2);
    player.next();
    script.wait_for(2);
    player.previous();
    script.wait_for(2);
    player.focus();
    let seen: Vec<String> = ["PlayPause", "Next", "Previous"]
        .into_iter()
        .flat_map(|method| {
            [
                LIST_NAMES.to_owned(),
                format!(
                    "dbus-send --session --type=method_call --dest=org.mpris.MediaPlayer2.spotify \
                     /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player.{method}"
                ),
            ]
        })
        .collect();
    assert_eq!(script.seen(), seen);
}

/// This test binary again, as a tool: it runs only the ignored [`tool`] test, which `AIPET_TEST_TOOL` tells what to
/// do.
fn as_tool(what: &str) -> Command {
    let mut tool = Command::new(std::env::current_exe().expect("this test binary"));
    tool.args(["--exact", "tool", "--ignored", "--nocapture"])
        .env("AIPET_TEST_TOOL", what);
    tool
}

#[test]
fn a_tool_gives_its_exit_code_and_output_and_is_stopped_past_its_time() {
    let (code, out) = linux::capture(&mut as_tool("print"), Duration::from_secs(60));
    // libtest's own lines come first
    assert_eq!(code, 3, "{out:?}");
    assert!(
        out.contains("Playing\tRick Astley\tNever Gonna Give You Up\tspotify\n"),
        "{out:?}"
    );

    let started = Instant::now();
    assert_eq!(
        linux::capture(&mut as_tool("hang"), Duration::from_millis(500)),
        (-1, String::new())
    );
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "stopped after {:?}",
        started.elapsed()
    );

    let missing = linux::capture(&mut Command::new("aipet-test-no-such-tool"), Duration::from_secs(10));
    assert_eq!(missing, (-1, String::new()));
}

/// The tool [`a_tool_gives_its_exit_code_and_output_and_is_stopped_past_its_time`] runs: it prints a line of
/// playerctl's and exits 3, or hangs for 30 s. Without `AIPET_TEST_TOOL` it does nothing.
#[test]
#[ignore = "a tool that a_tool_gives_its_exit_code_and_output_and_is_stopped_past_its_time runs"]
fn tool() {
    match std::env::var("AIPET_TEST_TOOL").as_deref() {
        Ok("print") => {
            println!("Playing\tRick Astley\tNever Gonna Give You Up\tspotify");
            std::process::exit(3);
        }
        Ok("hang") => std::thread::sleep(Duration::from_secs(30)),
        _ => {}
    }
}

// ----------------------------------------------------------------------------------------------------- Windows

/// A window on the made-up desktop. Its thread's id is its own.
#[derive(Clone, Default)]
struct Window {
    id: Hwnd,
    process: u32,
    title: &'static str,
    class: &'static str,
    hidden: bool,
    owned: bool,
    tool: bool,
    minimized: bool,
}

/// A main window of a process, with a title (empty for none).
fn window(id: Hwnd, process: u32, title: &'static str) -> Window {
    Window {
        id,
        process,
        title,
        class: "Chrome_WidgetWin_1",
        ..Window::default()
    }
}

/// A desktop as a test has it: its windows front to back, each process's exe, and what GetForegroundWindow and
/// SetForegroundWindow answer, in turn. It notes every call that changes something, each wait and log line, in
/// order, and each process whose exe was looked up.
struct Desktop {
    windows: Mutex<Vec<Window>>,
    exes: HashMap<u32, &'static str>,
    foreground: Mutex<Vec<Hwnd>>,
    set_foreground: Mutex<Vec<bool>>,
    now: Mutex<f64>,
    seen: Mutex<Vec<String>>,
    looked_up: Mutex<Vec<u32>>,
}

impl Desktop {
    /// No window in front, and SetForegroundWindow always works.
    fn new(windows: &[Window], exes: &[(u32, &'static str)]) -> Desktop {
        Desktop {
            windows: Mutex::new(windows.to_vec()),
            exes: exes.iter().copied().collect(),
            foreground: Mutex::new(vec![0]),
            set_foreground: Mutex::new(vec![true]),
            now: Mutex::new(NOW),
            seen: Mutex::default(),
            looked_up: Mutex::default(),
        }
    }

    /// With GetForegroundWindow's answers, in turn.
    fn in_front(self, windows: &[Hwnd]) -> Desktop {
        *lock(&self.foreground) = windows.to_vec();
        self
    }

    /// With SetForegroundWindow's answers, in turn.
    fn bringing(self, answers: &[bool]) -> Desktop {
        *lock(&self.set_foreground) = answers.to_vec();
        self
    }

    fn window(&self, id: Hwnd) -> Option<Window> {
        lock(&self.windows).iter().find(|window| window.id == id).cloned()
    }

    fn retitle(&self, id: Hwnd, title: &'static str) {
        if let Some(window) = lock(&self.windows).iter_mut().find(|window| window.id == id) {
            window.title = title;
        }
    }

    fn seen(&self) -> Vec<String> {
        lock(&self.seen).clone()
    }

    fn note(&self, line: String) {
        lock(&self.seen).push(line);
    }
}

impl Win32 for Desktop {
    fn windows(&self) -> Vec<Hwnd> {
        lock(&self.windows).iter().map(|window| window.id).collect()
    }

    fn visible(&self, window: Hwnd) -> bool {
        self.window(window).is_some_and(|window| !window.hidden)
    }

    fn owned(&self, window: Hwnd) -> bool {
        self.window(window).is_some_and(|window| window.owned)
    }

    fn tool_window(&self, window: Hwnd) -> bool {
        self.window(window).is_some_and(|window| window.tool)
    }

    fn title(&self, window: Hwnd) -> String {
        self.window(window)
            .map_or_else(String::new, |window| window.title.to_owned())
    }

    fn class(&self, window: Hwnd) -> String {
        self.window(window)
            .map_or_else(String::new, |window| window.class.to_owned())
    }

    fn thread_process(&self, window: Hwnd) -> (u32, u32) {
        self.window(window)
            .map_or((0, 0), |window| (window.id as u32, window.process))
    }

    fn exe(&self, process: u32) -> Option<String> {
        lock(&self.looked_up).push(process);
        self.exes.get(&process).map(|exe| (*exe).to_owned())
    }

    fn foreground(&self) -> Hwnd {
        next(&mut lock(&self.foreground)).unwrap_or(0)
    }

    fn minimized(&self, window: Hwnd) -> bool {
        self.window(window).is_some_and(|window| window.minimized)
    }

    fn restore(&self, window: Hwnd) {
        self.note(format!("ShowWindow {window} SW_RESTORE"));
    }

    fn set_foreground(&self, window: Hwnd) -> bool {
        self.note(format!("SetForegroundWindow {window}"));
        next(&mut lock(&self.set_foreground)).unwrap_or(true)
    }

    fn bring_to_top(&self, window: Hwnd) {
        self.note(format!("BringWindowToTop {window}"));
    }

    fn current_thread(&self) -> u32 {
        1
    }

    fn attach_input(&self, thread: u32, to: u32, attach: bool) {
        self.note(format!("AttachThreadInput {thread} {to} {attach}"));
    }

    fn app_command(&self, window: Hwnd, command: AppCommand) {
        self.note(format!("WM_APPCOMMAND {window} {command:?}"));
    }

    fn shell_open(&self, target: &OsStr) {
        self.note(format!("ShellExecute {}", target.to_string_lossy()));
    }

    fn sleep(&self, time: Duration) {
        self.note(format!("sleep {}ms", time.as_millis()));
    }

    fn now(&self) -> f64 {
        *lock(&self.now)
    }

    fn log(&self, line: &str) {
        self.note(format!("log {line}"));
    }
}

fn windows(desktop: Desktop) -> (Windows, Arc<Desktop>) {
    let desktop = Arc::new(desktop);
    (Windows::with(Arc::clone(&desktop) as Arc<dyn Win32>), desktop)
}

/// The agents' apps, behind windows of theirs that aren't main windows, and others'.
fn agents() -> Vec<Window> {
    vec![
        Window {
            hidden: true,
            ..window(101, 10, "Claude")
        },
        Window {
            owned: true,
            ..window(102, 10, "Menu")
        },
        Window {
            tool: true,
            ..window(103, 10, "Quick entry")
        },
        window(104, 10, ""),
        window(105, 11, "Claude - Notepad"),
        // its exe can't be read
        window(106, 12, "Claude"),
        window(107, 13, "Claude"),
        window(108, 14, "ChatGPT"),
        window(109, 15, "Codex"),
    ]
}

const EXES: [(u32, &str); 5] = [
    (10, "claude.exe"),
    (11, "notepad.exe"),
    (13, "Claude.exe"),
    (14, "ChatGPT.exe"),
    (15, "codex.exe"),
];

#[test]
fn an_agents_app_is_the_first_main_window_of_its_exe_with_a_title() {
    let without_chatgpt: Vec<Window> = agents().into_iter().filter(|window| window.id != 108).collect();
    for (agent, desktop, comes) in [
        ("claude", agents(), 107),
        ("codex", agents(), 108),
        ("codex", without_chatgpt, 109),
    ] {
        let (platform, desktop) = windows(Desktop::new(&desktop, &EXES).in_front(&[comes]));
        platform.focus_agent(agent, Some("claude://claude.ai/chat/0b5f"));
        assert_eq!(desktop.seen(), [format!("SetForegroundWindow {comes}")], "{agent}");
    }
}

#[test]
fn the_pet_waits_for_the_window_it_brings_and_logs_one_that_never_comes() {
    let editor = window(900, 90, "Editor");
    // minimised: restored first; in front on the third look
    let claude = Window {
        minimized: true,
        ..window(107, 13, "Claude")
    };
    let desktop = Desktop::new(&[claude, editor.clone()], &EXES).in_front(&[900, 900, 107]);
    let (platform, desktop) = windows(desktop);
    platform.focus_agent("claude", None);
    assert_eq!(
        desktop.seen(),
        lines(&[
            "ShowWindow 107 SW_RESTORE",
            "SetForegroundWindow 107",
            "sleep 15ms",
            "sleep 15ms"
        ])
    );

    // Windows refuses: tried again through the input of the foreground window's thread, and still never in front
    let desktop = Desktop::new(&[window(107, 13, "Claude"), editor], &EXES)
        .in_front(&[900])
        .bringing(&[false]);
    let (platform, desktop) = windows(desktop);
    platform.focus_agent("claude", None);
    let mut seen = lines(&[
        "SetForegroundWindow 107",
        "AttachThreadInput 1 900 true",
        "BringWindowToTop 107",
        "SetForegroundWindow 107",
        "AttachThreadInput 1 900 false",
    ]);
    seen.extend(vec!["sleep 15ms".to_owned(); 10]);
    seen.push("log couldn't bring claude to the front".to_owned());
    assert_eq!(desktop.seen(), seen);
}

#[test]
fn without_its_window_an_agents_chat_link_opens_else_the_apps_own() {
    let cases: [(&str, Option<&str>, &[&str]); 4] = [
        (
            "claude",
            None,
            &[
                "ShellExecute claude://",
                "log no claude window found; opening claude://",
            ],
        ),
        (
            "claude",
            Some("claude://claude.ai/chat/0b5f"),
            &[
                "ShellExecute claude://claude.ai/chat/0b5f",
                "log no claude window found; opening the chat's link",
            ],
        ),
        // nothing opens ChatGPT
        ("codex", None, &["log no codex window found"]),
        (
            "codex",
            Some("codex://threads/019a2b3c"),
            &[
                "ShellExecute codex://threads/019a2b3c",
                "log no codex window found; opening the chat's link",
            ],
        ),
    ];
    for (agent, link, seen) in cases {
        let (platform, desktop) = windows(Desktop::new(&[window(900, 90, "Editor")], &[(90, "code.exe")]));
        platform.focus_agent(agent, link);
        assert_eq!(desktop.seen(), lines(seen), "{agent} {link:?}");
    }
}

#[test]
fn the_shell_opens_links_and_folders() {
    let (platform, desktop) = windows(Desktop::new(&[], &[]));
    platform.open_url("https://github.com/o/r/pull/7");
    platform.open_folder(Path::new(r"C:\Users\me\AppData\Local\AiPet"));
    assert_eq!(
        desktop.seen(),
        lines(&[
            "ShellExecute https://github.com/o/r/pull/7",
            r"ShellExecute C:\Users\me\AppData\Local\AiPet"
        ])
    );
}

#[test]
fn spotifys_title_says_what_plays() {
    let (platform, desktop) = windows(Desktop::new(&[window(204, 31, "")], &[(31, "Spotify.exe")]));
    let player = platform.media().expect("a player");
    assert_eq!(player.name(), "Spotify");
    let song = |song, artist, playing, since| media("Spotify", song, artist, playing, since);
    let rick = Some("Never Gonna Give You Up");
    // (when, Spotify's title, what plays); an empty title is no window
    let polls = [
        (
            100.0,
            "Rick Astley - Never Gonna Give You Up",
            song(rick, "Rick Astley", true, 100.0),
        ),
        // paused: the track stays
        (101.0, "Spotify Premium", song(rick, "Rick Astley", false, 100.0)),
        (
            102.0,
            "Rick Astley - Never Gonna Give You Up",
            song(rick, "Rick Astley", true, 100.0),
        ),
        (103.0, "Advertisement", song(rick, "Rick Astley", false, 100.0)),
        (104.0, "  ", song(rick, "Rick Astley", false, 100.0)),
        // the first " - " splits; one at the very start, or none, leaves it all song
        (
            105.0,
            "Daft Punk - Harder - Better",
            song(Some("Harder - Better"), "Daft Punk", true, 105.0),
        ),
        (106.0, " - Lead", song(Some("- Lead"), "", true, 106.0)),
        (107.0, "  Intro  ", song(Some("Intro"), "", true, 107.0)),
        // Spotify closed: nothing plays, and its paused title has no track to keep
        (108.0, "", song(None, "", false, 107.0)),
        (109.0, "spotify free", song(None, "", false, 107.0)),
        (
            110.0,
            "Rick Astley - Never Gonna Give You Up",
            song(rick, "Rick Astley", true, 110.0),
        ),
    ];
    for (now, title, plays) in polls {
        *lock(&desktop.now) = now;
        desktop.retitle(204, title);
        assert_eq!(player.poll(), plays, "{title:?}");
    }
}

#[test]
fn spotifys_window_is_its_first_chrome_window_with_a_title_and_takes_the_media_keys() {
    let desktop = Desktop::new(
        &[
            window(201, 20, "Artist - A Song In A Browser"),
            Window {
                class: "GDI+ Hook Window Class",
                ..window(202, 30, "GDI+ Window (Spotify.exe)")
            },
            window(203, 30, ""),
            window(204, 31, "Artist - Song"),
            window(205, 31, "Other - Later"),
        ],
        &[(20, "chrome.exe"), (30, "Spotify.exe"), (31, "SPOTIFY.EXE")],
    );
    let (platform, desktop) = windows(desktop);
    let player = platform.media().expect("a player");
    // a key before any poll finds the window itself
    player.play_pause();
    assert_eq!(player.poll(), media("Spotify", Some("Song"), "Artist", true, NOW));
    player.next();
    player.previous();
    player.focus();
    assert_eq!(
        desktop.seen(),
        lines(&[
            "WM_APPCOMMAND 204 PlayPause",
            "WM_APPCOMMAND 204 Next",
            "WM_APPCOMMAND 204 Previous",
            "SetForegroundWindow 204",
        ])
    );
    // each process's exe is looked up once in 10 s
    assert_eq!(*lock(&desktop.looked_up), [20, 30, 31]);
    *lock(&desktop.now) = NOW + 10.5;
    player.poll();
    assert_eq!(*lock(&desktop.looked_up), [20, 30, 31, 20, 30, 31]);

    // Spotify gone: the keys go nowhere, and nothing comes forward
    desktop.retitle(204, "");
    desktop.retitle(205, "");
    assert_eq!(player.poll(), media("Spotify", None, "", false, NOW));
    player.play_pause();
    player.focus();
    assert_eq!(desktop.seen().len(), 4);
}

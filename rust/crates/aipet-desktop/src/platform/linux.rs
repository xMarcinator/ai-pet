//! The Linux platform (`src/AiPet.UI/Platform/LinuxPlatform.cs`), for X11 and Wayland sessions alike, through the
//! desktop's own tools when they are installed: xdotool, else wmctrl, to bring an agent's window forward; xdg-open
//! for links and folders; and MPRIS for the player, through playerctl, else dbus-send. Native Wayland windows can't
//! be found or brought forward this way: for them nothing comes forward, and at most the chat's link opens.
//!
//! Everything it runs goes through [`Tools`] (the C#'s `Sh`). Nothing here is Linux-only Rust, so it compiles, and its
//! tests run, everywhere; `platform::new` picks it on Linux.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use aipet_ui::platform::{Media, MediaPlayer, Platform};

use super::{Track, lock};

/// How long a tool gets (`Sh.Capture`'s timeout).
const TIMEOUT: Duration = Duration::from_secs(3);

/// The player playerctl asks: Spotify when it runs, else any.
const ANY_PLAYER: &str = "--player=spotify,%any";

/// What the Linux platform reaches outside itself: the tools it runs (the C#'s `Sh`), its waits, the clock and
/// aipet.log. [`Linux::new`] has the real ones; tests answer for them.
pub trait Tools: Send + Sync {
    /// Whether the tool is on `PATH` (`Sh.Which(tool) != null`).
    fn has(&self, tool: &str) -> bool;

    /// Runs a tool, and gives its exit code and what it printed (`Sh.Capture`, and `Sh.Run` where the output is
    /// left): -1 and nothing when it couldn't start, or ran past 3 s and was stopped.
    fn capture(&self, tool: &str, args: &[&str]) -> (i32, String);

    /// Starts a tool and leaves it running (`Sh.Start`).
    fn start(&self, tool: &str, arg: &OsStr);

    fn sleep(&self, time: Duration);

    /// Now, in Unix seconds.
    fn now(&self) -> f64;

    /// Writes a line to aipet.log.
    fn log(&self, line: &str);
}

/// The Linux platform, over its [`Tools`].
pub struct Linux {
    tools: Arc<dyn Tools>,
    mpris: Mpris,
}

impl Linux {
    /// With the desktop's own tools.
    pub fn new() -> Linux {
        Linux::with(Arc::new(Processes::default()))
    }

    /// With these tools (a test's).
    pub fn with(tools: Arc<dyn Tools>) -> Linux {
        Linux {
            mpris: Mpris {
                tools: Arc::clone(&tools),
                heard: Mutex::new(Heard {
                    player: "Spotify".into(),
                    track: Track::default(),
                }),
            },
            tools,
        }
    }

    /// `Active`: the window manager may refuse to activate a window (focus-stealing prevention) while the tools still
    /// exit 0, so a window only counts as in front once xdotool names it the active one.
    fn active(&self, id: &str) -> bool {
        let (code, active) = self.tools.capture("xdotool", &["getactivewindow"]);
        code == 0 && matches!((active.trim().parse::<i64>(), id.parse::<i64>()), (Ok(a), Ok(b)) if a == b)
    }
}

impl Default for Linux {
    fn default() -> Linux {
        Linux::new()
    }
}

impl Platform for Linux {
    fn open_url(&self, url: &str) {
        self.tools.start("xdg-open", OsStr::new(url));
    }

    fn open_folder(&self, path: &Path) {
        self.tools.start("xdg-open", path.as_os_str());
    }

    /// `FocusAgent`, by the window classes of the agents' desktop apps where they exist (e.g. unofficial Claude
    /// builds): only an open window is brought forward. When none is found, the chat's link opens, if there is one;
    /// nothing is started.
    fn focus_agent(&self, agent: &str, link: Option<&str>) {
        let classes: &[&str] = if agent == "codex" {
            &["chatgpt", "codex"]
        } else {
            &["claude"]
        };
        for class in classes {
            if self.tools.has("xdotool") {
                // visible windows only: Electron apps have hidden helper windows of the same class
                let (code, ids) = self
                    .tools
                    .capture("xdotool", &["search", "--onlyvisible", "--class", class]);
                if code == 0
                    && let Some(id) = ids.split('\n').map(str::trim).find(|id| !id.is_empty())
                {
                    self.tools.capture("xdotool", &["windowactivate", id]);
                    let mut tries = 0;
                    while !self.active(id) {
                        if tries == 10 {
                            self.tools.log(&format!("couldn't bring {agent} to the front"));
                            return;
                        }
                        self.tools.sleep(Duration::from_millis(15));
                        tries += 1;
                    }
                    return;
                }
            }
            // wmctrl can bring a window forward (a minimised one too), but can't tell whether it came
            if self.tools.has("wmctrl") && self.tools.capture("wmctrl", &["-x", "-a", class]).0 == 0 {
                return;
            }
        }
        if let Some(link) = link {
            self.open_url(link);
        }
    }

    fn media(&self) -> Option<&dyn MediaPlayer> {
        Some(&self.mpris)
    }
}

/// MPRIS players, Spotify first (`Mpris`).
struct Mpris {
    tools: Arc<dyn Tools>,
    heard: Mutex<Heard>,
}

/// The player last heard, and its track.
struct Heard {
    /// Its name, as the C#'s `playerName`: Spotify until a poll names one, then the last one named.
    player: String,
    track: Track,
}

impl MediaPlayer for Mpris {
    /// The player as Settings names it when the pet starts: Spotify, the C#'s first `Name`. The one heard since is in
    /// what [`poll`](MediaPlayer::poll) gives.
    fn name(&self) -> &str {
        "Spotify"
    }

    /// `Poll`, waited for: playerctl when it is installed, else dbus-send.
    fn poll(&self) -> Media {
        let said = if self.tools.has("playerctl") {
            read_playerctl(&*self.tools)
        } else {
            read_dbus(&*self.tools)
        };
        let now = self.tools.now();
        let mut heard = lock(&self.heard);
        if let Some(player) = said.player {
            heard.player = player;
        }
        heard.track.set(said.song, said.artist, said.playing, now);
        heard.track.media(&heard.player)
    }

    fn previous(&self) {
        self.command("previous", "Previous");
    }

    fn play_pause(&self) {
        self.command("play-pause", "PlayPause");
    }

    fn next(&self) {
        self.command("next", "Next");
    }

    /// Through wmctrl, by Spotify's window class; nothing without wmctrl.
    fn focus(&self) {
        if self.tools.has("wmctrl") {
            self.tools.capture("wmctrl", &["-x", "-a", "spotify"]);
        }
    }
}

impl Mpris {
    /// `Command`: a media key for the player, through playerctl, else the player's own method over dbus-send. It runs
    /// on a thread of its own, as the C#'s on the thread pool, so a slow player never holds up the pet. The C# asks
    /// the player again right after; here the core's next poll, within a second, does.
    fn command(&self, playerctl: &'static str, method: &'static str) {
        let tools = Arc::clone(&self.tools);
        let _ = thread::Builder::new().name("aipet-media-key".into()).spawn(move || {
            if tools.has("playerctl") {
                tools.capture("playerctl", &[ANY_PLAYER, playerctl]);
            } else if let Some(bus) = bus(&*tools) {
                let (dest, method) = (
                    format!("--dest={bus}"),
                    format!("org.mpris.MediaPlayer2.Player.{method}"),
                );
                tools.capture(
                    "dbus-send",
                    &[
                        "--session",
                        "--type=method_call",
                        &dest,
                        "/org/mpris/MediaPlayer2",
                        &method,
                    ],
                );
            }
        });
    }
}

/// What a player said (the C#'s `(song, artist, playing, player)`): nothing, when nothing plays.
#[derive(Debug, Default)]
struct Said {
    song: Option<String>,
    artist: Option<String>,
    playing: bool,
    player: Option<String>,
}

/// `ReadPlayerctl`: the status, artist, title and player's name, tab-separated.
fn read_playerctl(tools: &dyn Tools) -> Said {
    let format = "{{status}}\t{{artist}}\t{{title}}\t{{playerName}}";
    let (code, out) = tools.capture("playerctl", &[ANY_PLAYER, "metadata", "--format", format]);
    if code != 0 || out.trim().is_empty() {
        return Said::default();
    }
    let fields: Vec<&str> = out.trim_end_matches('\n').split('\t').collect();
    match fields[..] {
        [status, artist, title, player, ..] if !title.trim().is_empty() => Said {
            song: Some(title.to_owned()),
            artist: Some(artist.to_owned()),
            playing: status == "Playing",
            player: (!player.is_empty()).then(|| capitalized(player)),
        },
        _ => Said::default(),
    }
}

/// `ReadDbus`: the player's bus, then its `Metadata` and `PlaybackStatus`.
fn read_dbus(tools: &dyn Tools) -> Said {
    let Some(bus) = bus(tools) else {
        return Said::default();
    };
    let meta = property(tools, &bus, "Metadata");
    let title = string_after(&meta, "xesam:title");
    if title.as_deref().is_none_or(|title| title.trim().is_empty()) {
        return Said::default();
    }
    let status = property(tools, &bus, "PlaybackStatus");
    // the bus name's last part: org.mpris.MediaPlayer2.spotify is Spotify
    let player = bus.rsplit_once('.').map_or(bus.as_str(), |(_, last)| last);
    Said {
        song: title,
        artist: string_after(&meta, "xesam:artist"),
        playing: status.contains("\"Playing\""),
        player: (!player.is_empty()).then(|| capitalized(player)),
    }
}

/// `Bus`: a player on the session bus, from the names `ListNames` gives: Spotify's, else the first.
fn bus(tools: &dyn Tools) -> Option<String> {
    let (code, names) = tools.capture(
        "dbus-send",
        &[
            "--session",
            "--dest=org.freedesktop.DBus",
            "--type=method_call",
            "--print-reply",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.ListNames",
        ],
    );
    if code != 0 {
        return None;
    }
    let players = players(&names);
    let spotify = players
        .iter()
        .find(|name| name.to_ascii_lowercase().contains("spotify"));
    spotify.or(players.first()).map(|name| (*name).to_owned())
}

/// The quoted names that start with `org.mpris.MediaPlayer2.` and have more after it, in order: what the C#'s
/// `"(org\.mpris\.MediaPlayer2\.[^"]+)"` matches.
fn players(reply: &str) -> Vec<&str> {
    const OPEN: &str = "\"org.mpris.MediaPlayer2.";
    let mut players = Vec::new();
    let mut rest = reply;
    while let Some(at) = rest.find(OPEN) {
        let after = at + OPEN.len();
        match rest[after..].find('"') {
            // nothing after the prefix: no match here, but the next quote may start one
            Some(0) => rest = &rest[at + 1..],
            Some(len) => {
                players.push(&rest[at + 1..after + len]);
                rest = &rest[after + len + 1..];
            }
            None => break,
        }
    }
    players
}

/// `Prop`: a property of the player's `org.mpris.MediaPlayer2.Player`, as dbus-send prints it (whatever it printed,
/// failed or not).
fn property(tools: &dyn Tools, bus: &str, name: &str) -> String {
    let (dest, name) = (format!("--dest={bus}"), format!("string:{name}"));
    let args = [
        "--session",
        "--print-reply",
        &dest,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties.Get",
        "string:org.mpris.MediaPlayer2.Player",
        &name,
    ];
    tools.capture("dbus-send", &args).1
}

/// `After`: in a printed `Metadata`, the first `string "…"` after the key (escapes left as printed): what the C#'s
/// `string "((?:[^"\\]|\\.)*)"` matches from the key on. None when the key isn't there or no string follows it.
fn string_after(meta: &str, key: &str) -> Option<String> {
    let mut rest = &meta[meta.find(&format!("\"{key}\""))?..];
    while let Some(at) = rest.find("string \"") {
        let text = &rest[at + "string \"".len()..];
        if let Some(len) = quoted(text) {
            return Some(text[..len].to_owned());
        }
        rest = &rest[at + 1..];
    }
    None
}

/// How far a quoted string's text runs, up to its closing quote: a backslash takes the character after it along
/// (not a line break). None when the text ends first.
fn quoted(text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    loop {
        match chars.next()? {
            (at, '"') => return Some(at),
            (_, '\\') => match chars.next() {
                Some((_, '\n')) | None => return None,
                Some(_) => {}
            },
            _ => {}
        }
    }
}

/// The name with its first letter upper-cased (`char.ToUpperInvariant(name[0]) + name[1..]`): one letter for one,
/// so a letter whose capital is longer (ß) stays.
fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut upper = first.to_uppercase();
    let first = match (upper.next(), upper.next()) {
        (Some(capital), None) => capital,
        _ => first,
    };
    std::iter::once(first).chain(chars).collect()
}

/// The real tools: programs started by name, as the C#'s are.
#[derive(Debug, Default)]
struct Processes {
    /// `Sh.Which`'s answers: each tool is looked for once.
    found: Mutex<HashMap<String, bool>>,
}

impl Tools for Processes {
    fn has(&self, tool: &str) -> bool {
        *lock(&self.found)
            .entry(tool.to_owned())
            .or_insert_with(|| on_path(tool))
    }

    fn capture(&self, tool: &str, args: &[&str]) -> (i32, String) {
        capture(Command::new(tool).args(args), TIMEOUT)
    }

    fn start(&self, tool: &str, arg: &OsStr) {
        super::start(Command::new(tool).arg(arg));
    }

    fn sleep(&self, time: Duration) {
        thread::sleep(time);
    }

    fn now(&self) -> f64 {
        super::unix_now()
    }

    fn log(&self, line: &str) {
        aipet_core::log::write(line);
    }
}

/// `Sh.Which`: whether a file of that name is in a `PATH` folder (an empty entry is the current folder).
fn on_path(tool: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| dir.join(tool).is_file())
}

/// Runs a command and gives its exit code and what it printed, read as UTF-8 (`Sh.Capture`): -1 and nothing when it
/// can't start, or hasn't exited within `timeout`, when it is stopped. What it says on stderr is dropped.
pub fn capture(command: &mut Command, timeout: Duration) -> (i32, String) {
    let deadline = Instant::now() + timeout;
    let Ok(mut child) = command.stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return (-1, String::new());
    };
    let stdout = child.stdout.take();
    let (send, output) = mpsc::channel();
    // read as it comes, so a tool that prints a lot never waits on a full pipe
    let _ = thread::Builder::new().name("aipet-tool-output".into()).spawn(move || {
        let mut out = Vec::new();
        if stdout.is_some_and(|mut stdout| stdout.read_to_end(&mut out).is_ok()) {
            let _ = send.send(out);
        }
    });
    // the output ends as the tool exits
    if let Ok(out) = output.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return (status.code().unwrap_or(-1), String::from_utf8_lossy(&out).into_owned());
                }
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
                _ => break,
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    (-1, String::new())
}

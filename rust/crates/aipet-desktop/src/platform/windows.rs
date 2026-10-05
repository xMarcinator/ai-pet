//! The Windows platform (`src/AiPet.UI/Platform/WindowsPlatform.cs`): an agent's app is the first main window of its
//! exe, brought to the front; links and folders open with `ShellExecuteW`; and the player is Spotify, read from its
//! window's title and told what to play with `WM_APPCOMMAND`.
//!
//! Every Windows call goes through [`Win32`]. Only the real ones (`native`) are Windows-only, so the logic compiles,
//! and its tests run, everywhere.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use aipet_ui::platform::{Media, MediaPlayer, Platform};

use super::{Track, lock};

/// A window's handle (`HWND`); 0 is none.
pub type Hwnd = isize;

/// The media keys the pet sends Spotify's window (`APPCOMMAND_MEDIA_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommand {
    PlayPause,
    Next,
    Previous,
}

/// The Windows calls the platform makes, one method each, and its waits, the clock and aipet.log. [`Windows::new`]
/// has the real ones; tests answer for them.
pub trait Win32: Send + Sync {
    /// `EnumWindows`: the top-level windows, front to back.
    fn windows(&self) -> Vec<Hwnd>;

    /// `IsWindowVisible`.
    fn visible(&self, window: Hwnd) -> bool;

    /// Whether `GetWindow(GW_OWNER)` names an owner: the window is a pop-up of another (a menu, a tooltip).
    fn owned(&self, window: Hwnd) -> bool;

    /// Whether the window's extended style (`GetWindowLongW(GWL_EXSTYLE)`) has `WS_EX_TOOLWINDOW`.
    fn tool_window(&self, window: Hwnd) -> bool;

    /// `GetWindowTextW`, up to 511 characters.
    fn title(&self, window: Hwnd) -> String;

    /// `GetClassNameW`.
    fn class(&self, window: Hwnd) -> String;

    /// `GetWindowThreadProcessId`: the window's thread and process.
    fn thread_process(&self, window: Hwnd) -> (u32, u32);

    /// The file name of a process's exe (`OpenProcess`, `QueryFullProcessImageNameW`): none when it can't be read.
    fn exe(&self, process: u32) -> Option<String>;

    /// `GetForegroundWindow`.
    fn foreground(&self) -> Hwnd;

    /// `IsIconic`.
    fn minimized(&self, window: Hwnd) -> bool;

    /// `ShowWindow(SW_RESTORE)`.
    fn restore(&self, window: Hwnd);

    /// `SetForegroundWindow`.
    fn set_foreground(&self, window: Hwnd) -> bool;

    /// `BringWindowToTop`.
    fn bring_to_top(&self, window: Hwnd);

    /// `GetCurrentThreadId`.
    fn current_thread(&self) -> u32;

    /// `AttachThreadInput`.
    fn attach_input(&self, thread: u32, to: u32, attach: bool);

    /// `SendMessageW(window, WM_APPCOMMAND, window, command << 16)`: a media key for that window alone.
    fn app_command(&self, window: Hwnd, command: AppCommand);

    /// `ShellExecuteW` with the default verb: a link in its app, a folder in Explorer.
    fn shell_open(&self, target: &OsStr);

    fn sleep(&self, time: Duration);

    /// Now, in Unix seconds.
    fn now(&self) -> f64;

    /// Writes a line to aipet.log.
    fn log(&self, line: &str);
}

/// The Windows platform, over its [`Win32`] calls.
pub struct Windows {
    win32: Arc<dyn Win32>,
    spotify: Spotify,
}

impl Windows {
    /// With Windows' own calls.
    #[cfg(windows)]
    pub fn new() -> Windows {
        Windows::with(Arc::new(native::Native))
    }

    /// With these calls (a test's).
    pub fn with(win32: Arc<dyn Win32>) -> Windows {
        Windows {
            spotify: Spotify {
                win32: Arc::clone(&win32),
                seen: Mutex::default(),
            },
            win32,
        }
    }

    /// `FindWindowOf`: the first main window with a title of one of these exes (in lower case). A main window is
    /// visible, and neither a pop-up of another window (a menu, a tooltip) nor a tool window.
    fn window_of(&self, exes: &[&str]) -> Option<Hwnd> {
        let w = &*self.win32;
        w.windows().into_iter().find(|&window| {
            w.visible(window)
                && !w.owned(window)
                && !w.tool_window(window)
                && !w.title(window).is_empty()
                && w.exe(w.thread_process(window).1)
                    .is_some_and(|exe| exes.contains(&exe.to_lowercase().as_str()))
        })
    }
}

#[cfg(windows)]
impl Default for Windows {
    fn default() -> Windows {
        Windows::new()
    }
}

impl Platform for Windows {
    fn open_url(&self, url: &str) {
        let started = Instant::now();
        self.win32.shell_open(OsStr::new(url));
        let scheme = url.split(':').next().unwrap_or_default();
        debug(|| format!("a {scheme}: link opened in {}", ms(started)));
    }

    fn open_folder(&self, path: &Path) {
        self.win32.shell_open(path.as_os_str());
    }

    /// `FocusAgent`: the agent's desktop app comes to the front. Codex runs in ChatGPT's (ChatGPT.exe, from the
    /// OpenAI.Codex package). When no window of it is found, one link opens: the chat's, else the app's own
    /// (`claude://`; nothing opens ChatGPT, as `codex app` would start a new chat in the pet's own folder).
    fn focus_agent(&self, agent: &str, link: Option<&str>) {
        let (exes, own): (&[&str], _) = match agent {
            "codex" => (&["chatgpt.exe", "codex.exe"], None),
            _ => (&["claude.exe"], Some("claude://")),
        };
        // AIPET_DEBUG times it: the pet calls this as it handles the click
        let started = Instant::now();
        let found = self.window_of(exes);
        debug(|| format!("{agent}'s window looked for in {}: {}", ms(started), found.is_some()));
        let Some(window) = found else {
            // one link only: the chat's opens the app too (a ChatGPT window hidden in the tray isn't found)
            let url = link.or(own);
            if let Some(url) = url {
                self.open_url(url);
            }
            self.win32.log(&match url {
                Some(url) => format!(
                    "no {agent} window found; opening {}",
                    if link.is_some() { "the chat's link" } else { url }
                ),
                None => format!("no {agent} window found"),
            });
            return;
        };
        let w = &*self.win32;
        bring(w, window);
        let mut tries = 0;
        while tries < 10 && !in_front(w, window) {
            w.sleep(Duration::from_millis(15));
            tries += 1;
        }
        let front = in_front(w, window);
        if !front {
            w.log(&format!("couldn't bring {agent} to the front"));
        }
        debug(|| {
            let outcome = if front { "in front" } else { "not in front" };
            format!(
                "{agent} {outcome} {} after the click ({tries} waits of 15 ms)",
                ms(started)
            )
        });
    }

    fn media(&self) -> Option<&dyn MediaPlayer> {
        Some(&self.spotify)
    }
}

/// With AIPET_DEBUG=1, a line on stderr, as the shell's are.
fn debug(line: impl FnOnce() -> String) {
    static ON: LazyLock<bool> = LazyLock::new(|| std::env::var_os("AIPET_DEBUG").is_some_and(|v| v == "1"));
    if *ON {
        eprintln!("aipet: open: {}", line());
    }
}

/// The time since `started`, in ms.
fn ms(started: Instant) -> String {
    format!("{:.1} ms", started.elapsed().as_secs_f64() * 1000.0)
}

/// `InFront`: the foreground window belongs to the window's process.
fn in_front(w: &dyn Win32, window: Hwnd) -> bool {
    let foreground = w.foreground();
    foreground != 0 && w.thread_process(foreground).1 == w.thread_process(window).1
}

/// `Bring`: a minimised window is restored, and brought to the front; through the foreground window's input when
/// Windows refuses. No Alt-key trick: a lone Alt moves Electron apps (Claude, ChatGPT) into their menu bar. The pet
/// is normally the foreground app (the user just clicked it).
fn bring(w: &dyn Win32, window: Hwnd) {
    if w.minimized(window) {
        w.restore(window);
    }
    if !w.set_foreground(window) {
        let (foreground, me) = (w.thread_process(w.foreground()).0, w.current_thread());
        w.attach_input(me, foreground, true);
        w.bring_to_top(window);
        w.set_foreground(window);
        w.attach_input(me, foreground, false);
    }
}

/// Spotify (`SpotifyWindowTitle`): its main window's title is "Artist - Song" while it plays, and "Spotify…" while
/// it is paused. The media keys go to that window alone, so they can't reach another player.
struct Spotify {
    win32: Arc<dyn Win32>,
    seen: Mutex<Seen>,
}

/// What Spotify's last poll saw.
#[derive(Default)]
struct Seen {
    /// Its window (0: none).
    window: Hwnd,
    /// The window's title: a poll that sees the same one leaves the track.
    title: Option<String>,
    track: Track,
    /// Whether each process looked at runs Spotify. Forgotten every 10 s, as the C#'s list of Spotify's processes
    /// is taken again.
    spotify: HashMap<u32, bool>,
    /// When `spotify` was started (Unix seconds).
    spotify_since: f64,
}

impl Spotify {
    /// `Find`: the first `Chrome_WidgetWin*` window with a title of a Spotify process, and its title.
    fn find(&self, seen: &mut Seen) -> Option<(Hwnd, String)> {
        let w = &*self.win32;
        let now = w.now();
        if now - seen.spotify_since > 10.0 {
            seen.spotify.clear();
            seen.spotify_since = now;
        }
        w.windows().into_iter().find_map(|window| {
            if !w.class(window).starts_with("Chrome_WidgetWin") {
                return None;
            }
            let process = w.thread_process(window).1;
            let spotify = seen
                .spotify
                .entry(process)
                .or_insert_with(|| w.exe(process).is_some_and(|exe| is_spotify(&exe)));
            if !*spotify {
                return None;
            }
            let title = w.title(window);
            (!title.is_empty()).then_some((window, title))
        })
    }

    /// `Send`: a media key for the window the last poll found, else for the one there is now.
    fn send(&self, command: AppCommand) {
        let window = {
            let mut seen = lock(&self.seen);
            if seen.window == 0 {
                seen.window = self.find(&mut seen).map_or(0, |(window, _)| window);
            }
            seen.window
        };
        if window != 0 {
            self.win32.app_command(window, command);
        }
    }
}

impl MediaPlayer for Spotify {
    fn name(&self) -> String {
        "Spotify".to_owned()
    }

    /// `Poll`: Spotify's title again. A new one says what plays; a paused one keeps the last track.
    fn poll(&self) -> Media {
        let mut seen = lock(&self.seen);
        let found = self.find(&mut seen);
        seen.window = found.as_ref().map_or(0, |(window, _)| *window);
        let title = found.map(|(_, title)| title);
        if title != seen.title {
            let now = self.win32.now();
            match title.as_deref() {
                None => seen.track.set(None, None, false, now),
                Some(title) if paused(title) => seen.track.pause(),
                Some(title) => {
                    let (artist, song) = artist_and_song(title);
                    seen.track.set(Some(song), Some(artist), true, now);
                }
            }
            seen.title = title;
        }
        seen.track.media("Spotify")
    }

    fn previous(&self) {
        self.send(AppCommand::Previous);
    }

    fn play_pause(&self) {
        self.send(AppCommand::PlayPause);
    }

    fn next(&self) {
        self.send(AppCommand::Next);
    }

    /// Brings the window the last poll found to the front.
    fn focus(&self) {
        let window = lock(&self.seen).window;
        if window != 0 {
            bring(&*self.win32, window);
        }
    }
}

/// Whether an exe is Spotify's: the C#'s `GetProcessesByName("Spotify")`, which takes the name without `.exe`, in
/// any case.
fn is_spotify(exe: &str) -> bool {
    exe.eq_ignore_ascii_case("spotify.exe") || exe.eq_ignore_ascii_case("spotify")
}

/// A title Spotify shows while nothing plays: none, its own name ("Spotify Premium"), or an advert's.
fn paused(title: &str) -> bool {
    title.trim().is_empty()
        || title
            .get(..7)
            .is_some_and(|start| start.eq_ignore_ascii_case("Spotify"))
        || title.eq_ignore_ascii_case("Advertisement")
}

/// "Artist - Song": the artist before the first " - " and the song after it, trimmed. A title without one, or with
/// one at the very start, is all song.
fn artist_and_song(title: &str) -> (String, String) {
    match title.find(" - ") {
        Some(cut) if cut > 0 => (title[..cut].trim().to_owned(), title[cut + 3..].trim().to_owned()),
        _ => (String::new(), title.trim().to_owned()),
    }
}

/// Windows' own calls.
#[cfg(windows)]
mod native {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;
    use std::time::Duration;
    use std::{ptr, thread};

    use windows_sys::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM, WPARAM};
    use windows_sys::Win32::System::SystemServices::{
        APPCOMMAND_MEDIA_NEXTTRACK, APPCOMMAND_MEDIA_PLAY_PAUSE, APPCOMMAND_MEDIA_PREVIOUSTRACK,
    };
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow, GetWindow,
        GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE, SW_SHOWNORMAL,
        SendMessageW, SetForegroundWindow, ShowWindow, WM_APPCOMMAND, WS_EX_TOOLWINDOW,
    };

    use super::{AppCommand, Hwnd, Win32};

    pub(super) struct Native;

    impl Win32 for Native {
        fn windows(&self) -> Vec<Hwnd> {
            unsafe extern "system" fn add(window: HWND, list: LPARAM) -> BOOL {
                // SAFETY: `list` is the Vec `windows` gave EnumWindows, alive and not otherwise touched for the call
                unsafe { (*(list as *mut Vec<Hwnd>)).push(window) };
                1
            }
            let mut list: Vec<Hwnd> = Vec::new();
            // SAFETY: the callback only pushes to `list`, which outlives the call
            unsafe { EnumWindows(Some(add), &mut list as *mut Vec<Hwnd> as LPARAM) };
            list
        }

        fn visible(&self, window: Hwnd) -> bool {
            // SAFETY: plain user32 call; a window that is gone is not visible
            unsafe { IsWindowVisible(window) != 0 }
        }

        fn owned(&self, window: Hwnd) -> bool {
            // SAFETY: plain user32 call
            unsafe { GetWindow(window, GW_OWNER) != 0 }
        }

        fn tool_window(&self, window: Hwnd) -> bool {
            // SAFETY: plain user32 call
            let style = unsafe { GetWindowLongW(window, GWL_EXSTYLE) } as u32;
            style & WS_EX_TOOLWINDOW != 0
        }

        fn title(&self, window: Hwnd) -> String {
            let mut text = [0u16; 512];
            // SAFETY: the length passed is the buffer's; the call writes at most that many characters with the NUL
            let len = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) };
            String::from_utf16_lossy(&text[..len.clamp(0, text.len() as i32 - 1) as usize])
        }

        fn class(&self, window: Hwnd) -> String {
            let mut name = [0u16; 256];
            // SAFETY: the length passed is the buffer's; the call writes at most that many characters with the NUL
            let len = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
            String::from_utf16_lossy(&name[..len.clamp(0, name.len() as i32 - 1) as usize])
        }

        fn thread_process(&self, window: Hwnd) -> (u32, u32) {
            let mut process = 0;
            // SAFETY: the call writes the process id to a local
            let thread = unsafe { GetWindowThreadProcessId(window, &mut process) };
            (thread, process)
        }

        fn exe(&self, process: u32) -> Option<String> {
            // SAFETY: plain kernel32 call; the handle is closed below
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) };
            if handle == 0 {
                return None;
            }
            let mut path = [0u16; 512];
            let mut len = path.len() as u32;
            // SAFETY: `len` is the buffer's length on the way in, and the length written (without the NUL) on the
            // way out
            let read = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len) };
            // SAFETY: the handle OpenProcess gave, closed once
            unsafe { CloseHandle(handle) };
            if read == 0 {
                return None;
            }
            let path = OsString::from_wide(&path[..(len as usize).min(path.len())]);
            Some(Path::new(&path).file_name()?.to_string_lossy().into_owned())
        }

        fn foreground(&self) -> Hwnd {
            // SAFETY: plain user32 call
            unsafe { GetForegroundWindow() }
        }

        fn minimized(&self, window: Hwnd) -> bool {
            // SAFETY: plain user32 call
            unsafe { IsIconic(window) != 0 }
        }

        fn restore(&self, window: Hwnd) {
            // SAFETY: plain user32 call
            unsafe { ShowWindow(window, SW_RESTORE) };
        }

        fn set_foreground(&self, window: Hwnd) -> bool {
            // SAFETY: plain user32 call
            unsafe { SetForegroundWindow(window) != 0 }
        }

        fn bring_to_top(&self, window: Hwnd) {
            // SAFETY: plain user32 call
            unsafe { BringWindowToTop(window) };
        }

        fn current_thread(&self) -> u32 {
            // SAFETY: plain kernel32 call
            unsafe { GetCurrentThreadId() }
        }

        fn attach_input(&self, thread: u32, to: u32, attach: bool) {
            // SAFETY: plain user32 call
            unsafe { AttachThreadInput(thread, to, BOOL::from(attach)) };
        }

        fn app_command(&self, window: Hwnd, command: AppCommand) {
            let command = match command {
                AppCommand::PlayPause => APPCOMMAND_MEDIA_PLAY_PAUSE,
                AppCommand::Next => APPCOMMAND_MEDIA_NEXTTRACK,
                AppCommand::Previous => APPCOMMAND_MEDIA_PREVIOUSTRACK,
            };
            // SAFETY: plain user32 call; WM_APPCOMMAND carries no pointers
            unsafe { SendMessageW(window, WM_APPCOMMAND, window as WPARAM, (command << 16) as LPARAM) };
        }

        fn shell_open(&self, target: &OsStr) {
            let target: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
            // a NUL inside would cut the target short: such a one isn't opened, as std won't pass it as an argument
            if target[..target.len() - 1].contains(&0) {
                return;
            }
            // SAFETY: `target` ends with its NUL; the verb, parameters and folder are null (the defaults)
            unsafe { ShellExecuteW(0, ptr::null(), target.as_ptr(), ptr::null(), ptr::null(), SW_SHOWNORMAL) };
        }

        fn sleep(&self, time: Duration) {
            thread::sleep(time);
        }

        fn now(&self) -> f64 {
            crate::platform::unix_now()
        }

        fn log(&self, line: &str) {
            aipet_core::log::write(line);
        }
    }
}

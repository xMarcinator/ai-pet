//! The Codex log watcher: Codex chats as Codex's own session files show them (`src/AiPet.Core/CodexWatcher.cs`), for
//! when its hooks don't report them: hooks that aren't trusted yet (Codex skips them silently), a desktop app that
//! doesn't run them, an interrupted turn (on Windows AiPet registers no Interrupt hook), or a turn that ends in an
//! error (Codex runs no Stop hook then). The Board orders the hook's live state against this by Codex's turn ids,
//! else by time.
//!
//! It reads, never writes:
//! - `<codex home>/session_index.jsonl`: `{id, thread_name, updated_at}` per chat, so its name, and which chats exist;
//! - `<codex home>/sessions/**/rollout-*.jsonl`: one file per chat, session_meta first (id, originator, cwd, source),
//!   then event_msg lines: task_started, item_completed, task_complete, turn_aborted.
//!
//! These are Codex's internal formats (0.155/0.156), so everything here is best effort. [`CodexWatcher::start`] polls
//! them every 2 s on a thread of its own; [`CodexWatcher::sessions`] is what the last poll found.
//!
//! Where parity bends:
//! - the index is read as UTF-8, a byte order mark at the start of its last 512 KB dropped; .NET's StreamReader would
//!   also follow a UTF-16 or UTF-32 one (Codex writes UTF-8);
//! - a `thread_source` that starts with `memory`, and the order of the day folders, are compared ordinally, where the
//!   C# compares by the current culture: they differ only for text with characters a culture ignores or combines, and
//!   for folders not named as Codex's `YYYY/MM/DD`;
//! - a time without an offset reads as UTC (see `http::unix_seconds`); Codex writes them with a `Z`;
//! - the folders are listed without following symbolic links.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::board::Session;
use crate::http::{Element, Poller, lock, parse_json, unix_seconds};
use crate::sessions::AgentSessions;

/// Chats active within this long are shown.
const RECENT: Duration = Duration::from_secs(3 * 3600);
/// How often the loop polls.
const INTERVAL: Duration = Duration::from_secs(2);
/// How often the sessions folder is scanned for new rollout files.
const SCAN: Duration = Duration::from_secs(30);
/// How long a chat idle for hours is left before its file is looked at again.
const LOOK_AGAIN: Duration = Duration::from_secs(30);
/// How much of the end of the index is read, and of a rollout file on its first read.
const TAIL: u64 = 512 * 1024;
/// How much of a rollout file's first line (session_meta, which holds the model's instructions too) is read.
const FIRST_LINE: u64 = 4 * 1024 * 1024;
/// After the first scan, how many day folders (the newest) are scanned for new chats.
const DAYS: usize = 21;

/// Watches Codex's session files (`CodexWatcher`). It polls once [`CodexWatcher::start`] has started it, until
/// [`CodexWatcher::stop`] or until it is dropped.
pub struct CodexWatcher {
    shared: Arc<Shared>,
    poller: Mutex<Poller>,
}

struct Shared {
    /// Where Codex keeps its files; none for `$CODEX_HOME` (else `~/.codex`), read on each poll as the C#'s
    /// `Paths.CodexHome` is.
    home: Option<PathBuf>,
    state: Mutex<State>,
    /// What the last poll found, apart from the state so that reading it never waits for a poll's files.
    snapshot: Mutex<Vec<Session>>,
}

impl Default for CodexWatcher {
    fn default() -> Self {
        CodexWatcher::new()
    }
}

impl CodexWatcher {
    /// A watcher of `$CODEX_HOME`, else `~/.codex`.
    pub fn new() -> CodexWatcher {
        CodexWatcher::watching(None)
    }

    /// A watcher of the Codex files in `codex_home` rather than `$CODEX_HOME`'s: for tests, which keep off the user's
    /// data.
    pub fn with_codex_home(codex_home: impl Into<PathBuf>) -> CodexWatcher {
        CodexWatcher::watching(Some(codex_home.into()))
    }

    fn watching(home: Option<PathBuf>) -> CodexWatcher {
        CodexWatcher {
            shared: Arc::new(Shared {
                home,
                state: Mutex::default(),
                snapshot: Mutex::default(),
            }),
            poller: Mutex::default(),
        }
    }

    /// The chats as bubbles, keyed `codex:<id>` like the hook's.
    pub fn sessions(&self) -> Vec<Session> {
        lock(&self.shared.snapshot).clone()
    }

    /// Starts polling, now and then every 2 s, on a thread of its own (a running loop is stopped first).
    pub fn start(&self) {
        let shared = Arc::clone(&self.shared);
        lock(&self.poller).restart("aipet-codex-watcher", Duration::ZERO, move || {
            shared.poll(SystemTime::now());
            INTERVAL
        });
    }

    /// Stops polling: a poll under way finishes, as the C#'s does.
    pub fn stop(&self) {
        // a poller stops its loop when it is dropped
        *lock(&self.poller) = Poller::default();
    }

    /// One poll, now, on this thread, as the loop polls: for tests, which can't wait for the loop.
    #[doc(hidden)]
    pub fn poll(&self) {
        self.shared.poll(SystemTime::now());
    }
}

impl Shared {
    fn poll(&self, now: SystemTime) {
        let home = self.home.clone().unwrap_or_else(aipet_ipc::paths::codex_home);
        let found = if home.is_dir() {
            lock(&self.state).poll(&home, now)
        } else {
            Vec::new()
        };
        *lock(&self.snapshot) = found;
    }
}

/// What the watcher knows of the chats between polls.
#[derive(Default)]
struct State {
    /// The chats, in the order they were first seen.
    chats: Vec<Chat>,
    by_id: HashMap<String, usize>,
    /// The rollout file of each chat the scans found.
    path_by_id: HashMap<String, PathBuf>,
    last_scan: Option<SystemTime>,
    scanned_all: bool,
    /// The index's size and time when it was last read.
    index: Option<(u64, Option<SystemTime>)>,
}

/// A chat as its files show it.
struct Chat {
    id: String,
    path: Option<PathBuf>,
    /// The index's name for it (none while the index gives none).
    name: Option<String>,
    cwd: Option<String>,
    place: Option<String>,
    state: &'static str,
    detail: &'static str,
    prop: Option<&'static str>,
    /// How far into its file the chat has been read.
    offset: u64,
    /// Whether its file has been looked at, and the file's size and time then (no size: it couldn't be read then).
    looked: bool,
    length: Option<u64>,
    mtime: Option<SystemTime>,
    /// When to look again at the file of a chat idle for hours.
    look_again: Option<SystemTime>,
    ts: f64,
    /// A sub-agent, or one of Codex's own helper threads.
    skip: bool,
    /// The turn the latest task_started, task_complete or turn_aborted named (none when it named none), and whether
    /// it has ended.
    turn: Option<String>,
    turn_ended: bool,
}

impl Chat {
    fn new(id: String, path: Option<PathBuf>) -> Chat {
        Chat {
            id,
            path,
            name: None,
            cwd: None,
            place: None,
            state: "idle",
            detail: "Ready",
            prop: None,
            offset: 0,
            looked: false,
            length: None,
            mtime: None,
            look_again: None,
            ts: 0.0,
            skip: false,
            turn: None,
            turn_ended: false,
        }
    }

    /// The bubble, if the chat is one to show: a chat of its own, active within the last few hours.
    fn session(&self, now: SystemTime) -> Option<Session> {
        if self.skip || self.ts == 0.0 || older_than(self.ts, RECENT, now) {
            return None;
        }
        // only the index's name (the app's own title); the Board falls back to the folder, below the hook's prompt
        Some(Session {
            id: format!("codex:{}", self.id),
            agent: "codex",
            ts: self.ts,
            name: self.name.clone().unwrap_or_default(),
            eff: self.state,
            detail: self.detail.into(),
            prop: self.prop,
            cwd: self.cwd.clone().unwrap_or_default(),
            r#where: self.place.clone(),
            source: Some("log"),
            turn: self.turn.clone(),
            turn_ended: self.turn_ended,
            ..Session::default()
        })
    }
}

impl State {
    fn poll(&mut self, home: &Path, now: SystemTime) -> Vec<Session> {
        // which chats exist and their names (read again only when the index changes); the rollout files are located
        // once per chat
        let index = home.join("session_index.jsonl");
        if let Ok(meta) = fs::metadata(&index)
            && meta.is_file()
        {
            let seen = (meta.len(), meta.modified().ok());
            if self.index != Some(seen) {
                self.index = Some(seen);
                for (id, name) in read_index(&index) {
                    self.chat(id).name = name;
                }
            }
        }
        if self
            .last_scan
            .is_none_or(|last| now.duration_since(last).is_ok_and(|since| since > SCAN))
        {
            self.scan_files(&home.join("sessions"), now);
            self.last_scan = Some(now);
        }

        let mut found = Vec::new();
        for t in &mut self.chats {
            if t.path.is_none() {
                t.path = self.path_by_id.get(&t.id).cloned();
            }
            let Some(path) = t.path.clone() else { continue };
            if t.look_again.is_some_and(|at| now < at) {
                continue;
            }
            // cheap checks first: an unchanged file isn't opened, and a chat untouched for hours isn't read at all
            // until its file changes (Windows may keep an open file's time stale, so its size counts too)
            let Ok(meta) = fs::metadata(&path) else { continue };
            if !meta.is_file() {
                continue;
            }
            let (first_look, length, mtime) = (!t.looked, meta.len(), meta.modified().ok());
            let changed = t.length != Some(length) || t.mtime != mtime;
            (t.looked, t.length, t.mtime) = (true, Some(length), mtime);
            let untouched = mtime.is_some_and(|mtime| now.duration_since(mtime).is_ok_and(|age| age > RECENT));
            if t.offset == 0 && untouched && (first_look || !changed) {
                t.look_again = now.checked_add(LOOK_AGAIN);
            } else if changed && read(t, &path).is_err() {
                // couldn't read it just now: try again next time
                t.length = None;
            }
            found.extend(t.session(now));
        }
        found
    }

    /// The chat with this id, which is new if there's none yet.
    fn chat(&mut self, id: String) -> &mut Chat {
        let i = match self.by_id.get(&id) {
            Some(&i) => i,
            None => self.add(Chat::new(id, None)),
        };
        &mut self.chats[i]
    }

    fn add(&mut self, chat: Chat) -> usize {
        self.by_id.insert(chat.id.clone(), self.chats.len());
        self.chats.push(chat);
        self.chats.len() - 1
    }

    /// Maps chat ids to their rollout files (`rollout-<time>-<id>.jsonl`). A chat keeps writing to the file of the day
    /// it started, so the first scan goes through every day folder (for older chats that get resumed); after that
    /// only the newest few weeks of folders, for new chats.
    fn scan_files(&mut self, root: &Path, now: SystemTime) {
        let mut found = Vec::new();
        // listing every folder fails as a whole, as the C#'s sort of them does
        if let Ok(mut days) = folders(root) {
            let root_len = utf16_len(root);
            days.retain(|day| utf16_len(day).saturating_sub(root_len) >= 11);
            days.sort_by(|a, b| b.as_os_str().cmp(a.as_os_str()));
            let take = if self.scanned_all { DAYS } else { usize::MAX };
            // a folder that can't be listed ends the scan, keeping what was found before it
            'days: for day in days.iter().take(take) {
                let Ok(files) = fs::read_dir(day) else { break };
                for file in files {
                    let Ok(file) = file else { break 'days };
                    let name = file.file_name();
                    let Some(stem) = rollout_stem(&name.to_string_lossy()).map(str::to_owned) else {
                        continue;
                    };
                    let path = file.path();
                    if path.is_file()
                        && let Some(id) = last_units(&stem, 36)
                    {
                        found.push((id, path));
                    }
                }
            }
        }
        self.scanned_all = true;
        for (id, path) in found {
            self.path_by_id.insert(id.clone(), path.clone());
            // chats started in the last few hours that have no name yet aren't in the index
            let recent = fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .is_ok_and(|mtime| now.duration_since(mtime).map_or(true, |age| age < RECENT));
            if !self.by_id.contains_key(&id) && recent {
                self.add(Chat::new(id, Some(path)));
            }
        }
    }
}

/// Whether Unix time `ts` (seconds, read to the millisecond as the C# reads it) is more than `span` before `now`.
fn older_than(ts: f64, span: Duration, now: SystemTime) -> bool {
    let ms = (ts * 1000.0) as i64;
    let at = if ms >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_millis(ms.unsigned_abs()))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_millis(ms.unsigned_abs()))
    };
    at.is_some_and(|at| now.duration_since(at).is_ok_and(|age| age > span))
}

/// The chats the index names, in its order, each with the name its line gives (none for a null or missing one),
/// from its last 512 KB. A line that isn't one the C# reads is left out.
fn read_index(path: &Path) -> Vec<(String, Option<String>)> {
    let Ok(bytes) = read_tail(path, TAIL) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    text.split('\n').filter_map(index_line).collect()
}

fn index_line(line: &str) -> Option<(String, Option<String>)> {
    if line.encode_utf16().count() < 10 {
        return None;
    }
    let doc = parse_json(line).ok()?;
    let text = |key: &str| -> Result<Option<String>, String> {
        Ok(match Element(&doc).try_get(key)? {
            Some(value) => value.string()?.map(str::to_owned),
            None => None,
        })
    };
    let id = text("id").ok()?;
    let name = text("thread_name").ok()?;
    // the C# parses updated_at and drops it: only a value that isn't text (which throws) matters
    text("updated_at").ok()?;
    Some((id?, name))
}

/// The last `tail` bytes of the file. Like the C#'s FileStream, it lets Codex go on writing, and even delete the file.
fn read_tail(path: &Path, tail: u64) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(tail)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Every folder under `root`, as `Directory.EnumerateDirectories(root, "*", SearchOption.AllDirectories)` lists them.
fn folders(root: &Path) -> io::Result<Vec<PathBuf>> {
    let (mut all, mut pending) = (Vec::new(), vec![root.to_path_buf()]);
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
                all.push(entry.path());
            }
        }
    }
    Ok(all)
}

fn utf16_len(path: &Path) -> usize {
    path.to_string_lossy().encode_utf16().count()
}

/// A rollout file's name without its `.jsonl`: one `rollout-*.jsonl` matches, ignoring case on Windows as .NET does
/// there.
fn rollout_stem(name: &str) -> Option<&str> {
    let same = |a: &str, b: &str| {
        if cfg!(windows) {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let stem = name.len().checked_sub(6).filter(|&n| n >= 8)?;
    (same(name.get(..8)?, "rollout-") && same(name.get(stem..)?, ".jsonl")).then(|| &name[..stem])
}

/// The text's last `n` UTF-16 units, as the C#'s `s[^n..]` (a character cut in half leaves U+FFFD); none when it's
/// shorter.
fn last_units(s: &str, n: usize) -> Option<String> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let from = units.len().checked_sub(n)?;
    Some(String::from_utf16_lossy(&units[from..]))
}

/// Reads what was appended since last time (the first time: the start, for session_meta, and the last 512 KB). An
/// error is a file that couldn't be opened or read just now.
fn read(t: &mut Chat, path: &Path) -> io::Result<()> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len == t.offset {
        return Ok(());
    }
    if t.offset == 0 {
        let (first, end) = first_line(&file)?;
        if let Some(first) = first {
            // it never fails, so the offset always moves past that line
            meta(t, &first);
        }
        t.offset = end.max(len.saturating_sub(TAIL));
    }
    if len < t.offset {
        // rewritten
        t.offset = 0;
    }
    file.seek(SeekFrom::Start(t.offset))?;
    let mut bytes = Vec::new();
    (&file).take(len - t.offset).read_to_end(&mut bytes)?;
    let Some(end) = bytes.iter().rposition(|&b| b == b'\n') else {
        // no complete line yet
        return Ok(());
    };
    t.offset += end as u64 + 1;
    for line in String::from_utf8_lossy(&bytes[..=end]).split('\n') {
        if line.contains("\"event_msg\"") {
            event(t, line);
        }
    }
    Ok(())
}

/// The file's first line (at most its first 4 MB), none when it's empty; and where it ends in the file (past its
/// `\n`, when it has one).
fn first_line(file: &File) -> io::Result<(Option<String>, u64)> {
    let mut bytes = Vec::new();
    BufReader::new(file.take(FIRST_LINE)).read_until(b'\n', &mut bytes)?;
    let end = bytes.len() as u64;
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    Ok((
        (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned()),
        end,
    ))
}

/// A JSON value's text, if it is text (`ValueKind == JsonValueKind.String`).
fn text_of(value: Option<Element<'_>>) -> Option<&str> {
    match value {
        Some(Element(Value::String(s))) => Some(s),
        _ => None,
    }
}

/// session_meta: where the chat runs, and whether it is a chat of its own. A line that isn't JSON, or not of the shape
/// expected (another Codex version, a stray file), is left alone: the error is the C#'s exception, which it catches.
fn meta(t: &mut Chat, line: &str) {
    if let Ok(doc) = parse_json(line) {
        let _ = read_meta(t, Element(&doc));
    }
}

fn read_meta(t: &mut Chat, doc: Element<'_>) -> Result<(), String> {
    let Some(p) = doc.try_get("payload")? else {
        return Ok(());
    };
    if let Some(cwd) = text_of(p.try_get("cwd")?) {
        t.cwd = Some(cwd.replace(r"\\?\", ""));
    }
    t.place = match text_of(p.try_get("originator")?) {
        Some("Codex Desktop" | "codex_work_desktop") => Some("desktop".into()),
        Some("codex-tui" | "codex_cli_rs") => Some("terminal".into()),
        Some("codex_exec") => Some("exec".into()),
        Some("") | None => None,
        Some(other) => Some(AgentSessions::lower_invariant(other)),
    };
    // sub-agents and Codex's own background threads (reviews, memory) aren't chats of their own
    if let Some(Element(Value::Object(_))) = p.try_get("source")? {
        t.skip = true;
    }
    if text_of(p.try_get("thread_source")?).is_some_and(|src| src == "guardian_review" || src.starts_with("memory")) {
        t.skip = true;
    }
    Ok(())
}

/// An event_msg line: what the chat is doing, from when. A line that isn't JSON, or not of the shape expected, is left
/// alone: the error is the C#'s exception, which it catches.
fn event(t: &mut Chat, line: &str) {
    if let Ok(doc) = parse_json(line) {
        let _ = read_event(t, Element(&doc));
    }
}

fn read_event(t: &mut Chat, doc: Element<'_>) -> Result<(), String> {
    let Some(p) = doc.try_get("payload")? else {
        return Ok(());
    };
    let Some(kind) = p.try_get("type")? else {
        return Ok(());
    };
    let ts = match doc.try_get("timestamp")? {
        Some(when) => when.string()?.and_then(unix_seconds).unwrap_or(0.0),
        None => 0.0,
    };
    let set = |t: &mut Chat, state, detail, prop| {
        (t.state, t.detail, t.prop) = (state, detail, prop);
        if ts > 0.0 {
            t.ts = ts;
        }
    };
    let turn = text_of(p.try_get("turn_id")?).map(str::to_owned);
    match kind.string()? {
        Some("task_started") => {
            set(t, "thinking", "Thinking", None);
            (t.turn, t.turn_ended) = (turn, false);
        }
        Some("task_complete") => {
            set(t, "done", "Done", None);
            (t.turn, t.turn_ended) = (turn, true);
        }
        Some("turn_aborted") => {
            set(t, "idle", "Interrupted", None);
            (t.turn, t.turn_ended) = (turn, true);
        }
        Some("item_completed") if matches!(t.state, "thinking" | "working") => {
            // only finished items are saved, so this is what it just did, not what it's doing
            let item = match p.try_get("item")? {
                Some(item) => match item.try_get("type")? {
                    Some(kind) => kind.string()?,
                    None => None,
                },
                None => None,
            };
            match item {
                Some("CommandExecution") => set(t, "working", "Running commands", Some("laptop")),
                Some("FileChange") => set(t, "working", "Editing files", Some("laptop")),
                Some("WebSearch") => set(t, "working", "Browsing the web", Some("lens")),
                Some("McpToolCall") => set(t, "working", "Using a tool", Some("laptop")),
                // reasoning, a message: it's back to thinking, so an older tool doesn't outlive the hook's "Thinking"
                _ => set(t, "thinking", "Thinking", None),
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The parts of polling the golden data can't reach, since the C# reads the real clock: a chat idle for hours is
    // looked at again every 30 s, and the scans after the first come every 30 s and go through the newest 21 day
    // folders only.

    fn home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aipet-codex-watcher-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sessions")).unwrap();
        dir
    }

    fn at(time: SystemTime) -> String {
        let ms = time.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
        let (days, ms_of_day) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
        // days since 1970 to a civil date (Howard Hinnant's algorithm)
        let z = days + 719_468;
        let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
        let y = yoe + era * 400 + i64::from(m <= 2);
        let (h, mi, s, milli) = (
            ms_of_day / 3_600_000,
            ms_of_day / 60_000 % 60,
            ms_of_day / 1000 % 60,
            ms_of_day % 1000,
        );
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{milli:03}Z")
    }

    fn rollout(dir: &Path, id: &str, started: SystemTime, mtime: SystemTime) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("rollout-2026-01-01T00-00-00-{id}.jsonl"));
        let text = format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/work/app\"}}}}\n\
             {{\"timestamp\":\"{}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\"}}}}\n",
            at(started)
        );
        fs::write(&path, text).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        path
    }

    fn shown(state: &mut State, home: &Path, now: SystemTime) -> Vec<String> {
        state.poll(home, now).into_iter().map(|s| s.id).collect()
    }

    #[test]
    fn a_chat_idle_for_hours_is_looked_at_again_every_30_s() {
        let home = home("idle");
        let now = SystemTime::now();
        let hours = now - Duration::from_secs(4 * 3600);
        let id = "00000000-0000-4000-8000-000000000001";
        fs::write(
            home.join("session_index.jsonl"),
            format!("{{\"id\":\"{id}\",\"thread_name\":\"Old\"}}\n"),
        )
        .unwrap();
        let path = rollout(&home.join("sessions/2026/01/01"), id, hours, hours);
        let mut state = State::default();
        assert!(shown(&mut state, &home, now).is_empty());
        // it goes on, but isn't looked at until 30 s have passed
        rollout(&home.join("sessions/2026/01/01"), id, now, now);
        assert!(shown(&mut state, &home, now + Duration::from_secs(29)).is_empty());
        assert_eq!(
            shown(&mut state, &home, now + Duration::from_secs(31)),
            [format!("codex:{id}")]
        );
        fs::remove_file(path).unwrap();
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn later_scans_come_every_30_s_and_go_through_the_newest_21_day_folders() {
        let home = home("scans");
        let now = SystemTime::now();
        let day = |n: u32| home.join(format!("sessions/2026/01/{n:02}"));
        for n in 1..=22 {
            fs::create_dir_all(day(n)).unwrap();
        }
        let mut state = State::default();
        assert!(shown(&mut state, &home, now).is_empty());
        let (oldest, newest) = (
            "00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000022",
        );
        rollout(&day(1), oldest, now, now);
        rollout(&day(22), newest, now, now);
        assert!(shown(&mut state, &home, now + Duration::from_secs(30)).is_empty());
        assert_eq!(
            shown(&mut state, &home, now + Duration::from_secs(31)),
            [format!("codex:{newest}")]
        );
        // the first scan of another watcher goes through them all
        let mut first = State::default();
        let mut both = shown(&mut first, &home, now);
        both.sort();
        assert_eq!(both, [format!("codex:{oldest}"), format!("codex:{newest}")]);
        let _ = fs::remove_dir_all(home);
    }
}

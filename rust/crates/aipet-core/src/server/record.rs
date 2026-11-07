//! `hook-events.log` and the recent lines the doctor's ping gets (`HookServer.cs:303-325`).
//!
//! One line per event, ignored ones too (the doctor's test event is one), `yyyy-MM-dd HH:mm:ss.fff <line>` in local
//! time: the last 30 are kept for pings, and every one is appended to hook-events.log, which keeps its last half past
//! 64 KB. The file is written as the C#'s `File` calls write it: UTF-8 without a BOM, lines ending in the system's
//! newline, and nothing at all when a call fails.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RECENT_LINES: usize = 30;
/// Past this size the file keeps its last half.
const LIMIT: u64 = 64 * 1024;
/// `Environment.NewLine`.
const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

pub(super) struct Record {
    recent: VecDeque<String>,
    path: PathBuf,
}

impl Record {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            recent: VecDeque::new(),
            path,
        }
    }

    /// Records an event's line, as of `now`.
    pub(super) fn add(&mut self, line: &str, now: SystemTime) {
        let line = format!("{} {line}", timestamp(now));
        let _ = append(&self.path, &line);
        self.recent.push_back(line);
        while self.recent.len() > RECENT_LINES {
            self.recent.pop_front();
        }
    }

    /// The last 30 lines, oldest first.
    pub(super) fn recent(&self) -> impl Iterator<Item = &str> {
        self.recent.iter().map(String::as_str)
    }
}

/// Appends the line, first halving a file that is past 64 KB.
fn append(path: &Path, line: &str) -> io::Result<()> {
    // File.Exists: anything but a folder
    if let Ok(meta) = fs::metadata(path)
        && !meta.is_dir()
        && meta.len() > LIMIT
    {
        let bytes = fs::read(path)?;
        let text = read_text(&bytes);
        // a half the C#'s writer refuses leaves the file as it was, and the line out of it
        let Some(half) = halve(&text) else {
            return Ok(());
        };
        fs::write(path, half)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(format!("{line}{NEWLINE}").as_bytes())
}

/// `File.ReadAllText`: UTF-8 without its BOM, with what isn't UTF-8 as U+FFFD.
fn read_text(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
}

/// The text's last half from the first line after its middle: `text[(text.Length / 2)..]`, as the C# cuts it, in
/// UTF-16 units. None when the middle splits a surrogate pair and no line follows it: that half would start with a
/// lone surrogate, which the C#'s UTF-8 writer throws on.
fn halve(text: &str) -> Option<&str> {
    let middle = text.encode_utf16().count() / 2;
    let (mut units, mut start, mut split) = (0, text.len(), false);
    for (i, c) in text.char_indices() {
        if units == middle {
            start = i;
            break;
        }
        units += c.len_utf16();
        if units > middle {
            // between c's two surrogates
            (start, split) = (i + c.len_utf8(), true);
            break;
        }
    }
    let half = &text[start..];
    match half.find('\n') {
        Some(newline) => Some(&half[newline + 1..]),
        None if split => None,
        None => Some(half),
    }
}

/// `DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss.fff", CultureInfo.InvariantCulture)`: the local time, its
/// milliseconds cut.
fn timestamp(now: SystemTime) -> String {
    let t = local(now);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.year, t.month, t.day, t.hour, t.minute, t.second, t.millis
    )
}

struct Civil {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
}

#[cfg(unix)]
fn local(now: SystemTime) -> Civil {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs() as libc::time_t;
    // SAFETY: an all-zero tm is a valid value to be filled in
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live values; localtime_r writes tm only
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        // no time zone to be had: UTC
        return civil(since.as_secs() * 10_000_000 + UNIX_EPOCH_TICKS + u64::from(since.subsec_nanos() / 100));
    }
    Civil {
        year: i64::from(tm.tm_year) + 1900,
        month: tm.tm_mon as u32 + 1,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
        millis: since.subsec_millis(),
    }
}

/// By the time zone's current bias (`FileTimeToLocalFileTime`), which is right for the present moment.
#[cfg(windows)]
fn local(now: SystemTime) -> Civil {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;

    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let utc = UNIX_EPOCH_TICKS + (since.as_nanos() / 100) as u64;
    let utc = FILETIME {
        dwLowDateTime: utc as u32,
        dwHighDateTime: (utc >> 32) as u32,
    };
    let mut local = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: both pointers are to live FILETIMEs; local is written only
    let time = if unsafe { FileTimeToLocalFileTime(&utc, &mut local) } != 0 {
        local
    } else {
        utc
    };
    civil((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime))
}

/// 100 ns ticks from 1601 to 1970.
const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;

/// The date and time `ticks` (100 ns) after the start of 1601, in the proleptic Gregorian calendar.
fn civil(ticks: u64) -> Civil {
    let ms = ticks / 10_000;
    let time = ms % 86_400_000;
    // days since 1970-03-01, whose years end with February's leap day (H. Hinnant's days_from_civil, backwards)
    let days = (ms / 86_400_000) as i64 - 134_774 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_from_march + 2) / 5 + 1) as u32;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    } as u32;
    Civil {
        year: year_of_era + era * 400 + i64::from(month <= 2),
        month,
        day,
        hour: (time / 3_600_000) as u32,
        minute: (time / 60_000 % 60) as u32,
        second: (time / 1000 % 60) as u32,
        millis: (time % 1000) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ticks: u64) -> String {
        let t = civil(ticks);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            t.year, t.month, t.day, t.hour, t.minute, t.second, t.millis
        )
    }

    #[test]
    fn ticks_give_the_calendars_date_and_time() {
        let unix = |secs: u64, ms: u64| UNIX_EPOCH_TICKS + secs * 10_000_000 + ms * 10_000 + 9_999;
        for (ticks, expected) in [
            (0, "1601-01-01 00:00:00.000"),
            (unix(0, 0), "1970-01-01 00:00:00.000"),
            (unix(951_782_400, 1), "2000-02-29 00:00:00.001"),
            (unix(1_709_251_199, 999), "2024-02-29 23:59:59.999"),
            (unix(1_758_800_000, 123), "2025-09-25 11:33:20.123"),
            (unix(4_102_444_800, 0), "2100-01-01 00:00:00.000"),
            (unix(4_107_456_000, 0), "2100-02-28 00:00:00.000"),
            (unix(4_107_542_400, 0), "2100-03-01 00:00:00.000"),
        ] {
            assert_eq!(at(ticks), expected, "{ticks}");
        }
    }

    #[test]
    fn a_line_starts_with_the_local_time() {
        let stamp = timestamp(SystemTime::now());
        let shape: String = stamp
            .chars()
            .map(|c| if c.is_ascii_digit() { '0' } else { c })
            .collect();
        assert_eq!(shape, "0000-00-00 00:00:00.000", "{stamp}");
    }

    /// The C#'s halves (`File.ReadAllText`, the cut, `File.WriteAllText`) of the same files, as .NET 10 wrote them:
    /// how many bytes are kept, and the first and last of them.
    #[test]
    fn a_file_keeps_its_last_half_as_the_csharp_cuts_it() {
        let lines = |n: usize, text: &str| -> String { (0..n).map(|i| format!("line {i} {text}\n")).collect() };
        let mut invalid = b"\xEF\xBB\xBF".to_vec();
        for i in 0..5000 {
            invalid.extend(format!("line {i} ").bytes());
            invalid.extend(b"\x80\xE2\x82 end\n");
        }
        // a name, the file, and how many of its bytes are kept, with the first and last of them
        type Case<'a> = (&'a str, Vec<u8>, usize, &'a [u8], &'a [u8]);
        let cases: [Case; 6] = [
            (
                "ascii",
                lines(4000, "abcdefgh").into(),
                37_430,
                b"line 2030 ab",
                b"99 abcdefgh\n",
            ),
            (
                "mixed",
                lines(4000, "é😀ü").into(),
                37_278,
                "line 2038 é".as_bytes(),
                "99 é😀ü\n".as_bytes(),
            ),
            (
                "a pair split, a line after it",
                format!(
                    "{}😀{}\nrest\n{}",
                    "x".repeat(40_000),
                    "y".repeat(20_000),
                    "z".repeat(19_994)
                )
                .into(),
                19_999,
                b"rest\nzzzzzzz",
                b"zzzzzzzzzzzz",
            ),
            (
                "one line",
                "q".repeat(65_537).into(),
                32_769,
                b"qqqqqqqqqqqq",
                b"qqqqqqqqqqqq",
            ),
            (
                "CR LF",
                lines(5000, "abc\r").into(),
                36_930,
                b"line 2538 ab",
                b"e 4999 abc\r\n",
            ),
            (
                "not UTF-8, with a BOM",
                invalid,
                51_807,
                b"line 2533 \xEF\xBF",
                b" \xEF\xBF\xBD\xEF\xBF\xBD end\n",
            ),
        ];
        for (name, file, kept, head, tail) in cases {
            let text = read_text(&file);
            let half = halve(&text).unwrap_or_else(|| panic!("{name}: no half"));
            assert_eq!(half.len(), kept, "{name}");
            assert!(half.as_bytes().starts_with(head), "{name}: {:?}", &half[..20]);
            assert!(half.as_bytes().ends_with(tail), "{name}");
        }
        // the middle splits a surrogate pair, and no line follows
        let text = format!("{}😀{}", "x".repeat(40_000), "y".repeat(40_000));
        assert_eq!(halve(&text), None);
    }

    /// At 64 KB the file is left whole; past it the line goes after the last half. A half the C# can't write leaves
    /// the file as it was, without the line; the line is kept for pings all the same.
    #[test]
    fn the_file_is_halved_only_past_64_kb() {
        let dir = std::env::temp_dir().join(format!("aipet-core-record-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hook-events.log");
        let mut record = Record::new(path.clone());
        let now = SystemTime::now();
        let stamp = timestamp(now);

        fs::write(&path, "q".repeat(65_536)).unwrap();
        record.add("whole", now);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, format!("{}{stamp} whole{NEWLINE}", "q".repeat(65_536)));

        fs::write(&path, format!("{}\n{}", "a".repeat(40_000), "b".repeat(30_000))).unwrap();
        record.add("halved", now);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, format!("{}{stamp} halved{NEWLINE}", "b".repeat(30_000)));

        let split = format!("{}😀{}", "x".repeat(40_000), "y".repeat(40_000));
        fs::write(&path, &split).unwrap();
        record.add("lost", now);
        assert_eq!(fs::read_to_string(&path).unwrap(), split);

        let recent: Vec<&str> = record.recent().collect();
        let lines = ["whole", "halved", "lost"].map(|l| format!("{stamp} {l}"));
        assert_eq!(recent, lines);
        for i in 0..40 {
            record.add(&i.to_string(), now);
        }
        assert_eq!(record.recent().count(), RECENT_LINES);
        assert_eq!(record.recent().next(), Some(format!("{stamp} 10").as_str()));
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! `aipet.log`, deleted past 256 KB (`Log.Write`, `src/AiPet.Core/Platform.cs`).
//!
//! One line per call, `HH:mm:ss.fff <line>` in local time, appended as UTF-8, with the current culture's time
//! separator in place of `:` as .NET's format has it (12.34.56.789 in Danish or Finnish). Logging never fails its
//! caller: a line that can't be written is lost, as in the C#.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Past this size the log is deleted before the next line.
const LIMIT: u64 = 256 * 1024;

/// Appends a line to the app's log in the data folder (`Log.Write`).
pub fn write(line: &str) {
    append(&aipet_ipc::paths::log(), line, SystemTime::now());
}

fn append(path: &Path, line: &str, now: SystemTime) {
    let _ = (|| -> io::Result<()> {
        // as the C#: a log that can't be deleted gets no line either
        if let Ok(meta) = fs::metadata(path)
            && meta.is_file()
            && meta.len() > LIMIT
        {
            fs::remove_file(path)?;
        }
        let (h, m, s, ms) = local_time(now);
        let sep = time_separator();
        let text = format!("{h:02}{sep}{m:02}{sep}{s:02}.{ms:03} {line}\n");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())
    })();
}

/// The current culture's time separator (`DateTimeFormatInfo.TimeSeparator`), which .NET writes for the `:` in
/// `HH:mm:ss.fff`. Found once, as .NET finds the current culture once.
pub fn time_separator() -> &'static str {
    static SEPARATOR: OnceLock<String> = OnceLock::new();
    SEPARATOR.get_or_init(current_time_separator)
}

/// On Windows the current culture is the user's (Region's format), with the time format as the user has it
/// (`LOCALE_STIMEFORMAT`), and .NET takes the separator out of that format. fr-CA's format has none (`HH 'h' mm`),
/// and .NET gives it `:`.
#[cfg(windows)]
fn current_time_separator() -> String {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, GetUserDefaultLocaleName, LOCALE_STIMEFORMAT};

    let mut name = [0u16; 85];
    // SAFETY: the length passed is the buffer's
    let n = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), name.len() as i32) };
    // no name: .NET's invariant culture
    if n <= 1 || String::from_utf16_lossy(&name[..n as usize - 1]) == "fr-CA" {
        return ":".into();
    }
    let mut format = [0u16; 128];
    // SAFETY: a null name is the user's locale (LOCALE_NAME_USER_DEFAULT); the length passed is the buffer's
    let n = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            LOCALE_STIMEFORMAT,
            format.as_mut_ptr(),
            format.len() as i32,
        )
    };
    if n <= 1 {
        return ":".into();
    }
    windows_format::separator(&windows_format::reescape(&String::from_utf16_lossy(
        &format[..n as usize - 1],
    )))
}

/// Elsewhere .NET names the current culture after the locale ICU finds in the environment (`LC_ALL`, else
/// `LC_MESSAGES`, else `LANG`), and ICU's data has its time format. (On macOS .NET asks the system for the locale
/// instead; macOS isn't a release target.)
#[cfg(not(windows))]
fn current_time_separator() -> String {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|l| !l.is_empty()))
        .unwrap_or_default();
    culture_time_separator(&culture(&locale)).to_owned()
}

/// The .NET culture of a POSIX locale: `da_DK.UTF-8` is da-DK; C and POSIX are the invariant culture (`""`).
#[cfg(not(windows))]
fn culture(locale: &str) -> String {
    let name = locale.split(['.', '@']).next().unwrap_or_default();
    if name == "C" || name == "POSIX" {
        String::new()
    } else {
        name.replace('_', "-")
    }
}

/// A culture's time separator in .NET on ICU (Linux), by the culture's name: `.` for the cultures whose time
/// format ICU writes as 12.34.56, and for the cultures under them (da-DK under da); `:` for the rest.
/// tests/golden/data/log.json has every culture's.
pub fn culture_time_separator(culture: &str) -> &'static str {
    const DOTTED: [&str; 11] = [
        "as", "da", "en-DK", "en-FI", "fi", "id", "kl", "ms-ID", "si", "smn", "su",
    ];
    let mut name = culture;
    loop {
        if DOTTED.iter().any(|d| d.eq_ignore_ascii_case(name)) {
            return ".";
        }
        match name.rfind('-') {
            Some(parent) => name = &name[..parent],
            None => return ":",
        }
    }
}

/// How .NET finds the time separator in a Windows time format (`CultureData`).
#[cfg(windows)]
mod windows_format {
    /// `ReescapeWin32String`: Windows' `''` in quotes as .NET's `\'`, and each `\` doubled.
    pub fn reescape(format: &str) -> String {
        let f: Vec<char> = format.chars().collect();
        let mut out = String::with_capacity(format.len());
        let mut quoted = false;
        let mut i = 0;
        while i < f.len() {
            match f[i] {
                '\'' if quoted && f.get(i + 1) == Some(&'\'') => {
                    out.push_str("\\'");
                    i += 1;
                }
                '\'' => {
                    quoted = !quoted;
                    out.push('\'');
                }
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            }
            i += 1;
        }
        out
    }

    /// `GetTimeSeparator`: what stands between the first run of an hour, minute or second letter and the next
    /// such letter, without its quotes and escapes; empty when there's none.
    pub fn separator(format: &str) -> String {
        let f: Vec<char> = format.chars().collect();
        let Some(mut i) = time_part(&f, 0) else {
            return String::new();
        };
        let part = f[i];
        while i < f.len() && f[i] == part {
            i += 1;
        }
        match time_part(&f, i) {
            Some(end) => unescape(&f, i, end - 1),
            None => String::new(),
        }
    }

    /// `IndexOfTimePart`: the next `H`, `h`, `m` or `s` from `from` that isn't in quotes.
    fn time_part(f: &[char], from: usize) -> Option<usize> {
        let mut quoted = false;
        let mut i = from;
        while i < f.len() {
            match f[i] {
                'H' | 'h' | 'm' | 's' if !quoted => return Some(i),
                '\\' if matches!(f.get(i + 1), Some('\'' | '\\')) => i += 1,
                '\'' => quoted = !quoted,
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// `UnescapeNlsString`: `f[start..=end]` without its quotes, each `\` taking the next character as it is.
    fn unescape(f: &[char], start: usize, end: usize) -> String {
        let mut out = String::new();
        let mut i = start;
        while i < f.len() && i <= end {
            match f[i] {
                '\'' => {}
                '\\' => {
                    i += 1;
                    out.extend(f.get(i));
                }
                c => out.push(c),
            }
            i += 1;
        }
        out
    }
}

/// The local time of day of `now`: hours, minutes, seconds and milliseconds (cut, not rounded, as .NET's `fff`).
#[cfg(unix)]
fn local_time(now: SystemTime) -> (u32, u32, u32, u32) {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs() as libc::time_t;
    // SAFETY: an all-zero tm is a valid value to be filled in
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live values; localtime_r writes tm only
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        let day = since.as_secs() % 86_400;
        return (
            (day / 3600) as u32,
            (day / 60 % 60) as u32,
            (day % 60) as u32,
            since.subsec_millis(),
        );
    }
    (
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
        since.subsec_millis(),
    )
}

/// The local time of day of `now`, by the time zone's current bias (`FileTimeToLocalFileTime`), which is right for
/// the present moment: hours, minutes, seconds and milliseconds (cut, not rounded, as .NET's `fff`).
#[cfg(windows)]
fn local_time(now: SystemTime) -> (u32, u32, u32, u32) {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;

    /// 100 ns ticks from 1601 to 1970.
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
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
    let ticks = if unsafe { FileTimeToLocalFileTime(&utc, &mut local) } != 0 {
        (u64::from(local.dwHighDateTime) << 32) | u64::from(local.dwLowDateTime)
    } else {
        (u64::from(utc.dwHighDateTime) << 32) | u64::from(utc.dwLowDateTime)
    };
    let ms = ticks / 10_000 % 86_400_000;
    (
        (ms / 3_600_000) as u32,
        (ms / 60_000 % 60) as u32,
        (ms / 1000 % 60) as u32,
        (ms % 1000) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("aipet-core-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }

    /// `HH:mm:ss.fff line`, with the culture's time separator, the milliseconds cut.
    #[test]
    fn a_line_has_the_time_of_day() {
        let d = dir("line");
        let log = d.0.join("aipet.log");
        let now = UNIX_EPOCH + Duration::new(1_758_800_000, 999_900_000);
        append(&log, "one", now);
        append(&log, "two", now);
        let text = fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text:?}");
        let (h, m, s, ms) = local_time(now);
        assert_eq!(ms, 999);
        let sep = time_separator();
        assert_eq!(lines[0], format!("{h:02}{sep}{m:02}{sep}{s:02}.999 one"));
        assert!(text.ends_with(" two\n"));
    }

    /// The log is deleted only once it is past 256 KB.
    #[test]
    fn the_log_rotates_past_256_kb() {
        let d = dir("rotate");
        let log = d.0.join("aipet.log");
        let line = |text: &str| format!("00{0}00{0}00.000 {text}\n", time_separator()).len();
        fs::write(&log, vec![b'x'; 256 * 1024]).unwrap();
        append(&log, "kept", SystemTime::now());
        let kept = fs::metadata(&log).unwrap().len();
        assert_eq!(kept, (256 * 1024 + line("kept")) as u64);
        append(&log, "fresh", SystemTime::now());
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.ends_with(" fresh\n") && text.len() == line("fresh"), "{text:?}");
    }

    /// A culture under a listed one takes its separator, whatever the case of its name.
    #[test]
    fn a_culture_takes_its_parents_separator() {
        for (culture, sep) in [
            ("da", "."),
            ("da-DK", "."),
            ("DA-dk", "."),
            ("su-Latn-ID", "."),
            ("en-DK", "."),
            ("en-US", ":"),
            ("en", ":"),
            ("ms-MY", ":"),
            ("fr-CA", ":"),
            ("", ":"),
            ("xx-YY", ":"),
        ] {
            assert_eq!(culture_time_separator(culture), sep, "{culture:?}");
        }
    }

    /// The POSIX locale's name without its code set or modifier; C and POSIX are the invariant culture.
    #[cfg(not(windows))]
    #[test]
    fn a_locale_names_its_culture() {
        for (locale, culture) in [
            ("da_DK.UTF-8", "da-DK"),
            ("en_DK.utf8@euro", "en-DK"),
            ("fi", "fi"),
            ("C.UTF-8", ""),
            ("POSIX", ""),
            ("", ""),
        ] {
            assert_eq!(super::culture(locale), culture, "{locale:?}");
        }
    }

    /// .NET's separator out of a Windows time format, quotes and escapes and all.
    #[cfg(windows)]
    #[test]
    fn a_windows_time_format_gives_its_separator() {
        use windows_format::{reescape, separator};
        for (format, sep) in [
            ("HH:mm:ss", ":"),
            ("H.mm.ss", "."),
            ("hh:mm:ss tt", ":"),
            ("tt h:mm:ss", ":"),
            ("HH 'h' mm", " h "),
            ("HH'''h'mm", "'h"),
            ("HH\\mm", "\\"),
            ("HHmmss", ""),
            ("HH", ""),
            ("tt", ""),
            ("", ""),
        ] {
            assert_eq!(separator(&reescape(format)), sep, "{format:?}");
        }
    }
}

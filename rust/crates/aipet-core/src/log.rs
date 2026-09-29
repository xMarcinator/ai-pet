//! `aipet.log`, deleted past 256 KB (`Log.Write`, `src/AiPet.Core/Platform.cs`).
//!
//! One line per call, `HH:mm:ss.fff <line>` in local time, appended as UTF-8. The C# writes its culture's time
//! separator, which is `:` in all but a few cultures; this always writes `:`. Logging never fails its caller: a
//! line that can't be written is lost, as in the C#.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
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
        let text = format!("{h:02}:{m:02}:{s:02}.{ms:03} {line}\n");
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())
    })();
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

    /// `HH:mm:ss.fff line`, the milliseconds cut.
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
        assert_eq!(lines[0], format!("{h:02}:{m:02}:{s:02}.999 one"));
        assert!(text.ends_with(" two\n"));
    }

    /// The log is deleted only once it is past 256 KB.
    #[test]
    fn the_log_rotates_past_256_kb() {
        let d = dir("rotate");
        let log = d.0.join("aipet.log");
        fs::write(&log, vec![b'x'; 256 * 1024]).unwrap();
        append(&log, "kept", SystemTime::now());
        let kept = fs::metadata(&log).unwrap().len();
        assert_eq!(kept, 256 * 1024 + "00:00:00.000 kept\n".len() as u64);
        append(&log, "fresh", SystemTime::now());
        let text = fs::read_to_string(&log).unwrap();
        assert!(
            text.ends_with(" fresh\n") && text.len() == "00:00:00.000 fresh\n".len(),
            "{text:?}"
        );
    }
}

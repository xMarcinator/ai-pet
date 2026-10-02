//! The wire protocol, version 1: its limits, field names and time format (`src/AiPet.Core/Ipc.cs:27-57`).
//!
//! One request per connection. The client writes one UTF-8 JSON line ending in `\n`, the pet answers with one line,
//! and both close. The answer is what tells the hook the pet has its event.
//!
//! ```text
//! {"v":1,"type":"event","agent":"claude"|"codex","at":..,"pid":..,"env":{..},"payload":{..},"sent":..}
//!     -> {"ok":true,"outcome":"<state>|ignored|stale|removed"}
//! {"v":1,"type":"ping"} -> {"ok":true,"app":"AiPet","pid":..,"recent":["<hook-events.log line>", ..]}
//! ```
//!
//! `at` is when the hook started (before stdin was read), `sent` when it sent the event: Unix seconds with ms
//! precision ([`unix_time`], written as [`format_seconds`] does). `env` is Claude's only ([`CLAUDE_ENV`]).
//!
//! The contract is frozen: the Rust and the .NET hooks and pets talk to each other through it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `v`: the only version there is.
pub const VERSION: i32 = 1;

/// A request's size limit, in bytes of its line without the `\n`.
pub const MAX_REQUEST: usize = 4 << 20;
/// How long either end waits for the other: the hook for its write and the reply together, the pet for a request.
pub const TIMEOUT: Duration = Duration::from_millis(2000);
/// The pipe buffers on Windows, big enough that a usual request is written without waiting, and the longest reply
/// the hook reads.
pub const BUFFER_SIZE: usize = 64 << 10;

/// The longest a hook waits for a free instance of a pipe that exists (all taken: a burst of events, or a pet busy
/// for a moment; on Unix, a pet that doesn't take connections).
pub const CONNECT: Duration = Duration::from_millis(2500);
/// The budget of a hook's whole run from the start of `main`. Claude gives a hook 5 s: stdin ([`STDIN`]), then a
/// free instance (what the budget has left once the reply's [`TIMEOUT`] is set aside, at most [`CONNECT`]), then the
/// reply. That leaves 500 ms for starting it (bash, sh and the launcher for the plugin).
pub const HOOK_BUDGET: Duration = Duration::from_millis(4500);
/// How long a hook waits for its event on stdin.
pub const STDIN: Duration = Duration::from_millis(2000);

/// The longest string of an event the hook passes on, in UTF-16 code units (Write's `tool_input` holds the whole
/// file, say).
pub const MAX_STRING: usize = 256 << 10;
/// How deep a request may nest: an event at the hook's own parse limit (64) is one level deeper in its envelope.
pub const MAX_DEPTH: usize = 128;
/// The only fields of `tool_input` the pet reads (which file, skill or patch): all the hook keeps of a tool call
/// that is still over [`MAX_REQUEST`] once its strings are cut.
pub const TOOL_INPUT_KEYS: [&str; 4] = ["file_path", "notebook_path", "skill", "command"];

/// Claude's environment variables that say where a chat runs; the hook passes them in `env`.
pub const CLAUDE_ENV: [&str; 2] = ["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_HOST_SESSION_ID"];

// The envelope's and the replies' field names.
pub const V: &str = "v";
pub const KIND: &str = "type";
pub const AGENT: &str = "agent";
pub const AT: &str = "at";
pub const SENT: &str = "sent";
pub const PID: &str = "pid";
pub const ENV: &str = "env";
pub const PAYLOAD: &str = "payload";
pub const OK: &str = "ok";
pub const OUTCOME: &str = "outcome";
pub const ERROR: &str = "error";
pub const APP: &str = "app";
pub const RECENT: &str = "recent";

/// `Ipc.UnixTime`: Unix seconds with ms precision. The milliseconds are rounded down (also before 1970, as
/// `DateTimeOffset.ToUnixTimeMilliseconds` does) and then divided by 1000.0, so the double is the C#'s to the bit.
pub fn unix_time(t: SystemTime) -> f64 {
    let ms = match t.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_millis() as i64,
        Err(before) => -(before.duration().as_nanos().div_ceil(1_000_000) as i64),
    };
    ms as f64 / 1000.0
}

/// A double as .NET writes it, both with `ToString(CultureInfo.InvariantCulture)` (the hook's `sent`) and in
/// System.Text.Json (its `at`): the shortest text that reads back as the same double, in E-notation only from 1e17
/// on or below 0.0001. serde_json differs: it writes `1758800000.0` for a whole second, where .NET writes
/// `1758800000`.
pub fn format_seconds(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // Rust's own shortest digits, as d.ddde<exp>
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} writes an exponent");
    let exp: i32 = exp.parse().expect("{:e} writes an integer exponent");
    // .NET's Number.FormatGeneral: the decimal point sits `exp + 1` digits in. Past 17 digits (a double's most) or
    // more than 3 zeros after the point, it switches to E-notation, with a sign and at least two exponent digits.
    let point = exp + 1;
    if !(-3..=17).contains(&point) {
        let exp_sign = if exp < 0 { '-' } else { '+' };
        format!("{sign}{mantissa}E{exp_sign}{:02}", exp.unsigned_abs())
    } else {
        format!("{sign}{}", x.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_time_keeps_whole_milliseconds() {
        let t = UNIX_EPOCH + Duration::new(1_758_800_000, 123_999_999);
        assert_eq!(unix_time(t).to_bits(), (1_758_800_000_123_f64 / 1000.0).to_bits());
        // before 1970 the milliseconds round down too: -0.0005 s is -1 ms
        assert_eq!(unix_time(UNIX_EPOCH - Duration::from_micros(500)), -0.001);
        assert_eq!(unix_time(UNIX_EPOCH - Duration::from_millis(1)), -0.001);
        assert_eq!(unix_time(UNIX_EPOCH), 0.0);
    }

    /// Each expected text is what .NET 10 prints for the same double (`d.ToString(CultureInfo.InvariantCulture)`,
    /// and System.Text.Json's writer, which agree).
    #[test]
    fn seconds_are_written_as_dotnet_writes_doubles() {
        for (x, net) in [
            (1_758_800_000.123, "1758800000.123"),
            (1_758_800_000.12, "1758800000.12"),
            (1_758_800_000.0, "1758800000"),
            (1_758_800_000_123_f64 / 1000.0, "1758800000.123"),
            (0.001, "0.001"),
            (0.0001, "0.0001"),
            (0.00001, "1E-05"),
            (-0.001, "-0.001"),
            (0.0, "0"),
            (-0.0, "-0"),
            (0.00012345, "0.00012345"),
            (4.35e-5, "4.35E-05"),
            (5e-324, "5E-324"),
            (1e15, "1000000000000000"),
            (1e16, "10000000000000000"),
            (9.999999999999998e16, "99999999999999980"),
            (1e17, "1E+17"),
            (1.5e17, "1.5E+17"),
            (1234567890123456.7, "1234567890123456.8"),
            (1.2345678901234568e17, "1.2345678901234568E+17"),
            (1.7976931348623157e308, "1.7976931348623157E+308"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(format_seconds(x), net, "{x:e}");
        }
    }

    /// `,"sent":1758800000.123` is the 22 bytes the hook sets aside for `sent` (Program.cs:178-191).
    #[test]
    fn sent_fits_its_22_bytes() {
        let sent = format!(",\"{SENT}\":{}", format_seconds(1_758_800_000.123));
        assert_eq!(sent.len(), 22);
    }
}

//! What Jira, GitHub and the presets share: the HTTP client, with the C#'s 20 s timeout, the watchers' polling
//! threads, and the .NET rules the C# reads addresses and answers by, so the error texts are the C#'s:
//! - [`Http`]: the C#'s shared `HttpClient`. It sends only the headers the C# sets (no User-Agent, Accept or
//!   Accept-Encoding of its own), follows up to 50 redirects without the credentials, and reads 4xx and 5xx answers.
//! - [`Failure`]: how a request failed, as the C#'s `catch` blocks tell them apart: a timeout
//!   (`TaskCanceledException`), a server that couldn't be reached (`HttpRequestException`, with .NET's message), or
//!   anything else.
//! - [`Poller`]: a watcher's loop on a thread of its own, with a stop flag.
//! - [`Element`]: a JSON value read as `JsonElement` reads it, each step failing with .NET's exception text.
//! - [`unix_seconds`]: `DateTimeOffset.TryParse` of an ISO 8601 time, then `ToUnixTimeMilliseconds() / 1000.0`.
//!
//! .NET's own words after the C#'s texts are reproduced where the cause is common: a host or port that can't be
//! reached (the OS's text, then `(host:port)`), a TLS failure, a body that is empty or can't start a JSON value (an
//! HTML page, say). Rarer causes keep the C#'s text before the colon, with a message of their own after it.

use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use ureq::config::ConfigBuilder;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::typestate::AgentScope;

/// The C#'s `HttpClient.Timeout` for Jira and GitHub: the whole request, answer included.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// `HttpClientHandler.MaxAutomaticRedirections`; after that many, the last answer is read as it is.
const REDIRECTS: u32 = 50;

/// The most of an answer that is read. .NET buffers up to 2 GB; Jira's 25 issues and GitHub's 30 pull requests with
/// their commits are a few megabytes at most.
const LIMIT: u64 = 64 * 1024 * 1024;

/// The HTTP client Jira and GitHub share. Cloning it shares its connections.
#[derive(Clone, Debug)]
pub struct Http {
    agent: Agent,
    /// Plain HTTP to this computer only, for a stub server in tests.
    local: bool,
}

impl Http {
    /// HTTPS, trusting the OS's certificates and using its proxy settings as .NET's `HttpClient` does, with the
    /// C#'s 20 s timeout: what the app uses.
    pub fn new() -> Http {
        let config = Agent::config_builder()
            .https_only(true)
            .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
            .timeout_global(Some(TIMEOUT));
        Http {
            agent: like_dotnet(config).build().new_agent(),
            local: false,
        }
    }

    /// Plain HTTP to this computer only (a stub server standing in for Jira or GitHub), with no proxy and the given
    /// timeout. Jira and GitHub themselves are only reached through [`Http::new`]; this client refuses any other host,
    /// so a test can't reach them by mistake.
    pub fn local(timeout: Duration) -> Http {
        let config = Agent::config_builder().proxy(None).timeout_global(Some(timeout));
        Http {
            agent: like_dotnet(config).build().new_agent(),
            local: true,
        }
    }

    /// The scheme a watcher's address is reached with: `https`, or `http` for [`Http::local`].
    pub(crate) fn scheme(&self) -> &'static str {
        if self.local { "http" } else { "https" }
    }

    /// `GET url` with these headers.
    pub(crate) fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Answer, Failure> {
        self.send(url, headers, None)
    }

    /// `POST url` with these headers and body.
    pub(crate) fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> Result<Answer, Failure> {
        self.send(url, headers, Some(body))
    }

    fn send(&self, url: &str, headers: &[(&str, &str)], body: Option<&str>) -> Result<Answer, Failure> {
        let url = canonical(url);
        // the http crate takes a port too big for a u16 as none; .NET refuses it
        if port_text(&url).is_some_and(|port| !port.is_empty() && port.parse::<u16>().is_err()) {
            return Err(Failure::Other(INVALID_PORT.into()));
        }
        let uri = url.parse::<ureq::http::Uri>().ok();
        let Some(host) = uri.as_ref().and_then(|u| u.host()) else {
            return Err(Failure::Other(INVALID_HOST.into()));
        };
        let default_port = if url.starts_with("http:") { 80 } else { 443 };
        let port = uri.as_ref().and_then(|u| u.port_u16()).unwrap_or(default_port);
        if self.local && !loopback(host) {
            return Err(Failure::Other(format!(
                "{host} isn't this computer, and plain HTTP goes nowhere else"
            )));
        }
        let failed = |e: ureq::Error| failure(e, host, port);
        let mut response = match body {
            None => headers
                .iter()
                .fold(self.agent.get(&url), |r, (k, v)| r.header(*k, *v))
                .call(),
            Some(body) => headers
                .iter()
                .fold(self.agent.post(&url), |r, (k, v)| r.header(*k, *v))
                .send(body),
        }
        .map_err(failed)?;
        let status = response.status().as_u16();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_vec()
            .map_err(failed)?;
        // as `ReadAsStringAsync` reads it: by its byte order mark, else UTF-8
        Ok(Answer {
            status,
            body: crate::config::read_text(&bytes),
        })
    }
}

impl Default for Http {
    fn default() -> Self {
        Http::new()
    }
}

/// What the C#'s `new HttpClient()` does that ureq doesn't by default: read 4xx and 5xx answers, send no headers of
/// its own, and give the last answer after too many redirects.
fn like_dotnet(config: ConfigBuilder<AgentScope>) -> ConfigBuilder<AgentScope> {
    config
        .http_status_as_error(false)
        .user_agent("")
        .accept("")
        .accept_encoding("")
        .max_redirects(REDIRECTS)
        .max_redirects_will_error(false)
}

/// A server's answer.
#[derive(Debug)]
pub(crate) struct Answer {
    pub status: u16,
    pub body: String,
}

impl Answer {
    /// `IsSuccessStatusCode`.
    pub(crate) fn success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// `$"{(int)StatusCode} {ReasonPhrase}"`. .NET takes the phrase from the server's status line, which Jira and
    /// GitHub send as the standard one; this is the standard one, and nothing for a code that has none.
    pub(crate) fn status_line(&self) -> String {
        let reason = ureq::http::StatusCode::from_u16(self.status)
            .ok()
            .and_then(|s| s.canonical_reason())
            .unwrap_or("");
        format!("{} {reason}", self.status)
    }
}

/// How a request failed, as the C#'s `catch` blocks tell them apart.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// No answer within the timeout (`TaskCanceledException`).
    Timeout,
    /// The server couldn't be reached, or the connection failed (`HttpRequestException`), with .NET's message.
    Unreachable(String),
    /// Anything else, with its message.
    Other(String),
}

/// .NET's `HttpRequestException` for a TLS handshake that failed.
const TLS_FAILED: &str = "The SSL connection could not be established, see inner exception.";
/// .NET's `HttpRequestException` for a connection that failed after it was made.
const SENDING_FAILED: &str = "An error occurred while sending the request.";

fn failure(e: ureq::Error, host: &str, port: u16) -> Failure {
    use ureq::Error as E;
    match e {
        E::Timeout(_) => Failure::Timeout,
        E::Io(e) if e.kind() == io::ErrorKind::TimedOut => Failure::Timeout,
        // rustls reports a failed handshake (a certificate it doesn't trust, say) as invalid data
        E::Io(e) if e.kind() == io::ErrorKind::InvalidData => Failure::Unreachable(TLS_FAILED.into()),
        E::Io(e) => Failure::Unreachable(match connect_error(&e) {
            // SocketsHttpHandler: $"{error.Message} ({host}:{port})"
            Some(text) => format!("{text} ({host}:{port})"),
            None => SENDING_FAILED.into(),
        }),
        E::HostNotFound => Failure::Unreachable(format!("{} ({host}:{port})", host_not_found())),
        E::Tls(_) | E::Rustls(_) | E::Pem(_) | E::TlsRequired => Failure::Unreachable(TLS_FAILED.into()),
        E::BadUri(_) => Failure::Other(INVALID_HOST.into()),
        E::Http(e) => Failure::Other(e.to_string()),
        E::BodyExceedsLimit(limit) => Failure::Other(format!(
            "Cannot write more bytes to the buffer than the configured maximum buffer size: {limit}."
        )),
        _ => Failure::Unreachable(SENDING_FAILED.into()),
    }
}

/// The OS's text for an error while connecting (a host that isn't known, a port nobody listens on), which .NET's
/// `SocketException` carries too; `None` for an error after the connection was made.
fn connect_error(e: &io::Error) -> Option<String> {
    use io::ErrorKind as K;
    // std's words before a failed getaddrinfo's own on Unix, which are what .NET shows
    const LOOKUP: &str = "failed to lookup address information: ";
    let text = os_text(e);
    if let Some(reason) = text.strip_prefix(LOOKUP) {
        return Some(reason.into());
    }
    // Windows Sockets' name resolution errors, WSAHOST_NOT_FOUND to WSANO_DATA
    let resolving = cfg!(windows) && matches!(e.raw_os_error(), Some(11001..=11004));
    let connecting = matches!(
        e.kind(),
        K::ConnectionRefused | K::HostUnreachable | K::NetworkUnreachable | K::NetworkDown | K::AddrNotAvailable
    );
    (resolving || connecting).then_some(text)
}

/// An error's text without std's ` (os error N)`, as .NET words it.
fn os_text(e: &io::Error) -> String {
    let text = e.to_string();
    match e.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&text)
            .into(),
        None => text,
    }
}

/// The OS's text for a host name that isn't known.
fn host_not_found() -> String {
    if cfg!(windows) {
        os_text(&io::Error::from_raw_os_error(11001))
    } else {
        "Name or service not known".into()
    }
}

/// `UriFormatException`'s messages for an address .NET can't make a URI of.
const INVALID_HOST: &str = "Invalid URI: The hostname could not be parsed.";
const INVALID_PORT: &str = "Invalid URI: Invalid port specified.";

/// The port as the URL writes it, if it writes one.
fn port_text(url: &str) -> Option<&str> {
    let authority = url
        .split_once("://")
        .map_or("", |(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""));
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    match host_port.rsplit_once(']') {
        Some((_, after)) => after.strip_prefix(':'),
        None => host_port.split_once(':').map(|(_, port)| port),
    }
}

/// The URL as .NET's `Uri` sends it: the host in lower case, the rest as it is.
fn canonical(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.into();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let at = authority.rfind('@').map_or(0, |i| i + 1);
    let (user, host) = authority.split_at(at);
    format!("{scheme}://{user}{}{tail}", host.to_ascii_lowercase())
}

fn loopback(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || bare.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// A Jira site or GitHub host as the C# reads one (and a preset compares them): trimmed, without `https://` or
/// `http://` anywhere, and without trailing slashes.
pub(crate) fn address(setting: &str) -> String {
    setting
        .trim()
        .replace("https://", "")
        .replace("http://", "")
        .trim_end_matches('/')
        .into()
}

/// `Uri.EscapeDataString`: every UTF-8 byte but the unreserved characters as `%XX`.
pub(crate) fn escape_data_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `Convert.ToBase64String`.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------
// The watchers' polling threads

/// A watcher's loop: a thread that polls, then waits, until the next start stops it (the C#'s `Restart`, with a
/// stop flag in place of its `CancellationTokenSource`). A stopped loop finishes a poll it is in, as the C#'s does.
#[derive(Debug, Default)]
pub(crate) struct Poller(Mutex<Option<Arc<Stop>>>);

impl Poller {
    /// Stops the running loop and starts another: `poll` after `first`, then again after each wait `poll` returns.
    pub(crate) fn restart(&self, name: &str, first: Duration, poll: impl Fn() -> Duration + Send + 'static) {
        let stop = Arc::new(Stop::default());
        if let Some(old) = lock(&self.0).replace(Arc::clone(&stop)) {
            old.stop();
        }
        // without a thread the watcher only stays still, as it does before its first start
        let _ = thread::Builder::new().name(name.into()).spawn(move || {
            let mut wait = first;
            while !stop.wait(wait) {
                wait = poll();
            }
        });
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        if let Some(running) = lock(&self.0).take() {
            running.stop();
        }
    }
}

/// A loop's stop flag.
#[derive(Debug, Default)]
struct Stop {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl Stop {
    fn stop(&self) {
        *lock(&self.stopped) = true;
        self.wake.notify_all();
    }

    /// Waits `time`, less if the loop is stopped; whether it is.
    fn wait(&self, time: Duration) -> bool {
        let stopped = lock(&self.stopped);
        let (stopped, _) = self
            .wake
            .wait_timeout_while(stopped, time, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *stopped
    }
}

/// A lock, whatever a thread that panicked holding it left behind.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------------------------------------------
// JSON, as System.Text.Json's JsonDocument and JsonElement read it

/// .NET's `KeyNotFoundException`, from `GetProperty`.
const KEY_NOT_FOUND: &str = "The given key was not present in the dictionary.";
/// .NET's `FormatException`, from `GetInt32` of a number that isn't a whole `int`.
const NOT_AN_INT: &str = "One of the identified items was in an invalid format.";
/// .NET's `NullReferenceException`.
pub(crate) const NULL_REFERENCE: &str = "Object reference not set to an instance of an object.";

/// `JsonDocument.Parse(text)`: the document, or the exception's text. .NET's own words for a body with nothing in it
/// and one that can't start a JSON value (an HTML page, say); serde_json's for the rarer failures.
pub(crate) fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| dotnet_parse_error(text).unwrap_or_else(|| e.to_string()))
}

fn dotnet_parse_error(text: &str) -> Option<String> {
    let (mut line, mut column) = (0, 0);
    for b in text.bytes() {
        match b {
            b'\n' => (line, column) = (line + 1, 0),
            b' ' | b'\t' | b'\r' => column += 1,
            b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n' => return None,
            _ => {
                let shown = if b.is_ascii_graphic() {
                    char::from(b).to_string()
                } else {
                    format!("0x{b:02X}")
                };
                return Some(format!(
                    "'{shown}' is an invalid start of a value. LineNumber: {line} | BytePositionInLine: {column}."
                ));
            }
        }
    }
    Some(format!(
        "The input does not contain any JSON tokens. Expected the input to start with a valid JSON token, when \
         isFinalBlock is true. LineNumber: {line} | BytePositionInLine: {column}."
    ))
}

/// A JSON value read as the C# reads a `JsonElement`: a step that doesn't fit fails with .NET's exception text.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Element<'a>(pub &'a Value);

impl<'a> Element<'a> {
    /// `JsonValueKind`'s name.
    fn kind(self) -> &'static str {
        match self.0 {
            Value::Null => "Null",
            Value::Bool(true) => "True",
            Value::Bool(false) => "False",
            Value::Number(_) => "Number",
            Value::String(_) => "String",
            Value::Array(_) => "Array",
            Value::Object(_) => "Object",
        }
    }

    fn wrong(self, wanted: &str) -> String {
        format!(
            "The requested operation requires an element of type '{wanted}', but the target element has type '{}'.",
            self.kind()
        )
    }

    /// `TryGetProperty`: the member, if there is one; the last of a name that comes twice.
    pub(crate) fn try_get(self, name: &str) -> Result<Option<Element<'a>>, String> {
        match self.0 {
            Value::Object(members) => Ok(members.get(name).map(Element)),
            _ => Err(self.wrong("Object")),
        }
    }

    /// `GetProperty`.
    pub(crate) fn get(self, name: &str) -> Result<Element<'a>, String> {
        self.try_get(name)?.ok_or_else(|| KEY_NOT_FOUND.into())
    }

    /// `GetString`: `None` for null.
    pub(crate) fn string(self) -> Result<Option<&'a str>, String> {
        match self.0 {
            Value::Null => Ok(None),
            Value::String(s) => Ok(Some(s)),
            _ => Err(self.wrong("String")),
        }
    }

    /// `EnumerateArray`, and `GetArrayLength`.
    pub(crate) fn array(self) -> Result<&'a [Value], String> {
        match self.0 {
            Value::Array(items) => Ok(items),
            _ => Err(self.wrong("Array")),
        }
    }

    /// `GetInt32`.
    pub(crate) fn int32(self) -> Result<i32, String> {
        match self.0 {
            Value::Number(n) => n
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| NOT_AN_INT.into()),
            _ => Err(self.wrong("Number")),
        }
    }

    /// The C#'s `Str(e, name)`: the member's text when it is a string, else empty.
    pub(crate) fn text(self, name: &str) -> Result<String, String> {
        Ok(match self.try_get(name)? {
            Some(Element(Value::String(s))) => s.clone(),
            _ => String::new(),
        })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Times

const TICKS_PER_SECOND: i64 = 10_000_000;

/// `DateTimeOffset.TryParse(text, out var t) ? t.ToUnixTimeMilliseconds() / 1000.0 : 0`, for the ISO 8601 times Jira
/// and GitHub send: `2026-09-29T14:03:12.345+0200`, `2026-09-29T12:03:12Z` and the variants .NET reads too (a space
/// for the `T`, a comma for the point, an offset of `+02`, `+2:00` or `-0930`, no seconds, spaces around). A time
/// without an offset is read as UTC, where .NET takes the computer's time zone; Jira and GitHub always send one.
pub(crate) fn unix_seconds(text: &str) -> Option<f64> {
    let mut p = Reader(text.trim().as_bytes());
    let (year, month, day) = (p.number(4)?, p.then(b'-', 2)?, p.then(b'-', 2)?);
    let (mut hour, mut minute, mut second, mut ticks) = (0, 0, 0, 0);
    let spaced = p.skip_spaces();
    if spaced || p.take(|b| matches!(b, b'T' | b't')) {
        (hour, minute) = (p.number(2)?, p.then(b':', 2)?);
        if p.take(|b| b == b':') {
            second = p.number(2)?;
            if p.take(|b| matches!(b, b'.' | b',')) {
                ticks = p.fraction()?;
            }
        }
    }
    p.skip_spaces();
    let offset = p.offset()?;
    let valid_date = (1..=12).contains(&month) && (1..=days_in(year, month)).contains(&day);
    if !p.0.is_empty() || !valid_date || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second - offset * 60;
    let utc = seconds * TICKS_PER_SECOND + ticks;
    // DateTimeOffset's range: the years 1 to 9999, in UTC
    let first = days_from_civil(1, 1, 1) * 86_400 * TICKS_PER_SECOND;
    let past_last = days_from_civil(10_000, 1, 1) * 86_400 * TICKS_PER_SECOND;
    (first..past_last)
        .contains(&utc)
        .then(|| utc.div_euclid(10_000) as f64 / 1000.0)
}

/// What is left of the text being read.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    /// Exactly `n` ASCII digits.
    fn number(&mut self, n: usize) -> Option<i64> {
        let digits = self.0.get(..n).filter(|d| d.iter().all(u8::is_ascii_digit))?;
        self.0 = &self.0[n..];
        Some(digits.iter().fold(0, |v, d| v * 10 + i64::from(d - b'0')))
    }

    /// `separator`, then exactly `n` digits.
    fn then(&mut self, separator: u8, n: usize) -> Option<i64> {
        if !self.take(|b| b == separator) {
            return None;
        }
        self.number(n)
    }

    /// Takes the next byte if it is one of these.
    fn take(&mut self, wanted: impl Fn(u8) -> bool) -> bool {
        match self.0.split_first() {
            Some((&b, rest)) if wanted(b) => {
                self.0 = rest;
                true
            }
            _ => false,
        }
    }

    fn skip_spaces(&mut self) -> bool {
        let mut any = false;
        while self.take(|b| b == b' ') {
            any = true;
        }
        any
    }

    /// A second's fraction, at least one digit, in ticks: rounded to the nearest tick, a half to even, as .NET's
    /// `Math.Round` does.
    fn fraction(&mut self) -> Option<i64> {
        let n = self.0.iter().take_while(|b| b.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        let (digits, rest) = self.0.split_at(n);
        self.0 = rest;
        let mut ticks = (0..7).fold(0, |t, i| t * 10 + digits.get(i).map_or(0, |d| i64::from(d - b'0')));
        if let Some((&next, beyond)) = digits.get(7..).and_then(|d| d.split_first()) {
            let exactly_half = next == b'5' && beyond.iter().all(|&d| d == b'0');
            if next >= b'5' && !(exactly_half && ticks % 2 == 0) {
                ticks += 1;
            }
        }
        Some(ticks)
    }

    /// The offset from UTC in minutes: `Z`, `+HH:MM`, `+HHMM`, `+HH` or `+H:MM`; none is UTC. At most 14 hours.
    fn offset(&mut self) -> Option<i64> {
        if self.0.is_empty() || self.take(|b| matches!(b, b'Z' | b'z')) {
            return Some(0);
        }
        let sign = match self.0.first() {
            Some(b'+') => 1,
            Some(b'-') => -1,
            _ => return None,
        };
        self.0 = &self.0[1..];
        let hours = self.number(2).or_else(|| self.number(1))?;
        let colon = self.take(|b| b == b':');
        let minutes = if colon || !self.0.is_empty() {
            self.number(2)?
        } else {
            0
        };
        (minutes <= 59 && hours * 60 + minutes <= 14 * 60).then_some(sign * (hours * 60 + minutes))
    }
}

fn days_in(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days from 1970-01-01 to this date of the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ---------------------------------------------------------------------------------------------------------------
// Casing, by .NET's invariant and ordinal rules

/// `ToUpperInvariant`: each character's simple upper-case mapping, except ı, which .NET keeps.
pub(crate) fn upper_invariant(text: &str) -> String {
    text.chars().map(upper_char).collect()
}

/// A character as `ToUpperInvariant` maps it.
pub(crate) fn upper_char(c: char) -> char {
    if c == 'ı' { c } else { simple_upper(c) }
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`: each character's simple upper-case mapping, except ı
/// and ſ, whose upper cases are ASCII and which .NET's ordinal casing keeps.
pub(crate) fn equals_ignoring_case(a: &str, b: &str) -> bool {
    let upper = |c: char| if c == 'ı' || c == 'ſ' { c } else { simple_upper(c) };
    a.chars().map(upper).eq(b.chars().map(upper))
}

/// A character's simple upper-case mapping (UnicodeData.txt), which `char::to_uppercase` gives where it is a single
/// character; the same rule as `sessions::describe`'s.
fn simple_upper(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        // a full mapping of several characters (ß to SS, say): the simple one is a single character only for these
        // Greek letters with ypogegrammeni, which map to their title-case forms
        _ => match u32::from(c) {
            0x1F80..=0x1F87 | 0x1F90..=0x1F97 | 0x1FA0..=0x1FA7 => char::from_u32(u32::from(c) + 8).unwrap_or(c),
            0x1FB3 => '\u{1FBC}',
            0x1FC3 => '\u{1FCC}',
            0x1FF3 => '\u{1FFC}',
            _ => c,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_client_is_the_csharps() {
        let http = Http::new();
        let config = http.agent.config();
        assert_eq!(config.timeouts().global, Some(Duration::from_secs(20)));
        assert!(config.https_only());
        assert!(!config.http_status_as_error());
        assert_eq!(config.max_redirects(), 50);
        assert_eq!(http.scheme(), "https");
        assert_eq!(Http::local(TIMEOUT).scheme(), "http");
    }

    #[test]
    fn a_local_client_reaches_this_computer_only() {
        let http = Http::local(Duration::from_millis(200));
        for url in [
            "http://api.github.com/graphql",
            "http://team.example.net/rest",
            "http://10.0.0.1/",
        ] {
            let refused = matches!(http.get(url, &[]), Err(Failure::Other(m)) if m.contains("isn't this computer"));
            assert!(refused, "{url}");
        }
        assert!(loopback("127.0.0.1") && loopback("[::1]") && loopback("LocalHost"));
        assert!(!loopback("127.example"));
    }

    #[test]
    fn connection_errors_read_as_dotnets() {
        let at = |e: io::Error| failure(ureq::Error::Io(e), "team.example.net", 443);
        let refused = os_text(&io::Error::from(io::ErrorKind::ConnectionRefused));
        assert_eq!(
            at(io::Error::from(io::ErrorKind::ConnectionRefused)),
            Failure::Unreachable(format!("{refused} (team.example.net:443)"))
        );
        #[cfg(windows)]
        {
            let text = io::Error::from_raw_os_error(11001).to_string();
            let text = text.strip_suffix(" (os error 11001)").unwrap();
            assert_eq!(
                at(io::Error::from_raw_os_error(11001)),
                Failure::Unreachable(format!("{text} (team.example.net:443)"))
            );
        }
        let lookup = io::Error::other("failed to lookup address information: Name or service not known");
        assert_eq!(
            at(lookup),
            Failure::Unreachable("Name or service not known (team.example.net:443)".into())
        );
        assert_eq!(at(io::Error::from(io::ErrorKind::TimedOut)), Failure::Timeout);
        assert_eq!(
            at(io::Error::from(io::ErrorKind::InvalidData)),
            Failure::Unreachable(TLS_FAILED.into())
        );
        assert_eq!(
            at(io::Error::from(io::ErrorKind::UnexpectedEof)),
            Failure::Unreachable(SENDING_FAILED.into())
        );
        assert_eq!(
            failure(ureq::Error::Timeout(ureq::Timeout::Global), "h", 1),
            Failure::Timeout
        );
        assert_eq!(
            failure(ureq::Error::Tls("x"), "h", 1),
            Failure::Unreachable(TLS_FAILED.into())
        );
    }

    #[test]
    fn addresses_are_read_as_the_csharp_reads_them() {
        assert_eq!(address(" https://Team.example.net/ "), "Team.example.net");
        assert_eq!(address("http://ghe.example.com//"), "ghe.example.com");
        // every occurrence, https:// first
        assert_eq!(address("hthttps://tp://x"), "x");
        assert_eq!(
            canonical("https://Team.Example.NET:8443/Jira?q=A"),
            "https://team.example.net:8443/Jira?q=A"
        );
        assert_eq!(canonical("https://Me@Host/x"), "https://Me@host/x");
        assert_eq!(port_text("https://u:p@[::1]:8443/rest"), Some("8443"));
        assert_eq!(port_text("https://localhost:99999/rest?a=b:c"), Some("99999"));
        assert_eq!(port_text("https://team.example.net/rest:1"), None);
        // UriFormatException's messages, as recorded from .NET 10
        let http = Http::local(Duration::from_millis(200));
        for (url, message) in [
            ("http://my site/rest", INVALID_HOST),
            ("http:///rest", INVALID_HOST),
            ("http://localhost:99999/rest", INVALID_PORT),
        ] {
            assert_eq!(http.get(url, &[]).unwrap_err(), Failure::Other(message.into()), "{url}");
        }
    }

    #[test]
    fn escaping_and_base64_are_dotnets() {
        // Uri.EscapeDataString and Convert.ToBase64String, as recorded from .NET 10
        assert_eq!(
            escape_data_string("a b()!*'~-_.é/?&=+#"),
            "a%20b%28%29%21%2A%27~-_.%C3%A9%2F%3F%26%3D%2B%23"
        );
        assert_eq!(base64(b"me@example.com:t0k3n"), "bWVAZXhhbXBsZS5jb206dDBrM24=");
        assert_eq!(base64(b"me@example.com:t"), "bWVAZXhhbXBsZS5jb206dA==");
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn json_failures_read_as_dotnets() {
        // JsonDocument.Parse's and JsonElement's texts, as recorded from .NET 10
        let invalid = |start: &str, line: u32, column: u32| {
            format!("'{start}' is an invalid start of a value. LineNumber: {line} | BytePositionInLine: {column}.")
        };
        assert_eq!(parse_json("<html>").unwrap_err(), invalid("<", 0, 0));
        assert_eq!(parse_json(" \n <x").unwrap_err(), invalid("<", 1, 1));
        assert_eq!(parse_json("é").unwrap_err(), invalid("0xC3", 0, 0));
        assert_eq!(
            parse_json("   ").unwrap_err(),
            "The input does not contain any JSON tokens. Expected the input to start with a valid JSON token, when \
             isFinalBlock is true. LineNumber: 0 | BytePositionInLine: 3."
        );
        let wrong = |wanted: &str, actual: &str| {
            format!(
                "The requested operation requires an element of type '{wanted}', but the target element has type \
                 '{actual}'."
            )
        };
        let doc: Value = serde_json::from_str(r#"{"a":null,"n":1.5,"s":"x","i":12,"d":1,"d":2}"#).unwrap();
        let root = Element(&doc);
        assert_eq!(root.get("b").unwrap_err(), KEY_NOT_FOUND);
        assert_eq!(root.get("a").unwrap().get("x").unwrap_err(), wrong("Object", "Null"));
        assert_eq!(root.get("n").unwrap().int32().unwrap_err(), NOT_AN_INT);
        assert_eq!(root.get("s").unwrap().int32().unwrap_err(), wrong("Number", "String"));
        assert_eq!(root.get("n").unwrap().string().unwrap_err(), wrong("String", "Number"));
        assert_eq!(root.get("s").unwrap().array().unwrap_err(), wrong("Array", "String"));
        assert_eq!(root.get("i").unwrap().int32(), Ok(12));
        assert_eq!(root.get("a").unwrap().string(), Ok(None));
        assert_eq!(root.text("s").unwrap(), "x");
        assert_eq!(root.text("n").unwrap(), "");
        // a name that comes twice: the last one
        assert_eq!(root.get("d").unwrap().int32(), Ok(2));
    }

    #[test]
    fn times_read_as_dotnets() {
        // DateTimeOffset.TryParse(…).ToUnixTimeMilliseconds() / 1000.0, as recorded from .NET 10
        for (text, seconds) in [
            ("2026-09-29T14:03:12.345+0200", Some(1790683392.345)),
            ("2026-09-29T12:03:12Z", Some(1790683392.0)),
            ("2026-09-29T14:03:12.3456789+02:00", Some(1790683392.345)),
            ("2026-09-29T14:03:12.99999999+02:00", Some(1790683393.0)),
            ("2026-09-29T14:03:12+02", Some(1790683392.0)),
            ("2026-09-29 14:03:12Z", Some(1790690592.0)),
            (" 2026-09-29T14:03Z ", Some(1790690580.0)),
            ("2026-09-29T14:03:12-0930", Some(1790724792.0)),
            ("2026-09-29T14:03:12.5z", Some(1790690592.5)),
            ("2026-09-29T14:03:12,5Z", Some(1790690592.5)),
            ("2026-09-29T14:03:12+2:00", Some(1790683392.0)),
            ("2026-09-29T14:03:12 +02:00", Some(1790683392.0)),
            ("2026-09-29T14:03:12.1234567890123+00:00", Some(1790690592.123)),
            ("1969-12-31T23:59:59.9995Z", Some(-0.001)),
            ("0001-01-01T00:00:00Z", Some(-62135596800.0)),
            ("9999-12-31T23:59:59.9999999Z", Some(253402300799.999)),
            ("2026-02-29T00:00:00Z", None),
            ("2026-09-29T24:00:00Z", None),
            ("2026-09-29T14:03:60Z", None),
            ("2026-09-29T14:03:12+14:01", None),
            ("2026-9-9T4:3:2Z", None),
            ("2026-09-29T14:03:12.Z", None),
            ("0001-01-01T00:00:00+01:00", None),
            ("", None),
            ("yesterday", None),
            // no offset: UTC here, the computer's time zone for .NET
            ("2026-09-29T14:03:12", Some(1790690592.0)),
        ] {
            assert_eq!(unix_seconds(text), seconds, "{text}");
        }
    }

    #[test]
    fn casing_is_dotnets() {
        // ToUpperInvariant and OrdinalIgnoreCase, as recorded from .NET 10
        assert_eq!(upper_invariant("abc ı ſ ß é ǅ ᾀ"), "ABC ı S ß É Ǆ ᾈ");
        assert!(equals_ignoring_case("github.com", "GitHub.com"));
        assert!(!equals_ignoring_case("evıl.example", "EVIL.example"));
        assert!(!equals_ignoring_case("ſite", "SITE"));
        assert!(!equals_ignoring_case("Straße", "STRASSE"));
    }

    #[test]
    fn status_lines_name_the_standard_reason() {
        let line = |status| {
            Answer {
                status,
                body: String::new(),
            }
            .status_line()
        };
        assert_eq!(line(403), "403 Forbidden");
        assert_eq!(line(429), "429 Too Many Requests");
        assert_eq!(line(599), "599 ");
    }
}

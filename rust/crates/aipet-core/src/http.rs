//! What Jira, GitHub and the presets share: the HTTP client, with the C#'s 20 s timeout, the watchers' polling
//! threads, and the .NET rules the C# reads addresses and answers by, so the error texts are the C#'s:
//! - [`Http`]: the C#'s shared `HttpClient`. It sends only the headers the C# sets (no User-Agent, Accept or
//!   Accept-Encoding of its own), follows up to 50 redirects without the credentials, reads 4xx and 5xx answers, and
//!   keeps each answer's reason phrase as the server sent it (ureq drops it; .NET's `ReasonPhrase` has it).
//! - [`Failure`]: how a request failed, as the C#'s `catch` blocks tell them apart: a timeout
//!   (`TaskCanceledException`), a server that couldn't be reached (`HttpRequestException`, with .NET's message), or
//!   anything else.
//! - [`Poller`]: a watcher's loop on a thread of its own, with a stop flag.
//! - [`parse_json`]: `JsonDocument.Parse`, which refuses what .NET's reader refuses, with its message and position.
//! - [`Element`]: a JSON value read as `JsonElement` reads it, each step failing with .NET's exception text.
//! - [`unix_seconds`]: `DateTimeOffset.TryParse` of an ISO 8601 time, then `ToUnixTimeMilliseconds() / 1000.0`.
//!
//! .NET's own words after the C#'s texts are reproduced where the cause is common: a host or port that can't be
//! reached (the OS's text, then `(host:port)`), a TLS failure, a body that isn't JSON. Rarer causes keep the C#'s
//! text before the colon, with a message of their own after it.

use std::cell::RefCell;
use std::fmt;
use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use ureq::config::{Config, ConfigBuilder};
use ureq::http::Uri;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::typestate::AgentScope;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport};

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
            agent: agent(like_dotnet(config).build(), DefaultResolver::default()),
            local: false,
        }
    }

    /// Plain HTTP to this computer only (a stub server standing in for Jira or GitHub), with no proxy and the given
    /// timeout. Jira and GitHub themselves are only reached through [`Http::new`]; this client refuses any other host
    /// before it looks the name up, a redirect's included ([`ThisComputer`]), so a test can't reach them by mistake.
    pub fn local(timeout: Duration) -> Http {
        let config = Agent::config_builder().proxy(None).timeout_global(Some(timeout));
        Http {
            agent: agent(like_dotnet(config).build(), ThisComputer::default()),
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
        let failed = |e: ureq::Error| failure(e, host, port);
        // what an earlier request left there isn't this one's
        take_status_line();
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
        // the last status line read, a redirect's last hop's; a stray line (an interim 1xx, say) has another code
        let reason = take_status_line().and_then(|(code, reason)| (code == status).then_some(reason));
        let bytes = response
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_vec()
            .map_err(failed)?;
        // as `ReadAsStringAsync` reads it: by its byte order mark, else UTF-8
        Ok(Answer {
            status,
            reason,
            body: crate::config::read_text(&bytes),
        })
    }
}

/// An agent whose connections note each answer's status line ([`StatusLines`]), looking names up with `resolver`.
fn agent(config: Config, resolver: impl Resolver) -> Agent {
    Agent::with_parts(config, DefaultConnector::default().chain(StatusLines), resolver)
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
    /// The reason phrase of the answer's status line, as .NET reads it ([`read_status_line`]); `None` when the
    /// line wasn't read.
    pub reason: Option<String>,
    pub body: String,
}

impl Answer {
    /// `IsSuccessStatusCode`.
    pub(crate) fn success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// `$"{(int)StatusCode} {ReasonPhrase}"`: the phrase the server sent, empty when it sent none, as .NET keeps it.
    /// Should the line not have been read, the standard phrase, which is .NET's when it has none (and nothing for a
    /// code without one).
    pub(crate) fn status_line(&self) -> String {
        let standard = || {
            ureq::http::StatusCode::from_u16(self.status)
                .ok()
                .and_then(|s| s.canonical_reason())
                .unwrap_or("")
        };
        format!("{} {}", self.status, self.reason.as_deref().unwrap_or_else(standard))
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
        E::Other(e) => match e.downcast_ref::<NotThisComputer>() {
            Some(refused) => Failure::Other(refused.to_string()),
            None => Failure::Unreachable(SENDING_FAILED.into()),
        },
        _ => Failure::Unreachable(SENDING_FAILED.into()),
    }
}

/// Looks names up for [`Http::local`]: this computer's only. ureq asks before every connection it makes, a
/// redirect's included, so nothing [`Http::local`] sends leaves this computer, not even a name lookup.
#[derive(Debug, Default)]
struct ThisComputer(DefaultResolver);

impl Resolver for ThisComputer {
    fn resolve(&self, uri: &Uri, config: &Config, timeout: NextTimeout) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let host = uri.host().unwrap_or_default();
        if !loopback(host) {
            return Err(ureq::Error::Other(Box::new(NotThisComputer(host.into()))));
        }
        self.0.resolve(uri, config, timeout)
    }
}

/// [`Http::local`]'s refusal of a host that isn't this computer.
#[derive(Debug)]
struct NotThisComputer(String);

impl fmt::Display for NotThisComputer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} isn't this computer, and plain HTTP goes nowhere else", self.0)
    }
}

impl std::error::Error for NotThisComputer {}

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
// The status line's reason phrase, which .NET's `ReasonPhrase` keeps and ureq drops

thread_local! {
    /// The code and reason phrase of the last status line this thread read. ureq reads an answer on the thread
    /// that sent its request, so [`Http::send`] finds its answer's here.
    static STATUS_LINE: RefCell<Option<(u16, String)>> = const { RefCell::new(None) };
}

fn take_status_line() -> Option<(u16, String)> {
    STATUS_LINE.with(|line| line.borrow_mut().take())
}

/// The last connector of each agent's chain, after TLS, so it sees the answers as they are: it wraps every connection
/// in [`NotesStatusLines`]. The transport API is ureq's unversioned one, which may change in a minor version;
/// Cargo.lock pins the one this is written for.
#[derive(Debug)]
struct StatusLines;

impl Connector<Box<dyn Transport>> for StatusLines {
    type Out = NotesStatusLines;

    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<NotesStatusLines>, ureq::Error> {
        Ok(chained.map(|inner| NotesStatusLines { inner, awaiting: false }))
    }
}

/// A connection that notes the status line of each answer it reads: the first line in after a request went out.
#[derive(Debug)]
struct NotesStatusLines {
    inner: Box<dyn Transport>,
    /// A request went out, and its answer's first line hasn't come in yet.
    awaiting: bool,
}

impl Transport for NotesStatusLines {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.awaiting = true;
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let progress = self.inner.await_input(timeout)?;
        if self.awaiting {
            // what hasn't been read yet starts with the answer
            let input = self.inner.buffers().input();
            if !b"HTTP/".starts_with(&input[..input.len().min(5)]) {
                self.awaiting = false;
            } else if let Some(end) = input.iter().position(|&b| b == b'\n') {
                self.awaiting = false;
                let line = &input[..end];
                let line = read_status_line(line.strip_suffix(b"\r").unwrap_or(line));
                STATUS_LINE.with(|noted| *noted.borrow_mut() = line);
            }
        }
        Ok(progress)
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

/// A status line's code and reason phrase as .NET reads them: `HTTP/1.` and a digit, a space, three digits, then
/// nothing or a space and the phrase, kept as it is, its bytes as Latin-1. `None` for a line .NET refuses.
fn read_status_line(line: &[u8]) -> Option<(u16, String)> {
    let digits = line.get(9..12)?;
    if !line.starts_with(b"HTTP/1.")
        || !line[7].is_ascii_digit()
        || line[8] != b' '
        || !digits.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let code = digits.iter().fold(0, |code, d| code * 10 + u16::from(d - b'0'));
    let reason = match &line[12..] {
        [] => String::new(),
        [b' ', phrase @ ..] => phrase.iter().copied().map(char::from).collect(),
        _ => return None,
    };
    Some((code, reason))
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

/// `JsonDocument.Parse(text)`: the document, or the exception's text. Whether the text is JSON, and what is wrong
/// where when it isn't, is .NET's reader's call ([`JsonReader`]); what it reads and serde_json can't (a lone
/// surrogate's escape, a number beyond `f64`) keeps serde_json's words.
pub(crate) fn parse_json(text: &str) -> Result<Value, String> {
    if let Some(message) = JsonReader::syntax_error(text) {
        return Err(message);
    }
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// `Utf8JsonReader` as `JsonDocument.Parse` runs it over a whole text (the final block, the default options: no
/// comments, no trailing commas, at most 64 levels deep), for what it refuses: its checks in its order, its messages,
/// and the line and byte it names, counted as it counts them.
struct JsonReader<'a> {
    json: &'a [u8],
    /// The bytes read.
    at: usize,
    /// `LineNumber` and `BytePositionInLine`, from 0.
    line: usize,
    column: usize,
    token: Token,
    /// The objects (true) and arrays (false) open, the innermost last.
    open: Vec<bool>,
    /// The text is an object or an array, not a single value.
    not_primitive: bool,
}

/// The last token read, as far as it decides what may come next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    None,
    StartObject,
    StartArray,
    PropertyName,
    /// A value, or the end of an object or array.
    Other,
}

// .NET's messages; `{0}` is the byte they name
const NO_TOKENS: &str = "The input does not contain any JSON tokens. Expected the input to start with a valid JSON \
                         token, when isFinalBlock is true.";
const OPEN_AT_END: &str = "Expected depth to be zero at the end of the JSON payload. There is an open JSON object or \
                           array that should be closed.";
const START_OF_VALUE: &str = "'{0}' is an invalid start of a value.";
const START_OF_PROPERTY: &str = "'{0}' is an invalid start of a property name. Expected a '\"'.";
const AFTER_PROPERTY_NAME: &str = "'{0}' is invalid after a property name. Expected a ':'.";
const VALUE_AT_END: &str = "Expected a value, but instead reached end of data.";
const PROPERTY_OR_VALUE_AT_END: &str = "Expected start of a property name or value, but instead reached end of data.";
const AFTER_VALUE: &str = "'{0}' is invalid after a value. Expected either ',', '}', or ']'.";
const AFTER_SINGLE_VALUE: &str = "'{0}' is invalid after a single JSON value. Expected end of data.";
const NO_MATCHING_OPEN: &str = "'{0}' is invalid without a matching open.";
const TRAILING_COMMA_IN_OBJECT: &str = "The JSON object contains a trailing comma at the end which is not supported \
                                        in this mode. Change the reader options.";
const TRAILING_COMMA_IN_ARRAY: &str = "The JSON array contains a trailing comma at the end which is not supported in \
                                       this mode. Change the reader options.";
const OBJECT_TOO_DEEP: &str = "The maximum configured depth of 64 has been exceeded. Cannot read next JSON object.";
const ARRAY_TOO_DEEP: &str = "The maximum configured depth of 64 has been exceeded. Cannot read next JSON array.";
const STRING_AT_END: &str = "Expected end of string, but instead reached end of data.";
const IN_STRING: &str = "'{0}' is invalid within a JSON string. The string should be correctly escaped.";
const ESCAPE: &str = "'{0}' is an invalid escapable character within a JSON string. The string should be correctly \
                      escaped.";
const NOT_HEX: &str = "'{0}' is not a hex digit following '\\u' within a JSON string. The string should be correctly \
                       escaped.";
const END_OF_NUMBER: &str = "'{0}' is an invalid end of a number. Expected a delimiter.";
const NOT_EXPONENT: &str = "'{0}' is an invalid end of a number. Expected 'E' or 'e'.";
const AFTER_SIGN: &str = "'{0}' is invalid within a number, immediately after a sign character ('+' or '-'). Expected \
                          a digit ('0'-'9').";
const AFTER_POINT: &str = "'{0}' is invalid within a number, immediately after a decimal point ('.'). Expected a \
                           digit ('0'-'9').";
const DIGIT_AT_END: &str = "Expected a digit ('0'-'9'), but instead reached end of data.";
const LEADING_ZERO: &str = "Invalid leading zero before '{0}'.";

/// What may end a number.
const DELIMITERS: &[u8] = b",}] \n\r\t/";

impl JsonReader<'_> {
    /// What .NET's reader refuses in the text, as `JsonException`'s message; `None` for a JSON document.
    fn syntax_error(text: &str) -> Option<String> {
        let mut reader = JsonReader {
            json: text.as_bytes(),
            at: 0,
            line: 0,
            column: 0,
            token: Token::None,
            open: Vec::new(),
            not_primitive: false,
        };
        loop {
            match reader.read() {
                Ok(true) => {}
                Ok(false) => return None,
                Err(message) => return Some(message),
            }
        }
    }

    /// The message, and where the reader is.
    fn fail<T>(&self, text: &str) -> Result<T, String> {
        Err(format!(
            "{text} LineNumber: {} | BytePositionInLine: {}.",
            self.line, self.column
        ))
    }

    /// The message with the byte it names: printable ASCII as itself, anything else in hex.
    fn fail_at<T>(&self, text: &str, byte: u8) -> Result<T, String> {
        let shown = if (0x20..0x7F).contains(&byte) {
            char::from(byte).to_string()
        } else {
            format!("0x{byte:02X}")
        };
        self.fail(&text.replace("{0}", &shown))
    }

    fn in_object(&self) -> bool {
        self.open.last() == Some(&true)
    }

    fn advance(&mut self, bytes: usize) {
        self.at += bytes;
        self.column += bytes;
    }

    /// `Read`: whether it read a token.
    fn read(&mut self) -> Result<bool, String> {
        self.skip_white_space();
        if !self.more()? {
            // white space alone is no document
            return if self.token == Token::None {
                self.fail(NO_TOKENS)
            } else {
                Ok(false)
            };
        }
        let first = self.json[self.at];
        match self.token {
            Token::None => self.first_token(first)?,
            _ if first == b'/' => self.next_token(first)?,
            Token::StartObject if first == b'}' => self.end_object()?,
            Token::StartObject if first != b'"' => self.fail_at(START_OF_PROPERTY, first)?,
            Token::StartObject => self.property_name()?,
            Token::StartArray if first == b']' => self.end_array()?,
            Token::StartArray | Token::PropertyName => self.value(first)?,
            Token::Other => self.next_token(first)?,
        }
        Ok(true)
    }

    /// `HasMoreData`: whether there is more to read. An object or array still open at the end fails.
    fn more(&self) -> Result<bool, String> {
        if self.at < self.json.len() {
            Ok(true)
        } else if self.not_primitive && !self.open.is_empty() {
            self.fail(OPEN_AT_END)
        } else {
            Ok(false)
        }
    }

    /// JSON's white space; a line feed starts a line.
    fn skip_white_space(&mut self) {
        while let Some(&b) = self.json.get(self.at) {
            match b {
                b'\n' => (self.line, self.column) = (self.line + 1, 0),
                b' ' | b'\r' | b'\t' => self.column += 1,
                _ => return,
            }
            self.at += 1;
        }
    }

    /// `ReadFirstToken`.
    fn first_token(&mut self, first: u8) -> Result<(), String> {
        match first {
            b'{' | b'[' => {
                self.start(first);
                self.not_primitive = true;
            }
            b'0'..=b'9' | b'-' => {
                let length = self.number()?;
                self.advance(length);
                self.token = Token::Other;
            }
            _ => self.value(first)?,
        }
        Ok(())
    }

    /// `StartObject` or `StartArray`, once the depth is checked.
    fn start(&mut self, bracket: u8) {
        self.open.push(bracket == b'{');
        self.token = if bracket == b'{' {
            Token::StartObject
        } else {
            Token::StartArray
        };
        self.advance(1);
    }

    /// `ConsumeValue`.
    fn value(&mut self, first: u8) -> Result<(), String> {
        match first {
            b'"' => self.string(),
            b'{' | b'[' if self.open.len() >= 64 => {
                self.fail(if first == b'{' { OBJECT_TOO_DEEP } else { ARRAY_TOO_DEEP })
            }
            b'{' | b'[' => {
                self.start(first);
                Ok(())
            }
            b'0'..=b'9' | b'-' => {
                let length = self.number()?;
                self.advance(length);
                self.token = Token::Other;
                // the text can't end in a number inside an object or array
                if self.at == self.json.len() && self.not_primitive {
                    return self.fail_at(END_OF_NUMBER, self.json[self.at - 1]);
                }
                Ok(())
            }
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            b'n' => self.literal(b"null"),
            _ => self.fail_at(START_OF_VALUE, first),
        }
    }

    /// `ConsumeNextToken`: what may come after a value, or after an object's or array's end.
    fn next_token(&mut self, marker: u8) -> Result<(), String> {
        if self.open.is_empty() {
            return self.fail_at(AFTER_SINGLE_VALUE, marker);
        }
        match marker {
            b',' => {
                self.advance(1);
                if self.at == self.json.len() {
                    // named at the comma
                    self.at -= 1;
                    self.column -= 1;
                    return self.fail(PROPERTY_OR_VALUE_AT_END);
                }
                self.skip_white_space();
                let Some(&first) = self.json.get(self.at) else {
                    return self.fail(PROPERTY_OR_VALUE_AT_END);
                };
                match first {
                    b'"' if self.in_object() => self.property_name(),
                    b'}' if self.in_object() => self.fail(TRAILING_COMMA_IN_OBJECT),
                    _ if self.in_object() => self.fail_at(START_OF_PROPERTY, first),
                    b']' => self.fail(TRAILING_COMMA_IN_ARRAY),
                    _ => self.value(first),
                }
            }
            b'}' => self.end_object(),
            b']' => self.end_array(),
            _ => self.fail_at(AFTER_VALUE, marker),
        }
    }

    /// `EndObject`.
    fn end_object(&mut self) -> Result<(), String> {
        if !self.in_object() {
            return self.fail_at(NO_MATCHING_OPEN, b'}');
        }
        self.close();
        Ok(())
    }

    /// `EndArray`.
    fn end_array(&mut self) -> Result<(), String> {
        if self.in_object() || self.open.is_empty() {
            return self.fail_at(NO_MATCHING_OPEN, b']');
        }
        self.close();
        Ok(())
    }

    fn close(&mut self) {
        self.open.pop();
        self.token = Token::Other;
        self.advance(1);
    }

    /// `ConsumePropertyName`: a string, then a colon.
    fn property_name(&mut self) -> Result<(), String> {
        self.string()?;
        self.skip_white_space();
        match self.json.get(self.at) {
            None => self.fail(VALUE_AT_END),
            Some(b':') => {
                self.advance(1);
                self.token = Token::PropertyName;
                Ok(())
            }
            Some(&other) => self.fail_at(AFTER_PROPERTY_NAME, other),
        }
    }

    /// `ConsumeString`, from its opening quote.
    fn string(&mut self) -> Result<(), String> {
        let json = self.json;
        let content = &json[self.at + 1..];
        match content.iter().position(|&b| b == b'"' || b == b'\\' || b < b' ') {
            Some(end) if content[end] == b'"' => {
                self.advance(end + 2);
                self.token = Token::Other;
                Ok(())
            }
            Some(first) => self.escaped_string(content, first),
            None => {
                self.column += content.len() + 1;
                self.fail(STRING_AT_END)
            }
        }
    }

    /// `ConsumeStringAndValidate`: a string's content byte by byte, from its first backslash or control character.
    fn escaped_string(&mut self, content: &[u8], first: usize) -> Result<(), String> {
        self.column += first + 1;
        let mut escaped = false;
        let mut i = first;
        while i < content.len() {
            let b = content[i];
            if b == b'"' && !escaped {
                self.column += 1;
                self.at += i + 2;
                self.token = Token::Other;
                return Ok(());
            } else if b == b'\\' {
                escaped = !escaped;
            } else if escaped {
                if !b"\"nrt/ubf".contains(&b) {
                    return self.fail_at(ESCAPE, b);
                }
                if b == b'u' {
                    // four hex digits, each but the last counted as it is checked
                    self.column += 1;
                    for (n, &digit) in content.iter().skip(i + 1).take(4).enumerate() {
                        if !digit.is_ascii_hexdigit() {
                            return self.fail_at(NOT_HEX, digit);
                        }
                        if n < 3 {
                            self.column += 1;
                        }
                    }
                    if i + 4 >= content.len() {
                        break;
                    }
                    i += 4;
                }
                escaped = false;
            } else if b < b' ' {
                return self.fail_at(IN_STRING, b);
            }
            self.column += 1;
            i += 1;
        }
        self.fail(STRING_AT_END)
    }

    /// `ConsumeLiteral`: `true`, `false` or `null`. A wrong one is named with the rest of the text.
    fn literal(&mut self, literal: &[u8]) -> Result<(), String> {
        let json = self.json;
        let rest = &json[self.at..];
        if rest.starts_with(literal) {
            self.advance(literal.len());
            self.token = Token::Other;
            return Ok(());
        }
        // `CheckLiteral`: at the first byte that differs, or where the text ends
        let wrong = (1..literal.len())
            .find(|&i| rest.get(i) != Some(&literal[i]))
            .unwrap_or(literal.len());
        self.column += wrong;
        self.fail(&format!(
            "'{}' is an invalid JSON literal. Expected the literal '{}'.",
            String::from_utf8_lossy(rest),
            String::from_utf8_lossy(literal)
        ))
    }

    /// `TryGetNumber`: the number's length. A number that goes wrong fails where it does.
    fn number(&mut self) -> Result<usize, String> {
        let json = self.json;
        let data = &json[self.at..];
        let digits_from = |mut i: usize| {
            while data.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            i
        };
        // a digit wanted at `i`: the end of the text, or the byte that isn't one
        let digit_wanted = |i: usize, after: &'static str| if i == data.len() { DIGIT_AT_END } else { after };
        let mut i = usize::from(data[0] == b'-');
        if i == 1 && !data.get(1).is_some_and(u8::is_ascii_digit) {
            return self.fail_ahead(i, digit_wanted(i, AFTER_SIGN));
        }
        // the whole part: a 0, or digits
        let zero = data[i] == b'0';
        i = if zero { i + 1 } else { digits_from(i + 1) };
        match data.get(i) {
            None => return Ok(i),
            Some(b) if DELIMITERS.contains(b) => return Ok(i),
            Some(b) if zero && b.is_ascii_digit() => return self.fail_ahead(i, LEADING_ZERO),
            Some(b'.' | b'e' | b'E') => {}
            Some(_) => return self.fail_ahead(i, END_OF_NUMBER),
        }
        if data[i] == b'.' {
            i += 1;
            if !data.get(i).is_some_and(u8::is_ascii_digit) {
                return self.fail_ahead(i, digit_wanted(i, AFTER_POINT));
            }
            i = digits_from(i + 1);
            match data.get(i) {
                None => return Ok(i),
                Some(b) if DELIMITERS.contains(b) => return Ok(i),
                Some(b'e' | b'E') => {}
                Some(_) => return self.fail_ahead(i, NOT_EXPONENT),
            }
        }
        // the exponent: a sign or none, then digits
        i += 1;
        if matches!(data.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !data.get(i).is_some_and(u8::is_ascii_digit) {
            return self.fail_ahead(i, digit_wanted(i, AFTER_SIGN));
        }
        i = digits_from(i + 1);
        match data.get(i) {
            None => Ok(i),
            Some(b) if DELIMITERS.contains(b) => Ok(i),
            Some(_) => self.fail_ahead(i, END_OF_NUMBER),
        }
    }

    /// Fails `ahead` bytes on from where the reader is, naming the byte there when the message names one.
    fn fail_ahead<T>(&mut self, ahead: usize, text: &str) -> Result<T, String> {
        self.column += ahead;
        match self.json.get(self.at + ahead) {
            Some(&byte) if text.contains("{0}") => self.fail_at(text, byte),
            _ => self.fail(text),
        }
    }
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
    fn json_syntax_errors_are_dotnets() {
        // JsonDocument.Parse's messages for text that isn't JSON, as recorded from .NET 10
        let none = "The input does not contain any JSON tokens. Expected the input to start with a valid JSON token, \
                    when isFinalBlock is true.";
        let open = "Expected depth to be zero at the end of the JSON payload. There is an open JSON object or array \
                    that should be closed.";
        let too_deep = "[".repeat(65);
        for (text, message, line, column) in [
            // what it starts with
            ("<html>", "'<' is an invalid start of a value.", 0, 0),
            (" \n <x", "'<' is an invalid start of a value.", 1, 1),
            ("\r\n<html>", "'<' is an invalid start of a value.", 1, 0),
            ("é", "'0xC3' is an invalid start of a value.", 0, 0),
            ("\u{7f}", "'0x7F' is an invalid start of a value.", 0, 0),
            ("   ", none, 0, 3),
            ("\n\n", none, 2, 0),
            // a text that ends too soon
            ("{", open, 0, 1),
            ("{\"a\":", open, 0, 5),
            ("{\"a\":{\"b\":[]}\n", open, 1, 0),
            ("{\"a\"", "Expected a value, but instead reached end of data.", 0, 4),
            (
                "{\"a\":1",
                "'1' is an invalid end of a number. Expected a delimiter.",
                0,
                6,
            ),
            (
                "{\"a\":-0",
                "'0' is an invalid end of a number. Expected a delimiter.",
                0,
                7,
            ),
            (
                "[1,",
                "Expected start of a property name or value, but instead reached end of data.",
                0,
                2,
            ),
            (
                "[1, ",
                "Expected start of a property name or value, but instead reached end of data.",
                0,
                4,
            ),
            (
                r#"{"issues":[{"key":"ABC-1","fields":{"summary":"Fix"#,
                "Expected end of string, but instead reached end of data.",
                0,
                50,
            ),
            ("[\"é", "Expected end of string, but instead reached end of data.", 0, 4),
            (
                r#"{"a":"\u12"#,
                "Expected end of string, but instead reached end of data.",
                0,
                10,
            ),
            (
                r#"["a\u004"#,
                "Expected end of string, but instead reached end of data.",
                0,
                8,
            ),
            (
                r#"["x\\\"]"#,
                "Expected end of string, but instead reached end of data.",
                0,
                8,
            ),
            (
                "{\"a\":1.",
                "Expected a digit ('0'-'9'), but instead reached end of data.",
                0,
                7,
            ),
            (
                "{\"a\":1e+",
                "Expected a digit ('0'-'9'), but instead reached end of data.",
                0,
                8,
            ),
            (
                "-",
                "Expected a digit ('0'-'9'), but instead reached end of data.",
                0,
                1,
            ),
            // literals: a wrong one is named with the rest of the text
            (
                "{\"a\":tru",
                "'tru' is an invalid JSON literal. Expected the literal 'true'.",
                0,
                8,
            ),
            (
                "{\"a\":nil}",
                "'nil}' is an invalid JSON literal. Expected the literal 'null'.",
                0,
                6,
            ),
            (
                "[fals]",
                "'fals]' is an invalid JSON literal. Expected the literal 'false'.",
                0,
                5,
            ),
            (
                "{\"a\":nulx, \"b\": \"a tail\"}",
                "'nulx, \"b\": \"a tail\"}' is an invalid JSON literal. Expected the literal 'null'.",
                0,
                8,
            ),
            (
                "[tré]",
                "'tré]' is an invalid JSON literal. Expected the literal 'true'.",
                0,
                3,
            ),
            (
                "[nu\nll]",
                "'nu\nll]' is an invalid JSON literal. Expected the literal 'null'.",
                0,
                3,
            ),
            (
                "{\"a\":nulll}",
                "'l' is invalid after a value. Expected either ',', '}', or ']'.",
                0,
                9,
            ),
            (
                "truex",
                "'x' is invalid after a single JSON value. Expected end of data.",
                0,
                4,
            ),
            ("{\"a\":True}", "'T' is an invalid start of a value.", 0, 5),
            // objects and arrays
            (
                "{\"a\":1 \"b\":2}",
                "'\"' is invalid after a value. Expected either ',', '}', or ']'.",
                0,
                7,
            ),
            (
                "{\"a\":1}}",
                "'}' is invalid after a single JSON value. Expected end of data.",
                0,
                7,
            ),
            ("{\"a\":1]", "']' is invalid without a matching open.", 0, 6),
            ("[1}", "'}' is invalid without a matching open.", 0, 2),
            (
                "{\"a\" 1}",
                "'1' is invalid after a property name. Expected a ':'.",
                0,
                5,
            ),
            ("{\"a\":}", "'}' is an invalid start of a value.", 0, 5),
            (
                "{a:1}",
                "'a' is an invalid start of a property name. Expected a '\"'.",
                0,
                1,
            ),
            ("[}", "'}' is an invalid start of a value.", 0, 1),
            (
                "{]",
                "']' is an invalid start of a property name. Expected a '\"'.",
                0,
                1,
            ),
            (
                "{\"a\":1,}",
                "The JSON object contains a trailing comma at the end which is not supported in this mode. Change the \
                 reader options.",
                0,
                7,
            ),
            (
                "[1,]",
                "The JSON array contains a trailing comma at the end which is not supported in this mode. Change the \
                 reader options.",
                0,
                3,
            ),
            (
                "{\"a\":/*c*/1}",
                "'/' is invalid after a value. Expected either ',', '}', or ']'.",
                0,
                5,
            ),
            (
                "{\"a\":1}//c",
                "'/' is invalid after a single JSON value. Expected end of data.",
                0,
                7,
            ),
            (
                "{\"a\":1}\u{a0}",
                "'0xC2' is invalid after a single JSON value. Expected end of data.",
                0,
                7,
            ),
            (
                &too_deep,
                "The maximum configured depth of 64 has been exceeded. Cannot read next JSON array.",
                0,
                64,
            ),
            // lines and bytes: a line feed starts a line, a carriage return is a byte, a character is its UTF-8
            (
                "{\"a\":1}\n\n  x",
                "'x' is invalid after a single JSON value. Expected end of data.",
                2,
                2,
            ),
            (
                "{\n  \"a\": 1,\n  \"b\": [1, 2,\n    x]\n}",
                "'x' is an invalid start of a value.",
                3,
                4,
            ),
            (
                "[1,\r2 x]",
                "'x' is invalid after a value. Expected either ',', '}', or ']'.",
                0,
                6,
            ),
            (
                "{\"ab€\":1,x}",
                "'x' is an invalid start of a property name. Expected a '\"'.",
                0,
                11,
            ),
            // numbers
            ("{\"a\":01}", "Invalid leading zero before '1'.", 0, 6),
            ("00", "Invalid leading zero before '0'.", 0, 1),
            (
                "{\"a\":1.x}",
                "'x' is invalid within a number, immediately after a decimal point ('.'). Expected a digit ('0'-'9').",
                0,
                7,
            ),
            (
                "{\"a\":1ex}",
                "'x' is invalid within a number, immediately after a sign character ('+' or '-'). Expected a digit \
                 ('0'-'9').",
                0,
                7,
            ),
            (
                "[- 1]",
                "' ' is invalid within a number, immediately after a sign character ('+' or '-'). Expected a digit \
                 ('0'-'9').",
                0,
                2,
            ),
            (
                "[1.5E+]",
                "']' is invalid within a number, immediately after a sign character ('+' or '-'). Expected a digit \
                 ('0'-'9').",
                0,
                6,
            ),
            (
                "{\"a\":1.5.5}",
                "'.' is an invalid end of a number. Expected 'E' or 'e'.",
                0,
                8,
            ),
            (
                "{\"a\":1e5e5}",
                "'e' is an invalid end of a number. Expected a delimiter.",
                0,
                8,
            ),
            (
                "[0-1]",
                "'-' is an invalid end of a number. Expected a delimiter.",
                0,
                2,
            ),
            ("12x", "'x' is an invalid end of a number. Expected a delimiter.", 0, 2),
            (
                "[1\u{1}]",
                "'0x01' is an invalid end of a number. Expected a delimiter.",
                0,
                2,
            ),
            // strings
            (
                "{\"a\":\"line\nbreak\"}",
                "'0x0A' is invalid within a JSON string. The string should be correctly escaped.",
                0,
                10,
            ),
            (
                r#"{"a":"\x"}"#,
                "'x' is an invalid escapable character within a JSON string. The string should be correctly escaped.",
                0,
                7,
            ),
            (
                r#"{"a":"\u12G4"}"#,
                "'G' is not a hex digit following '\\u' within a JSON string. The string should be correctly escaped.",
                0,
                10,
            ),
            (
                r#"{"a\u12":1}"#,
                "'\"' is not a hex digit following '\\u' within a JSON string. The string should be correctly escaped.",
                0,
                7,
            ),
        ] {
            let expected = format!("{message} LineNumber: {line} | BytePositionInLine: {column}.");
            assert_eq!(parse_json(text).unwrap_err(), expected, "{text:?}");
        }
        // what .NET reads
        let deepest = format!("{}{}", "[".repeat(64), "]".repeat(64));
        for text in ["0", "-0", "1e5", "12 \n", "{\"a\":1,\"a\":2}", &deepest] {
            assert!(parse_json(text).is_ok(), "{text:?}");
        }
        // and serde_json doesn't, a lone surrogate's escape, which keeps serde_json's words
        assert_eq!(JsonReader::syntax_error(r#"["\ud800"]"#), None);
        assert!(parse_json(r#"["\ud800"]"#).is_err());
    }

    #[test]
    fn json_elements_read_as_dotnets() {
        // JsonElement's texts, as recorded from .NET 10
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
    fn status_lines_are_read_as_dotnet_reads_them() {
        // ReasonPhrase for these status lines, as recorded from .NET 10 (None: .NET refuses the line)
        for (line, read) in [
            (
                &b"HTTP/1.1 403 Rate Limit Exceeded"[..],
                Some((403, "Rate Limit Exceeded")),
            ),
            (b"HTTP/1.1 403 Forbidden", Some((403, "Forbidden"))),
            (b"HTTP/1.1 403 forbidden", Some((403, "forbidden"))),
            (b"HTTP/1.1 429 ", Some((429, ""))),
            (b"HTTP/1.1 429", Some((429, ""))),
            (b"HTTP/1.1 403  Two spaces", Some((403, " Two spaces"))),
            (b"HTTP/1.1 403 Trailing  ", Some((403, "Trailing  "))),
            (b"HTTP/1.1 403 Verboten \xc3\xbc", Some((403, "Verboten Ã¼"))),
            (b"HTTP/1.1 403 Forbidd\xe9n", Some((403, "Forbiddén"))),
            (b"HTTP/1.0 403 Nope", Some((403, "Nope"))),
            (b"HTTP/1.1 999 Custom", Some((999, "Custom"))),
            (b"HTTP/1.1 403\tTab", None),
            (b"HTTP/2 403 Forbidden", None),
            (b"HTTP/1.1 40x Bad", None),
        ] {
            let read = read.map(|(code, reason): (u16, &str)| (code, reason.to_string()));
            assert_eq!(read_status_line(line), read, "{}", String::from_utf8_lossy(line));
        }
        let line = |status, reason: Option<&str>| {
            Answer {
                status,
                reason: reason.map(Into::into),
                body: String::new(),
            }
            .status_line()
        };
        assert_eq!(line(403, Some("Rate Limit Exceeded")), "403 Rate Limit Exceeded");
        assert_eq!(line(429, Some("")), "429 ");
        // a line that wasn't read: the standard phrase, which .NET gives a code without its own, and nothing when
        // there is none
        assert_eq!(line(403, None), "403 Forbidden");
        assert_eq!(line(599, None), "599 ");
    }
}

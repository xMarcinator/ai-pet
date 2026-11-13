//! The checks before any handler: the body's size, Host and Origin for every route, and the bearer token for `/mcp`.
//! Each refusal has an empty body.
//!
//! Host and Origin keep web pages out (DNS rebinding, and pages of other local servers): a Host must be loopback
//! (`127.0.0.1`, `localhost` or `[::1]`, on the listener's port if one is given), and an Origin, when there is one,
//! must be the listener's own. Agents' CLIs send no Origin, and pass.

use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aipet_ipc::protocol::MAX_REQUEST;
use axum::body::Body;
use axum::extract::{Request, State as Shared};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::State;

/// The log gets a line about requests without the right token at most this often: a client that keeps trying would
/// fill it.
const REFUSED_EVERY: Duration = Duration::from_secs(60);

/// Every route's check: 413 for a body declared over [`MAX_REQUEST`] (a longer body that declares none is cut off
/// where it is read, with 413 too), 403 for a Host or an Origin that isn't let in.
pub(crate) async fn loopback(Shared(state): Shared<Arc<State>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|len| len.to_str().ok()?.parse::<u64>().ok());
    if declared.is_some_and(|len| len > MAX_REQUEST as u64) {
        return empty(StatusCode::PAYLOAD_TOO_LARGE);
    }
    if !host_allowed(headers, state.port) || !origin_allowed(headers, &state.origins()) {
        return empty(StatusCode::FORBIDDEN);
    }
    // a large body (or one that declares no length) is read and parsed in this thread's malloc arena
    let large = declared.is_none_or(|len| len >= LARGE_REQUEST);
    let response = next.run(request).await;
    if large {
        give_back_freed_memory();
    }
    response
}

/// From this size a request's freed memory is handed back to the OS when it is done (R15).
const LARGE_REQUEST: u64 = 256 * 1024;

/// Hands the heap's free pages back to the OS. glibc keeps what a large request freed in the listener thread's
/// arena: a hook body at the cap left the pet 5 MB of anonymous memory above a pet without the listener, for good
/// (R15's paired run, 2026-11-13). Windows' heap gives blocks over 512 kB back by itself, and musl and the BSDs'
/// allocators return large frees too.
fn give_back_freed_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only walks glibc's own arenas; it takes their locks and frees no live allocation.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// `/mcp`'s check: `Authorization: Bearer <the token>`, else 401, logged at most once a minute and never with what
/// was sent.
pub(crate) async fn bearer(Shared(state): Shared<Arc<State>>, request: Request, next: Next) -> Response {
    let sent = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
        });
    if sent.is_some_and(|sent| same(sent.as_bytes(), state.token.as_bytes())) {
        return next.run(request).await;
    }
    let due = {
        let mut refused = state.refused.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        due(&mut refused, Instant::now())
    };
    if due {
        let why = if sent.is_some() { "a wrong token" } else { "no token" };
        state.log(&format!(
            "mcp: refused a request with {why} (said at most once a minute)"
        ));
    }
    empty(StatusCode::UNAUTHORIZED)
}

/// A handler that panicked: 500, and a line in the log.
pub(crate) fn panicked(state: &State, panic: Box<dyn Any + Send + 'static>) -> Response {
    let what = panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("a panic");
    state.log(&format!("mcp: a request's handler panicked: {what}"));
    empty(StatusCode::INTERNAL_SERVER_ERROR)
}

pub(crate) fn empty(status: StatusCode) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

/// Whether a refusal is due a log line now, and if so, from now on the last.
fn due(last: &mut Option<Instant>, now: Instant) -> bool {
    if last.is_some_and(|last| now.saturating_duration_since(last) < REFUSED_EVERY) {
        return false;
    }
    *last = Some(now);
    true
}

/// Compares in a time that depends only on the lengths: the token's length is no secret.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// One Host header naming a loopback host, on `port` if it names a port.
fn host_allowed(headers: &HeaderMap, port: u16) -> bool {
    let mut hosts = headers.get_all(header::HOST).iter();
    let (Some(host), None) = (hosts.next(), hosts.next()) else {
        return false;
    };
    let Ok(authority) = host.to_str().unwrap_or("").parse::<axum::http::uri::Authority>() else {
        return false;
    };
    let name = authority.host().trim_start_matches('[').trim_end_matches(']');
    let loopback = ["127.0.0.1", "localhost", "::1"]
        .iter()
        .any(|l| name.eq_ignore_ascii_case(l));
    loopback && authority.port_u16().is_none_or(|p| p == port) && !authority.as_str().contains('@')
}

/// No Origin, or one that is the listener's own.
fn origin_allowed(headers: &HeaderMap, own: &[String]) -> bool {
    headers.get_all(header::ORIGIN).iter().all(|origin| {
        origin
            .to_str()
            .is_ok_and(|origin| own.iter().any(|o| o.eq_ignore_ascii_case(origin)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(name.clone(), value.parse().unwrap());
        }
        map
    }

    #[test]
    fn a_host_must_be_loopback_on_the_listeners_port() {
        for (host, allowed) in [
            ("127.0.0.1:47615", true),
            ("127.0.0.1", true),
            ("localhost:47615", true),
            ("LOCALHOST:47615", true),
            ("[::1]:47615", true),
            ("127.0.0.1:8080", false),
            ("evil.example:47615", false),
            ("127.0.0.1.nip.io:47615", false),
            ("0.0.0.0:47615", false),
            ("user@127.0.0.1:47615", false),
            ("", false),
        ] {
            assert_eq!(
                host_allowed(&headers(&[(header::HOST, host)]), 47615),
                allowed,
                "{host}"
            );
        }
        assert!(!host_allowed(&HeaderMap::new(), 47615), "no Host");
        let twice = headers(&[(header::HOST, "127.0.0.1:47615"), (header::HOST, "evil.example")]);
        assert!(!host_allowed(&twice, 47615), "two Hosts");
    }

    #[test]
    fn an_origin_must_be_the_listeners_own_when_there_is_one() {
        let own = ["http://127.0.0.1:47615", "http://localhost:47615", "http://[::1]:47615"].map(String::from);
        assert!(origin_allowed(&HeaderMap::new(), &own), "no Origin: a CLI");
        for (origin, allowed) in [
            ("http://127.0.0.1:47615", true),
            ("http://localhost:47615", true),
            ("http://[::1]:47615", true),
            ("http://localhost:3000", false),
            ("https://127.0.0.1:47615", false),
            ("http://evil.example", false),
            ("null", false),
        ] {
            assert_eq!(
                origin_allowed(&headers(&[(header::ORIGIN, origin)]), &own),
                allowed,
                "{origin}"
            );
        }
    }

    #[test]
    fn a_refusal_is_logged_at_most_once_a_minute() {
        let start = Instant::now();
        let mut last = None;
        assert!(due(&mut last, start));
        assert!(!due(&mut last, start + Duration::from_secs(1)));
        assert!(!due(&mut last, start + Duration::from_secs(59)));
        assert!(due(&mut last, start + Duration::from_secs(60)));
        assert!(!due(&mut last, start + Duration::from_secs(61)));
    }

    #[test]
    fn tokens_compare_whole() {
        assert!(same(b"abcd", b"abcd"));
        assert!(!same(b"abcd", b"abce"));
        assert!(!same(b"abc", b"abcd"));
        assert!(!same(b"", b"abcd"));
    }
}

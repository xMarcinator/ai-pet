//! The pet's HTTP listener (fnt-71): MCP at `/mcp` for agents that drive the pet (and for Codex's hooks, which call
//! its `report_event`), and Claude's HTTP hooks at `/hook/claude`, on 127.0.0.1 at [`MCP_PORT`].
//!
//! It runs on a thread of its own with a current-thread tokio runtime, never on the shells' executor (aipet-ui's
//! one-worker `Executor`), and it reaches the rest of the pet only through the core's [`Ingress`], which the pipe
//! server shares (events from either side land in the same chats and the same record), and through its [`PetLink`]:
//! the tools that read or change the pet's avatar, settings and board ask the pet, which answers within
//! [`ANSWER_WITHIN`] or the call is `isError`. The hooks' status events go to the ingress
//! through one bounded queue and a thread of their own ([`events`]), in the order they arrived.
//!
//! Every request passes the same checks first ([`guard`]): a body over [`MAX_REQUEST`] gets 413, a Host that isn't
//! loopback or an Origin that isn't the listener's own gets 403 (both empty), and at most 16 requests are handled at
//! once (the rest wait). `/mcp` also needs the user's bearer token ([`token`]), else 401. rmcp serves both MCP eras on
//! it: 2026-07-28's stateless requests, and initialize with `Mcp-Session-Id` for older clients, whose sessions end
//! after 5 idle minutes and with the pet (an old id then gets 404, and the client starts again). A panic in a handler
//! answers 500, and the pet runs on.
//!
//! When the port is taken (another user's pet, another program) the start fails with one line in the log, and the
//! pet runs without the listener: Settings and the pipe still work.

mod events;
mod guard;
mod hook;
mod report;
mod token;
mod tools;

use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use aipet_core::control::ANSWER_WITHIN;
use aipet_core::control::PetLink;
use aipet_core::server::Ingress;
use aipet_ipc::protocol::{MAX_REQUEST, MCP_PORT};
use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::sync::oneshot;
use tower::limit::GlobalConcurrencyLimitLayer;
use tower_http::catch_panic::CatchPanicLayer;

pub use tools::TOOLS;

/// Requests handled at once; past it they wait for one to finish.
pub const CONCURRENT: usize = 16;

/// Where the listener listens, its token and what it logs: the pet's own ([`Options::app`]), or a test's.
pub struct Options {
    /// 0 for any free port (tests).
    pub port: u16,
    /// The bearer token's file ([`aipet_ipc::paths::mcp_token`]), made or replaced at the start when it isn't right.
    pub token_file: PathBuf,
    /// The pet's version, which `status` gives.
    pub version: String,
    /// Where the listener's `mcp:` lines go: the app log, aipet.log.
    pub log: Arc<dyn Fn(&str) + Send + Sync>,
    /// How the tools reach the pet.
    pub pet: PetLink,
    /// The pet's data folder: its avatars folder, and the staging folder packs are copied to.
    pub data: PathBuf,
    /// The user's home folder, which a pack to install must be inside.
    pub home: PathBuf,
    /// How long a tool waits for the pet's answer: [`ANSWER_WITHIN`] (a debug build's tests, under load, wait longer).
    pub answer_within: Duration,
}

impl Options {
    /// The pet's: [`MCP_PORT`] (or `AIPET_PORT`, for tests), its data folder's token, aipet.log, and the pet.
    pub fn app(version: impl Into<String>, pet: PetLink) -> Options {
        Options {
            port: port(std::env::var("AIPET_PORT").ok().as_deref()),
            token_file: aipet_ipc::paths::mcp_token(),
            version: version.into(),
            log: Arc::new(aipet_core::log::write),
            pet,
            data: aipet_ipc::paths::data_dir().to_owned(),
            home: aipet_ipc::paths::home().to_owned(),
            answer_within: ANSWER_WITHIN,
        }
    }
}

/// A listening socket on 127.0.0.1 at `port`, as `TcpListener::bind` makes it, but on Windows with
/// `SO_EXCLUSIVEADDRUSE`: without it, a program that sets `SO_REUSEADDR` could bind the same port and take the agents'
/// connections, and with them the token. On Unix, `SO_REUSEADDR` (std's choice) lets nothing take a port that is
/// listening.
fn bind(port: u16) -> io::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    #[cfg(windows)]
    exclusive(&socket)?;
    socket.bind(&std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, port)).into())?;
    socket.listen(128)?;
    Ok(socket.into())
}

/// `SO_EXCLUSIVEADDRUSE` on `socket`, before it binds. windows-sys doesn't define it: Winsock's header makes it
/// `~SO_REUSEADDR`.
#[cfg(windows)]
fn exclusive(socket: &socket2::Socket) -> io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        SO_REUSEADDR, SOCKET_ERROR, SOL_SOCKET, WSAGetLastError, setsockopt,
    };
    let on: i32 = 1;
    // SAFETY: a valid socket handle, and a pointer to an i32 with its length, as setsockopt takes them
    let failed = unsafe {
        setsockopt(
            socket.as_raw_socket() as _,
            SOL_SOCKET,
            !SO_REUSEADDR,
            (&on as *const i32).cast(),
            std::mem::size_of::<i32>() as i32,
        )
    } == SOCKET_ERROR;
    if failed {
        // SAFETY: no arguments; reads this thread's last Winsock error
        return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
    }
    Ok(())
}

/// `AIPET_PORT` when it is a port number, else [`MCP_PORT`].
fn port(var: Option<&str>) -> u16 {
    var.and_then(|v| v.parse().ok()).unwrap_or(MCP_PORT)
}

/// The running listener. Dropping it stops it: its thread ends, and with it every connection and MCP session.
pub struct Listener {
    port: u16,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// Binds 127.0.0.1 at the port, makes or checks the token, and starts serving on a thread of its own. Whether it
    /// could goes to the log: once, either way. The pet runs on without the listener when it couldn't.
    pub fn start(options: Options, ingress: Arc<Ingress>) -> io::Result<Listener> {
        let log = Arc::clone(&options.log);
        let wanted = options.port;
        let started = Self::start_with(options, ingress);
        match &started {
            Ok(listener) => log(&format!("mcp: listening on 127.0.0.1:{}", listener.port)),
            Err(e) => log(&format!(
                "mcp: can't listen on 127.0.0.1:{wanted}, so agents can't reach the pet over HTTP: {e}"
            )),
        }
        started
    }

    fn start_with(options: Options, ingress: Arc<Ingress>) -> io::Result<Listener> {
        let Options {
            port,
            token_file,
            version,
            log,
            pet,
            data,
            home,
            answer_within,
        } = options;
        // the port first: a pet that can't have it leaves the token file alone
        let socket = bind(port)?;
        socket.set_nonblocking(true)?;
        let port = socket.local_addr()?.port();
        let token = token::load_or_make(&token_file, &*log)?;
        let events = events::Events::start(Arc::clone(&ingress), Arc::clone(&log))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let state = Arc::new(State {
            port,
            token,
            version,
            ingress,
            events,
            pet,
            data,
            home,
            answer_within,
            log,
            refused: Mutex::new(None),
        });
        let (stop, stopped) = oneshot::channel();
        let thread = thread::Builder::new()
            .name("aipet-mcp".into())
            .spawn(move || runtime.block_on(serve(socket, state, stopped)))?;
        Ok(Listener {
            port,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // the thread wakes at once when its channel goes
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What the listener's handlers share.
pub(crate) struct State {
    pub(crate) port: u16,
    pub(crate) token: String,
    pub(crate) version: String,
    pub(crate) ingress: Arc<Ingress>,
    /// Where the status events go on their way to the ingress, in the order they arrived.
    pub(crate) events: events::Events,
    pub(crate) pet: PetLink,
    pub(crate) data: PathBuf,
    pub(crate) home: PathBuf,
    pub(crate) answer_within: Duration,
    log: Arc<dyn Fn(&str) + Send + Sync>,
    /// When the log last got a line about a request without the right token.
    pub(crate) refused: Mutex<Option<Instant>>,
}

impl State {
    pub(crate) fn log(&self, line: &str) {
        (self.log)(line);
    }

    /// The Origins a browser gives a page of the listener's own: the only ones let in.
    fn origins(&self) -> [String; 3] {
        let port = self.port;
        [
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
            format!("http://[::1]:{port}"),
        ]
    }
}

/// Serves until the listener is dropped; the runtime, dropped after, ends whatever still runs.
async fn serve(socket: TcpListener, state: Arc<State>, stopped: oneshot::Receiver<()>) {
    let listener = match tokio::net::TcpListener::from_std(socket) {
        Ok(listener) => listener,
        Err(e) => return state.log(&format!("mcp: can't serve: {e}")),
    };
    let config = config(&state);
    let sessions_end = config.cancellation_token.clone();
    let router = router(state, config);
    tokio::select! {
        _ = axum::serve(listener, router).into_future() => {}
        _ = stopped => {}
    }
    sessions_end.cancel();
}

/// rmcp's settings. It checks Host and Origin too, as the guard does: its empty default would let any Origin in.
fn config(state: &State) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_allowed_origins(state.origins())
        .enforce_origin_validation()
        .with_max_request_body_bytes(MAX_REQUEST)
        .with_json_response(true)
        // clients of 2025-11-25 and before: initialize, Mcp-Session-Id and GET SSE
        .with_legacy_session_mode(true)
}

/// The routes, with the checks every request passes.
fn router(state: Arc<State>, config: StreamableHttpServerConfig) -> Router {
    let tools = Arc::clone(&state);
    // sessions end after SessionConfig's 5 idle minutes
    let mcp = StreamableHttpService::new(
        move || Ok(tools::Pet::new(Arc::clone(&tools))),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let panicked = Arc::clone(&state);
    Router::new()
        .nest_service("/mcp", mcp)
        .layer(axum::middleware::from_fn_with_state(Arc::clone(&state), guard::bearer))
        .merge(hook::routes())
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            guard::loopback,
        ))
        .layer(GlobalConcurrencyLimitLayer::new(CONCURRENT))
        .layer(CatchPanicLayer::custom(move |panic| guard::panicked(&panicked, panic)))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aipet_port_moves_the_listener_only_when_it_is_a_port() {
        assert_eq!(port(None), MCP_PORT);
        assert_eq!(port(Some("51234")), 51234);
        assert_eq!(port(Some("0")), 0);
        for bad in ["", "port", "70000", "-1"] {
            assert_eq!(port(Some(bad)), MCP_PORT, "{bad}");
        }
    }
}

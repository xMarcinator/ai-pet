//! Finding out, without panicking, whether the compositor has what iced_exwlshell needs: its runtime panics
//! when a required global is missing (a missing layer-shell only when the first layer surface is made).

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, QueueHandle};

/// The globals exwlshellev binds without a fallback, with the lowest version it takes.
const REQUIRED: [(&str, u32); 6] = [
    ("wl_compositor", 1),
    ("wl_shm", 1),
    ("wl_seat", 1),
    ("xdg_wm_base", 2),
    ("zwlr_layer_shell_v1", 3),
    ("wp_viewporter", 1),
];

struct Registry;

impl Dispatch<WlRegistry, GlobalListContents> for Registry {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// A connection to the session's compositor, if it offers every [`REQUIRED`] global; otherwise what is missing.
pub fn connect() -> Result<Connection, String> {
    let connection = Connection::connect_to_env().map_err(|e| format!("no Wayland connection: {e}"))?;
    let (globals, _queue) =
        registry_queue_init::<Registry>(&connection).map_err(|e| format!("cannot list Wayland globals: {e}"))?;
    let missing: Vec<&str> = globals.contents().with_list(|list| {
        REQUIRED
            .iter()
            .filter(|(name, version)| !list.iter().any(|g| g.interface == *name && g.version >= *version))
            .map(|(name, _)| *name)
            .collect()
    });
    match missing.as_slice() {
        [] => Ok(connection),
        _ => Err(format!("the compositor lacks {}", missing.join(", "))),
    }
}

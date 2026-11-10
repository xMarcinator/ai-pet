//! The flow-next tracker as the pet's companion (fnt-81, the tracker's FNT-64): its active tasks become task bubbles.
//!
//! - [`wire`]: protocol version 2's task messages, the tasks as the Board takes them;
//! - [`api`]: the tracker's companion routes, read into those;
//! - [`view`]: the active tasks as the change feed left them;
//! - [`watcher`]: enrolling, and following the feed on a thread of its own ([`TrackerWatcher`]).

pub mod api;
pub mod view;
pub mod watcher;
pub mod wire;

pub use watcher::{Event, SessionsWithoutTask, Status, TrackerSettings, TrackerWatcher};
pub use wire::{Need, Tone, TrackerTask};

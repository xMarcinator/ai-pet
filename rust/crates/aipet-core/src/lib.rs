//! The pet's core, without a UI: what the C#'s `src/AiPet.Core` (and the data parts of `src/AiPet.UI`) does.
//!
//! - [`server`]: the hook server, on the endpoint `aipet-ipc` names, with the C#'s trust boundary;
//! - [`sessions`]: the chats' state from the hooks' events, and their ordering (AgentSessions);
//! - [`codex_watcher`]: Codex's own session logs;
//! - [`jira`], [`github`] and [`presets`]: the review watchers and their settings;
//! - [`board`]: the sources merged into the pet's bubbles;
//! - [`config`], [`secrets`], [`log`], [`cleanup`] and [`update_status`]: the data, tokens and logs the .NET app
//!   reads and writes too, the uninstaller's hook cleanup, and the update texts.
//!
//! Like the C#, it runs on threads of its own and channels, never on a thread pool or an async runtime.

pub mod board;
pub mod cleanup;
pub mod codex_watcher;
pub mod config;
pub mod github;
mod http;
pub mod jira;
pub mod log;
pub mod presets;
pub mod secrets;
pub mod server;
pub mod sessions;
pub mod update_status;

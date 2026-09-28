//! `aipet-hook [--agent claude|codex]`, run by each coding agent on every hook event (`aipet-hook.exe` on Windows).
//!
//! It reads the event from stdin and hands it to the running pet over the socket `aipet-ipc` describes. When the pet
//! isn't running it does nothing at all: it writes nothing anywhere and starts nothing. It only observes: it never
//! prints anything back to the agent, and it always exits 0, so it can't disturb the agent.
//!
//! Also `aipet-hook --install|--uninstall claude|codex`, `aipet-hook --doctor claude|codex [--probe]` and
//! `aipet-hook --print-plugin-hooks claude|codex`. A port of `src/AiPet.Hook`; until the event path is ported it does
//! nothing and exits 0.

mod claude;
mod codex;
mod doctor;
mod event;
mod install;
mod json_out;
mod plugin_hooks;
mod toml_text;
mod trace;

fn main() {}

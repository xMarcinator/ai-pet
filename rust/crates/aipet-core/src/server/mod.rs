//! The hook server: the pet's end of the endpoint, on threads of its own (`src/AiPet.Core/HookServer.cs`).

mod answer;
mod record;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

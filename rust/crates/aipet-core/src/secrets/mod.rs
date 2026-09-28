//! The Jira and GitHub tokens, under the keys `AiPet:Jira` and `AiPet:GitHub`, where the .NET app keeps them.

mod file;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

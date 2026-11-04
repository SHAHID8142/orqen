//! Structured error handling for Tauri commands.
//!
//! Convention (see `docs/ARCHITECTURE.md`): every command returns
//! `CommandResult<T>`. Errors are serialized to the frontend as a
//! discriminated payload so the UI can distinguish recoverable input
//! problems from other failures. Add new variants here as commands grow;
//! the frontend normalizer already treats unknown payloads as internal.

use serde::Serialize;
use std::fmt;

/// Error type returned by Tauri commands.
#[derive(Debug)]
pub enum CommandError {
    /// The caller supplied invalid input. Recoverable; safe to show to the user.
    InvalidInput(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
        }
    }
}

impl std::error::Error for CommandError {}

/// Wire format sent to the frontend.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandErrorPayload {
    kind: &'static str,
    message: String,
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let payload = match self {
            Self::InvalidInput(message) => CommandErrorPayload {
                kind: "invalid_input",
                message: message.clone(),
            },
        };
        payload.serialize(serializer)
    }
}

/// Result alias used by every Tauri command.
pub type CommandResult<T> = Result<T, CommandError>;

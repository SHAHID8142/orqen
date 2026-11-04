//! Application information and IPC health-check commands.
//!
//! `get_app_info` is the foundation screen's data source: it exposes real
//! application metadata (name, version, platform) — nothing simulated.

use crate::error::{CommandError, CommandResult};
use serde::Serialize;

/// Maximum accepted length for an echo message.
const ECHO_PING_MAX_LENGTH: usize = 200;

/// Metadata about the running application.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Product name from `tauri.conf.json`.
    pub name: String,
    /// Application version.
    pub version: String,
    /// Operating system the app runs on (e.g. "macos", "windows").
    pub os: String,
}

/// Returns real application metadata read from the compiled Tauri context.
#[tauri::command]
pub fn get_app_info(app: tauri::AppHandle) -> CommandResult<AppInfo> {
    let package_info = app.package_info();
    Ok(AppInfo {
        name: package_info.name.to_string(),
        version: package_info.version.to_string(),
        os: std::env::consts::OS.to_string(),
    })
}

/// Echoes a short message back to the frontend.
///
/// Exists to verify the full IPC round trip (frontend -> Rust -> frontend)
/// with input validation on the Rust side. It performs no side effects.
#[tauri::command]
pub fn echo_ping(message: &str) -> CommandResult<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(CommandError::InvalidInput(
            "echo message must not be empty".to_string(),
        ));
    }
    if trimmed.len() > ECHO_PING_MAX_LENGTH {
        return Err(CommandError::InvalidInput(format!(
            "echo message must not exceed {ECHO_PING_MAX_LENGTH} characters"
        )));
    }
    Ok(trimmed.to_string())
}

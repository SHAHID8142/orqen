//! Application information and IPC health-check commands.
//!
//! `get_app_info` is the foundation screen's data source: it exposes real
//! application metadata (name, version, platform) — nothing simulated.

use crate::error::{CommandError, CommandResult};
use serde::Serialize;

/// Maximum accepted length for an echo message, counted in Unicode scalar
/// values (code points) — the shared length policy with the frontend,
/// decided in `docs/DECISIONS.md` (D-012).
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
    if trimmed.chars().count() > ECHO_PING_MAX_LENGTH {
        return Err(CommandError::InvalidInput(format!(
            "echo message must not exceed {ECHO_PING_MAX_LENGTH} characters"
        )));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression for D-012: the limit counts code points, not UTF-8 bytes.
    /// 150 emoji are 600 UTF-8 bytes but 150 code points — accepted.
    #[test]
    fn emoji_within_code_point_limit_is_accepted() {
        let message = "🙂".repeat(150);
        assert_eq!(message.len(), 600); // bytes, under the old policy this failed
        assert_eq!(echo_ping(&message).expect("150 emoji must pass"), message);
    }

    #[test]
    fn message_at_exact_limit_is_accepted() {
        let message = "a".repeat(ECHO_PING_MAX_LENGTH);
        assert_eq!(
            echo_ping(&message).expect("exactly 200 code points must pass"),
            message
        );
    }

    #[test]
    fn message_over_limit_is_rejected() {
        let message = "a".repeat(ECHO_PING_MAX_LENGTH + 1);
        assert!(echo_ping(&message).is_err());
    }

    #[test]
    fn bangla_message_is_accepted_and_trimmed() {
        assert_eq!(
            echo_ping("  বাংলা ঠিক আছে  ").expect("Bangla text must pass"),
            "বাংলা ঠিক আছে"
        );
    }

    #[test]
    fn emoji_message_is_accepted_and_trimmed() {
        assert_eq!(
            echo_ping("  🙂👍🏽 হ্যালো  ").expect("mixed emoji + Bangla must pass"),
            "🙂👍🏽 হ্যালো"
        );
    }

    /// 201 emoji are 201 code points — rejected. (Under UTF-16 counting the
    /// frontend would see 402 units; the code-point policy keeps both sides
    /// consistent at 201.)
    #[test]
    fn emoji_over_limit_is_rejected() {
        let message = "🙂".repeat(ECHO_PING_MAX_LENGTH + 1);
        assert!(echo_ping(&message).is_err());
    }

    #[test]
    fn bangla_over_limit_is_rejected() {
        // বাংলা has no surrogate complexity but is multi-byte in UTF-8.
        let message = "ক".repeat(ECHO_PING_MAX_LENGTH + 1);
        assert!(echo_ping(&message).is_err());
    }

    #[test]
    fn empty_message_is_rejected() {
        assert!(echo_ping("").is_err());
    }

    #[test]
    fn whitespace_only_message_is_rejected() {
        assert!(echo_ping("   \t\n  ").is_err());
    }

    #[test]
    fn english_message_is_trimmed() {
        assert_eq!(
            echo_ping("  hello world  ").expect("ASCII must pass"),
            "hello world"
        );
    }
}

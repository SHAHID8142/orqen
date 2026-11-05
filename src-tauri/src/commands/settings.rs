//! Persistent settings operations.
//!
//! Settings are saved preferences only: autonomous execution and Telegram
//! delivery are NOT operational (see docs/GOVERNANCE.md §Execution
//! controls). Persisting them changes nothing about available features.

use crate::db::DbState;
use crate::error::{CommandError, CommandResult};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// Allowed bounds for the retry preference (mirrors the frontend).
pub const MIN_RETRY: i64 = 1;
pub const MAX_RETRY_LIMIT: i64 = 10;

/// Persisted settings.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub approval_mode: String,
    pub max_retry: i64,
    pub telegram_notifications: bool,
}

/// Incoming settings update. All fields are validated at the boundary.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub approval_mode: String,
    pub max_retry: i64,
    pub telegram_notifications: bool,
}

pub fn load_settings_conn(conn: &Connection) -> Result<SettingsDto, String> {
    let (approval_mode, max_retry, telegram): (String, i64, i64) = conn
        .query_row(
            "SELECT approval_mode, max_retry, telegram_notifications FROM settings WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok(SettingsDto {
        approval_mode,
        max_retry,
        telegram_notifications: telegram != 0,
    })
}

pub fn update_settings_conn(
    conn: &Connection,
    input: &SettingsInput,
) -> Result<SettingsDto, CommandError> {
    if input.approval_mode != "manual" && input.approval_mode != "autonomous" {
        return Err(CommandError::InvalidInput(
            "Approval mode must be manual or autonomous.".to_string(),
        ));
    }
    if !(MIN_RETRY..=MAX_RETRY_LIMIT).contains(&input.max_retry) {
        return Err(CommandError::InvalidInput(format!(
            "Maximum retry must be between {MIN_RETRY} and {MAX_RETRY_LIMIT}."
        )));
    }
    let updated = conn
        .execute(
            "UPDATE settings SET approval_mode = ?1, max_retry = ?2, telegram_notifications = ?3 WHERE id = 1",
            params![
                input.approval_mode,
                input.max_retry,
                i64::from(input.telegram_notifications)
            ],
        )
        .map_err(|e| CommandError::Storage(e.to_string()))?;
    if updated == 0 {
        return Err(CommandError::Storage(
            "Settings row is missing; the database may need attention.".to_string(),
        ));
    }
    load_settings_conn(conn).map_err(CommandError::Storage)
}

/// Loads the persisted settings.
#[tauri::command]
pub fn load_settings(db: tauri::State<DbState>) -> CommandResult<SettingsDto> {
    db.with_conn(|conn| load_settings_conn(conn).map_err(CommandError::Storage))
}

/// Updates settings and returns the stored result.
#[tauri::command]
pub fn update_settings(
    db: tauri::State<DbState>,
    settings: SettingsInput,
) -> CommandResult<SettingsDto> {
    db.with_conn(|conn| update_settings_conn(conn, &settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;

    fn open_test_db() -> Connection {
        let dir = temp_db_dir();
        let conn = crate::db::open_database(&dir).expect("open test db");
        // Best-effort cleanup; the guard below handles the common case.
        std::mem::forget(dir);
        conn
    }

    fn input(mode: &str, retry: i64, telegram: bool) -> SettingsInput {
        SettingsInput {
            approval_mode: mode.to_string(),
            max_retry: retry,
            telegram_notifications: telegram,
        }
    }

    #[test]
    fn first_launch_loads_safe_defaults() {
        let conn = open_test_db();
        let settings = load_settings_conn(&conn).expect("load");
        assert_eq!(settings.approval_mode, "manual");
        assert_eq!(settings.max_retry, 3);
        assert!(settings.telegram_notifications);
    }

    #[test]
    fn updates_persist_and_round_trip() {
        let conn = open_test_db();
        let stored = update_settings_conn(&conn, &input("autonomous", 7, false)).expect("update");
        assert_eq!(stored.approval_mode, "autonomous");
        assert_eq!(stored.max_retry, 7);
        assert!(!stored.telegram_notifications);

        let reloaded = load_settings_conn(&conn).expect("reload");
        assert_eq!(reloaded.approval_mode, "autonomous");
        assert_eq!(reloaded.max_retry, 7);
        assert!(!reloaded.telegram_notifications);
    }

    #[test]
    fn restore_defaults_updates_persistent_storage() {
        let conn = open_test_db();
        update_settings_conn(&conn, &input("autonomous", 9, false)).expect("change");
        update_settings_conn(&conn, &input("manual", 3, true)).expect("restore");
        let reloaded = load_settings_conn(&conn).expect("reload");
        assert_eq!(reloaded.approval_mode, "manual");
        assert_eq!(reloaded.max_retry, 3);
        assert!(reloaded.telegram_notifications);
    }

    #[test]
    fn invalid_settings_are_rejected_without_writing() {
        let conn = open_test_db();
        assert!(update_settings_conn(&conn, &input("yolo", 3, true)).is_err());
        assert!(update_settings_conn(&conn, &input("manual", 0, true)).is_err());
        assert!(update_settings_conn(&conn, &input("manual", 99, true)).is_err());
        let unchanged = load_settings_conn(&conn).expect("load");
        assert_eq!(unchanged.approval_mode, "manual");
        assert_eq!(unchanged.max_retry, 3);
        assert!(unchanged.telegram_notifications);
    }

    #[test]
    fn settings_survive_a_reopened_connection() {
        let dir = crate::db::temp_db_dir();
        {
            let conn = crate::db::open_database(&dir).expect("open");
            update_settings_conn(&conn, &input("autonomous", 5, false)).expect("update");
        }
        {
            let conn = crate::db::open_database(&dir).expect("reopen");
            let settings = load_settings_conn(&conn).expect("reload");
            assert_eq!(settings.approval_mode, "autonomous");
            assert_eq!(settings.max_retry, 5);
            assert!(!settings.telegram_notifications);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

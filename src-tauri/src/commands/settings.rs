//! Persistent settings operations.
//!
//! Settings are saved preferences only: autonomous execution and Telegram
//! delivery are NOT operational (see docs/GOVERNANCE.md §Execution
//! controls). Persisting them changes nothing about available features.

use crate::db::DbState;
use crate::error::{CommandError, CommandResult};
use rusqlite::{Connection, OptionalExtension, params};
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
    /// Saved Main Agent preference (registry id) — a preference only, not
    /// an operational orchestrator. `None` when no preference is set.
    pub main_agent_id: Option<String>,
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
    let main_agent_id: Option<String> = conn
        .query_row(
            "SELECT main_agent_id FROM settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();
    // A stored preference that no longer matches a registered agent (e.g.
    // hand-edited data) is dropped on load, not surfaced as a broken state.
    let main_agent_id = main_agent_id.filter(|id| {
        if crate::agents::definitions::find_definition(id).is_none() {
            eprintln!("orqen settings: dropping unknown main_agent_id {id:?}");
            false
        } else {
            true
        }
    });
    Ok(SettingsDto {
        approval_mode,
        max_retry,
        telegram_notifications: telegram != 0,
        main_agent_id,
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

/// Saves (or clears with `null`) the preferred Main Agent.
///
/// This is a saved preference only: selecting an agent does not start any
/// task or delegation. Ids are validated against the built-in registry.
#[tauri::command]
pub fn set_main_agent(
    db: tauri::State<DbState>,
    agent_id: Option<String>,
) -> CommandResult<SettingsDto> {
    db.with_conn(|conn| set_main_agent_conn(conn, agent_id.as_deref()))
}

pub fn set_main_agent_conn(
    conn: &Connection,
    agent_id: Option<&str>,
) -> Result<SettingsDto, CommandError> {
    if agent_id.is_some_and(|id| crate::agents::definitions::find_definition(id).is_none()) {
        return Err(CommandError::InvalidInput(format!(
            "\"{}\" is not a supported agent.",
            agent_id.expect("checked above")
        )));
    }
    let updated = conn
        .execute(
            "UPDATE settings SET main_agent_id = ?1 WHERE id = 1",
            params![agent_id],
        )
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error(
                "saving the Main Agent preference",
                &e,
            ))
        })?;
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
    fn main_agent_preference_round_trips_and_clears() {
        let conn = open_test_db();
        assert_eq!(load_settings_conn(&conn).expect("load").main_agent_id, None);

        let stored = set_main_agent_conn(&conn, Some("claude-code")).expect("set");
        assert_eq!(stored.main_agent_id.as_deref(), Some("claude-code"));

        let stored = set_main_agent_conn(&conn, Some("codex")).expect("switch");
        assert_eq!(stored.main_agent_id.as_deref(), Some("codex"));

        let stored = set_main_agent_conn(&conn, None).expect("clear");
        assert_eq!(stored.main_agent_id, None);
    }

    #[test]
    fn main_agent_rejects_unknown_ids_without_writing() {
        let conn = open_test_db();
        assert!(set_main_agent_conn(&conn, Some("ghost")).is_err());
        assert_eq!(load_settings_conn(&conn).expect("load").main_agent_id, None);
    }

    #[test]
    fn corrupted_main_agent_preference_is_sanitized_on_load() {
        let dir = crate::db::temp_db_dir();
        let conn = crate::db::open_database(&dir).expect("open");
        // Simulate a hand-edited database row.
        conn.execute(
            "UPDATE settings SET main_agent_id = 'removed-agent' WHERE id = 1",
            [],
        )
        .expect("corrupt preference");
        let settings = load_settings_conn(&conn).expect("load sanitizes");
        assert_eq!(
            settings.main_agent_id, None,
            "an unknown stored id must not surface as a valid preference"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn main_agent_survives_reopen() {
        let dir = crate::db::temp_db_dir();
        {
            let conn = crate::db::open_database(&dir).expect("open");
            set_main_agent_conn(&conn, Some("codex")).expect("set");
        }
        {
            let conn = crate::db::open_database(&dir).expect("reopen");
            let settings = load_settings_conn(&conn).expect("reload");
            assert_eq!(settings.main_agent_id.as_deref(), Some("codex"));
        }
        let _ = std::fs::remove_dir_all(&dir);
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

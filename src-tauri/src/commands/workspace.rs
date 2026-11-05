//! Persistent project operations.
//!
//! Every mutating command returns the fresh workspace snapshot so the
//! frontend always converges on backend state (no divergence). SQLite
//! access happens on the connection owned by [`crate::DbState`]; the
//! `*_conn` helpers are `pub(crate)` so they are unit-testable without a
//! running Tauri app.

use crate::db::DbState;
use crate::error::{CommandError, CommandResult};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// Maximum project name length — counted in Unicode scalar values (code
/// points), the shared policy from docs/DECISIONS.md (D-012).
pub const MAX_PROJECT_NAME_LENGTH: usize = 80;

/// A persisted project.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

/// Full workspace snapshot: the project list plus the current selection.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshotDto {
    pub projects: Vec<ProjectDto>,
    pub selected_project_id: Option<String>,
}

/// Validates and normalizes a project name (trim, non-empty, code-point
/// limit). Mirrors the frontend validation but is authoritative.
pub fn validated_name(raw: &str) -> Result<String, CommandError> {
    let name = raw.trim();
    let length = name.chars().count();
    if length == 0 {
        return Err(CommandError::InvalidInput(
            "Project name must not be empty.".to_string(),
        ));
    }
    if length > MAX_PROJECT_NAME_LENGTH {
        return Err(CommandError::InvalidInput(format!(
            "Project name must not exceed {MAX_PROJECT_NAME_LENGTH} characters."
        )));
    }
    Ok(name.to_string())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Loads projects ordered by insertion time plus the selected project id.
pub fn load_workspace_conn(conn: &Connection) -> Result<WorkspaceSnapshotDto, String> {
    let mut statement = conn
        .prepare("SELECT id, name, created_at FROM projects ORDER BY created_at, id")
        .map_err(|e| e.to_string())?;
    let projects = statement
        .query_map([], |row| {
            Ok(ProjectDto {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let selected_project_id: Option<String> = conn
        .query_row(
            "SELECT selected_project_id FROM settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();

    Ok(WorkspaceSnapshotDto {
        projects,
        selected_project_id,
    })
}

/// Creates a project and selects it, atomically.
///
/// The INSERT and the selection UPDATE share one transaction, and the
/// snapshot is read inside that transaction: a commit failure rolls
/// everything back, so the reported state always matches the database —
/// no partial success, no lost selection. Restores the Phase 1 contract:
/// a newly created project is selected immediately.
pub fn create_project_conn(
    conn: &mut Connection,
    raw_name: &str,
) -> Result<WorkspaceSnapshotDto, CommandError> {
    let name = validated_name(raw_name)?;
    let id = uuid::Uuid::new_v4().to_string();
    let transaction = conn
        .transaction()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the project", &e)))?;

    let result = transaction.execute(
        "INSERT INTO projects (id, name, created_at) VALUES (?1, ?2, ?3)",
        params![id, name, now_rfc3339()],
    );
    if let Err(error) = result {
        return Err(map_project_write_error(&error, &name));
    }
    transaction
        .execute(
            "UPDATE settings SET selected_project_id = ?1 WHERE id = 1",
            params![id],
        )
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the project", &e)))?;

    // Read-your-writes inside the transaction: the snapshot matches the
    // state this transaction is about to commit.
    let snapshot = load_workspace_conn(&transaction).map_err(CommandError::Storage)?;
    transaction
        .commit()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the project", &e)))?;
    Ok(snapshot)
}

/// Selects a project and returns the committed snapshot.
pub fn select_project_conn(
    conn: &mut Connection,
    id: &str,
) -> Result<WorkspaceSnapshotDto, CommandError> {
    let transaction = conn.transaction().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("updating the selection", &e))
    })?;
    let updated = transaction
        .execute(
            "UPDATE settings SET selected_project_id = ?1 WHERE id = 1",
            params![id],
        )
        .map_err(|e| {
            if e.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                CommandError::InvalidInput("Unknown project.".to_string())
            } else {
                CommandError::Storage(crate::db::describe_error("updating the selection", &e))
            }
        })?;
    if updated == 0 {
        return Err(CommandError::Storage(
            "Settings row is missing; the database may need attention.".to_string(),
        ));
    }
    let snapshot = load_workspace_conn(&transaction).map_err(CommandError::Storage)?;
    transaction.commit().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("updating the selection", &e))
    })?;
    Ok(snapshot)
}

/// Deletes a project and returns the committed snapshot (deleting the
/// selected project clears the selection via the foreign key).
pub fn delete_project_conn(
    conn: &mut Connection,
    id: &str,
) -> Result<WorkspaceSnapshotDto, CommandError> {
    let transaction = conn.transaction().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("deleting the project", &e))
    })?;
    let deleted = transaction
        .execute("DELETE FROM projects WHERE id = ?1", params![id])
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error("deleting the project", &e))
        })?;
    if deleted == 0 {
        return Err(CommandError::InvalidInput("Unknown project.".to_string()));
    }
    let snapshot = load_workspace_conn(&transaction).map_err(CommandError::Storage)?;
    transaction.commit().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("deleting the project", &e))
    })?;
    Ok(snapshot)
}

/// Maps a project INSERT failure to the friendliest accurate error.
fn map_project_write_error(error: &rusqlite::Error, name: &str) -> CommandError {
    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
        return CommandError::InvalidInput(format!("A project named \"{name}\" already exists."));
    }
    CommandError::Storage(crate::db::describe_error("saving the project", error))
}

/// Loads the workspace snapshot.
#[tauri::command]
pub fn load_workspace(db: tauri::State<DbState>) -> CommandResult<WorkspaceSnapshotDto> {
    db.with_conn(|conn| load_workspace_conn(conn).map_err(CommandError::Storage))
}

/// Creates a project and returns the fresh snapshot.
#[tauri::command]
pub fn create_project(
    db: tauri::State<DbState>,
    name: String,
) -> CommandResult<WorkspaceSnapshotDto> {
    db.with_conn(|conn| create_project_conn(conn, &name))
}

/// Selects a project and returns the fresh snapshot.
#[tauri::command]
pub fn select_project(
    db: tauri::State<DbState>,
    id: String,
) -> CommandResult<WorkspaceSnapshotDto> {
    db.with_conn(|conn| select_project_conn(conn, &id))
}

/// Deletes a project and returns the fresh snapshot (selection updates
/// handled by the foreign key).
#[tauri::command]
pub fn delete_project(
    db: tauri::State<DbState>,
    id: String,
) -> CommandResult<WorkspaceSnapshotDto> {
    db.with_conn(|conn| delete_project_conn(conn, &id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;

    fn open_test_db() -> (temp_dir_guard::TempDir, Connection) {
        let dir = temp_db_dir();
        let conn = crate::db::open_database(&dir).expect("open test db");
        (temp_dir_guard::TempDir(dir), conn)
    }

    /// Removes the temp directory when dropped (best effort).
    mod temp_dir_guard {
        use std::path::PathBuf;
        pub struct TempDir(pub PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn validated_name_trims_and_enforces_code_point_limit() {
        assert_eq!(validated_name("  Demo  ").expect("valid"), "Demo",);
        assert!(validated_name("   ").is_err());
        assert!(validated_name("").is_err());
        // 80 emoji are 80 code points (160 UTF-16 units) — accepted.
        assert!(validated_name(&"🙂".repeat(80)).is_ok());
        assert!(validated_name(&"🙂".repeat(81)).is_err());
        assert!(validated_name(&"ক".repeat(81)).is_err());
    }

    #[test]
    fn create_select_delete_round_trip_returns_snapshots() {
        let (_guard, mut conn) = open_test_db();

        // Phase 1 contract restored: creating selects the new project.
        let snapshot = create_project_conn(&mut conn, "  Alpha  ").expect("create");
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.projects[0].name, "Alpha");
        let alpha_id = snapshot.projects[0].id.clone();
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(alpha_id.as_str()),
            "creation must select the new project"
        );

        let snapshot = create_project_conn(&mut conn, "Beta").expect("create beta");
        assert_eq!(snapshot.projects.len(), 2);
        let beta_id = snapshot.projects[1].id.clone();
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(beta_id.as_str()),
            "the newest project becomes the selection"
        );

        // Explicit selection still works.
        let snapshot = select_project_conn(&mut conn, &alpha_id).expect("select");
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(alpha_id.as_str())
        );

        // Deleting the selected project clears the selection via the
        // foreign key (ON DELETE SET NULL) — there is no "revert to the
        // previous selection" behavior.
        let snapshot = delete_project_conn(&mut conn, &alpha_id).expect("delete");
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(
            snapshot.selected_project_id, None,
            "deleting the selected project clears the selection"
        );

        // The remaining project can be selected again explicitly.
        let snapshot = select_project_conn(&mut conn, &beta_id).expect("select beta");
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(beta_id.as_str())
        );
        let snapshot = delete_project_conn(&mut conn, &beta_id).expect("delete selected");
        assert_eq!(snapshot.projects.len(), 0);
        assert_eq!(snapshot.selected_project_id, None);
    }

    #[test]
    fn create_selects_the_new_project_atomically() {
        let (_guard, mut conn) = open_test_db();
        let snapshot = create_project_conn(&mut conn, "Fresh").expect("create");
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(snapshot.projects[0].id.as_str()),
            "the returned snapshot must reflect the committed selection"
        );
    }

    #[test]
    fn create_selection_survives_reopen() {
        let dir = temp_db_dir();
        {
            let mut conn = crate::db::open_database(&dir).expect("open");
            let snapshot = create_project_conn(&mut conn, "Chosen").expect("create");
            assert_eq!(
                snapshot.selected_project_id,
                Some(snapshot.projects[0].id.clone())
            );
        }
        {
            let conn = crate::db::open_database(&dir).expect("reopen");
            let snapshot = load_workspace_conn(&conn).expect("load");
            assert_eq!(snapshot.projects.len(), 1);
            assert_eq!(
                snapshot.selected_project_id,
                Some(snapshot.projects[0].id.clone()),
                "the selection made at creation must survive reopening"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_names_are_rejected_with_input_error() {
        let (_guard, mut conn) = open_test_db();
        create_project_conn(&mut conn, "Demo").expect("first");
        let error = create_project_conn(&mut conn, "  Demo  ").expect_err("duplicate");
        match error {
            CommandError::InvalidInput(message) => {
                assert!(message.contains("already exists"), "got: {message}");
            }
            other => panic!("expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn unknown_ids_are_rejected() {
        let (_guard, mut conn) = open_test_db();
        assert!(select_project_conn(&mut conn, "nope").is_err());
        assert!(delete_project_conn(&mut conn, "nope").is_err());
    }

    #[test]
    fn snapshots_survive_a_reopened_connection() {
        let dir = temp_db_dir();
        {
            let mut conn = crate::db::open_database(&dir).expect("open");
            create_project_conn(&mut conn, "Persisted").expect("create");
        }
        {
            let conn = crate::db::open_database(&dir).expect("reopen");
            let snapshot = load_workspace_conn(&conn).expect("load");
            assert_eq!(snapshot.projects.len(), 1);
            assert_eq!(snapshot.projects[0].name, "Persisted");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

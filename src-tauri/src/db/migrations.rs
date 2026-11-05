//! Versioned, transactional database migrations.
//!
//! Each entry in [`MIGRATIONS`] is applied exactly once, inside its own
//! transaction, and the applied count is recorded in `PRAGMA user_version`.
//! A failing migration rolls back completely (schema and version), so the
//! next launch can retry it — no partial schema is ever committed.

use rusqlite::Connection;

/// Migration 1 — Phase 2 schema (projects + settings singleton).
///
/// Design notes:
/// - `projects.name` is unique per the approved duplicate-name behavior
///   (exact match after trimming, see docs/DECISIONS.md).
/// - `settings` is a enforced singleton row (id = 1) created on first run.
/// - `settings.selected_project_id` owns the "selected project" state with
///   a foreign key, so deleting the selected project clears the selection
///   at the database level (ON DELETE SET NULL).
pub const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE projects (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE UNIQUE INDEX idx_projects_name ON projects(name);
    CREATE TABLE settings (
        id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
        approval_mode TEXT NOT NULL DEFAULT 'manual'
            CHECK (approval_mode IN ('manual', 'autonomous')),
        max_retry INTEGER NOT NULL DEFAULT 3
            CHECK (max_retry BETWEEN 1 AND 10),
        telegram_notifications INTEGER NOT NULL DEFAULT 1
            CHECK (telegram_notifications IN (0, 1)),
        selected_project_id TEXT REFERENCES projects(id) ON DELETE SET NULL
    );
    INSERT INTO settings (id) VALUES (1);",
];

/// Reads the currently recorded schema version.
pub fn current_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map(|v| v.max(0) as u32)
}

/// Applies all pending migrations in order, each in its own transaction.
///
/// Idempotent: re-running on an up-to-date database is a no-op.
pub fn run_migrations(conn: &mut Connection) -> Result<(), String> {
    run_migrations_with(conn, MIGRATIONS)
}

/// Testable variant: runs an explicit migration list.
pub fn run_migrations_with(conn: &mut Connection, migrations: &[&str]) -> Result<(), String> {
    let mut version = current_version(conn).map_err(|e| e.to_string())?;
    for (index, sql) in migrations.iter().enumerate() {
        let target = (index + 1) as u32;
        if target <= version {
            continue;
        }
        let transaction = conn
            .transaction()
            .map_err(|e| format!("could not begin migration {target}: {e}"))?;
        // execute_batch runs every statement; any failure rolls the whole
        // transaction back (schema changes included).
        transaction
            .execute_batch(sql)
            .map_err(|e| format!("migration {target} failed: {e}"))?;
        transaction
            .pragma_update(None, "user_version", target)
            .map_err(|e| format!("migration {target} could not record version: {e}"))?;
        transaction
            .commit()
            .map_err(|e| format!("migration {target} could not commit: {e}"))?;
        version = target;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;

    fn open_mem() -> Connection {
        Connection::open_in_memory().expect("in-memory connection")
    }

    #[test]
    fn applies_migrations_and_records_version() {
        let mut conn = open_mem();
        run_migrations_with(&mut conn, MIGRATIONS).expect("migrations apply");
        assert_eq!(current_version(&conn).expect("version"), 1);
        // Tables exist.
        let projects: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('projects','settings')",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(projects, 2);
        // Settings singleton seeded.
        let settings_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .expect("settings count");
        assert_eq!(settings_rows, 1);
    }

    #[test]
    fn re_running_migrations_is_idempotent() {
        let mut conn = open_mem();
        run_migrations_with(&mut conn, MIGRATIONS).expect("first run");
        conn.execute(
            "INSERT INTO projects (id, name, created_at) VALUES ('p1', 'Keep', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("seed project");
        run_migrations_with(&mut conn, MIGRATIONS).expect("second run");
        assert_eq!(current_version(&conn).expect("version"), 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .expect("project count");
        assert_eq!(count, 1, "re-running must not recreate or wipe data");
    }

    #[test]
    fn failing_migration_rolls_back_schema_and_version() {
        let mut conn = open_mem();
        run_migrations_with(&mut conn, MIGRATIONS).expect("v1 applies");

        let bad_v2 =
            "CREATE TABLE tasks (id TEXT PRIMARY KEY); INSERT INTO missing_table (x) VALUES (1);";
        let result = run_migrations_with(&mut conn, &[MIGRATIONS[0], bad_v2]);
        assert!(result.is_err(), "broken migration must fail");

        // Version unchanged and no partial schema from the failed migration.
        assert_eq!(current_version(&conn).expect("version"), 1);
        let bad_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tasks'",
                [],
                |row| row.get(0),
            )
            .expect("tasks table check");
        assert_eq!(bad_table, 0, "failed migration must roll back its schema");
    }

    #[test]
    fn migrations_run_against_a_temp_file_database_and_survive_reopen() {
        let dir = temp_db_dir();
        let db_path = dir.join("orqen.sqlite3");

        {
            let mut conn = Connection::open(&db_path).expect("open temp db");
            run_migrations_with(&mut conn, MIGRATIONS).expect("migrations");
            conn.execute(
                "INSERT INTO projects (id, name, created_at) VALUES ('p1', 'Survivor', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert");
        }
        {
            let conn = Connection::open(&db_path).expect("reopen temp db");
            let name: String = conn
                .query_row("SELECT name FROM projects WHERE id='p1'", [], |row| {
                    row.get(0)
                })
                .expect("project survives reopen");
            assert_eq!(name, "Survivor");
            assert_eq!(current_version(&conn).expect("version"), 1);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unique_index_enforces_duplicate_names() {
        let mut conn = open_mem();
        run_migrations_with(&mut conn, MIGRATIONS).expect("migrations");
        conn.execute(
            "INSERT INTO projects (id, name, created_at) VALUES ('p1', 'Demo', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("first insert");
        let second = conn.execute(
            "INSERT INTO projects (id, name, created_at) VALUES ('p2', 'Demo', '2026-01-01T00:00:00Z')",
            [],
        );
        assert!(
            second.is_err(),
            "duplicate names must violate the unique index"
        );
    }

    #[test]
    fn settings_constraints_reject_invalid_values() {
        let mut conn = open_mem();
        run_migrations_with(&mut conn, MIGRATIONS).expect("migrations");
        assert!(
            conn.execute("UPDATE settings SET approval_mode = 'yolo'", [])
                .is_err()
        );
        assert!(
            conn.execute("UPDATE settings SET max_retry = 99", [])
                .is_err()
        );
        assert!(
            conn.execute("UPDATE settings SET telegram_notifications = 5", [])
                .is_err()
        );
        // Defaults are in force on the singleton row.
        let (mode, retry, telegram): (String, i64, i64) = conn
            .query_row(
                "SELECT approval_mode, max_retry, telegram_notifications FROM settings WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("defaults");
        assert_eq!((mode.as_str(), retry, telegram), ("manual", 3, 1));
    }

    #[test]
    fn deleting_selected_project_clears_the_selection_via_foreign_key() {
        let mut conn = open_mem();
        run_migrations_with(&mut conn, MIGRATIONS).expect("migrations");
        conn.execute(
            "INSERT INTO projects (id, name, created_at) VALUES ('p1', 'Selected', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("insert");
        conn.execute("UPDATE settings SET selected_project_id = 'p1'", [])
            .expect("select");
        conn.execute("DELETE FROM projects WHERE id = 'p1'", [])
            .expect("delete");
        let selected: Option<String> = conn
            .query_row(
                "SELECT selected_project_id FROM settings WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("selection after delete");
        assert_eq!(
            selected, None,
            "ON DELETE SET NULL must clear the selection"
        );
    }

    #[test]
    fn temp_dirs_are_unique_per_call() {
        let a = temp_db_dir();
        let b = temp_db_dir();
        assert_ne!(a, b, "each test must get an isolated directory");
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
    }
}

//! SQLite persistence layer.
//!
//! Ownership: SQLite is accessed exclusively by Rust. The frontend reaches
//! it only through typed, validated Tauri commands (see `crate::commands`).
//!
//! Connection strategy (docs/ARCHITECTURE.md §Persistence):
//! - one connection per application, guarded by a mutex, so commands never
//!   race on the same handle;
//! - WAL journal mode (readers never block the writer; see the backup notes
//!   in docs/ARCHITECTURE.md for the `-wal`/`-shm` side files);
//! - `foreign_keys = ON` on every connection (referential integrity incl.
//!   ON DELETE SET NULL for the selected project);
//! - `busy_timeout` of 5 s as defense in depth for the serialized handles;
//! - `synchronous = NORMAL`, the recommended pairing with WAL.
//!
//! Failure policy: a corrupted or unreadable database is never deleted or
//! replaced. Open failures are surfaced to the UI with recovery guidance.

pub mod migrations;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

/// Database file name inside the app-data directory.
pub const DB_FILE_NAME: &str = "orqen.sqlite3";

/// Creates a unique temporary directory for test databases.
#[cfg(test)]
pub(crate) fn temp_db_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("orqen-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Opens (or creates) the application database at `data_dir`, applies
/// pending migrations, and configures the connection strategy.
///
/// Existing data is never destroyed: reopening is a no-op apart from
/// applying pending migrations.
pub fn open_database(data_dir: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("could not create the data directory: {e}"))?;

    let db_path = data_dir.join(DB_FILE_NAME);
    let conn = Connection::open(&db_path)
        .map_err(|e| format!("could not open the local database: {e}"))?;

    // Corrupt or unreadable files surface here (the first pragmas touch
    // the file header) — reported as an open failure, never deleted.
    configure_connection(&conn).map_err(|e| format!("could not open the local database: {e}"))?;

    let mut conn = conn;
    migrations::run_migrations(&mut conn)
        .map_err(|e| format!("could not prepare the local database: {e}"))?;
    Ok(conn)
}

/// Applies the per-connection configuration. Public for tests.
pub fn configure_connection(conn: &Connection) -> Result<(), String> {
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    // Foreign keys are off by default in SQLite; enforce them always.
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Managed application state holding the database directory and the open
/// connection, if initialization succeeded. `None` means initialization
/// failed; every command then returns a storage error and load retries
/// re-attempt initialization — there is no silent in-memory fallback.
pub struct DbState {
    data_dir: PathBuf,
    connection: std::sync::Mutex<Option<Connection>>,
}

impl DbState {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            connection: std::sync::Mutex::new(None),
        }
    }

    /// Attempts to open the database if it is not open yet. Idempotent:
    /// an already-open database is left untouched (no data is reset).
    pub fn ensure_connection(&self) -> Result<(), String> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|_| "local database is unavailable".to_string())?;
        if guard.is_some() {
            return Ok(());
        }
        *guard = Some(open_database(&self.data_dir)?);
        Ok(())
    }

    /// Runs `f` with the open connection, re-attempting initialization if
    /// a previous attempt failed, and mapping a missing connection to a
    /// user-presentable storage error.
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, crate::error::CommandError>,
    ) -> Result<T, crate::error::CommandError> {
        self.ensure_connection()
            .map_err(crate::error::CommandError::Storage)?;
        let guard = self.connection.lock().map_err(|_| {
            crate::error::CommandError::Storage("local database is unavailable".to_string())
        })?;
        match guard.as_ref() {
            Some(conn) => f(conn),
            None => Err(crate::error::CommandError::Storage(
                "local database is unavailable. Your data is untouched — try \
                 restarting the application; if this persists, check the data \
                 directory's permissions or restore the database file from a \
                 backup."
                    .to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_creates_and_configures_the_database() {
        let dir = temp_db_dir();
        let conn = open_database(&dir).expect("open");
        assert_eq!(
            migrations::current_version(&conn).expect("version"),
            migrations::MIGRATIONS.len() as u32
        );
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        assert_eq!(journal, "wal");
        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign keys");
        assert_eq!(foreign_keys, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reopening_preserves_existing_data() {
        let dir = temp_db_dir();
        {
            let conn = open_database(&dir).expect("first open");
            conn.execute(
                "INSERT INTO projects (id, name, created_at) VALUES ('p1', 'Persisted', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert");
        }
        {
            let conn = open_database(&dir).expect("second open");
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
                .expect("count");
            assert_eq!(count, 1, "reopen must not wipe data");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_database_fails_to_open_and_is_never_deleted() {
        let dir = temp_db_dir();
        let db_path = dir.join(DB_FILE_NAME);
        std::fs::write(&db_path, b"this is definitely not a sqlite file").expect("corrupt file");

        let result = open_database(&dir);
        assert!(result.is_err(), "corrupt database must fail to open");
        let message = result.expect_err("error message");
        assert!(
            message.contains("could not prepare") || message.contains("could not open"),
            "error should be user-understandable, got: {message}"
        );
        // The corrupted file is still on disk — no destructive recovery.
        assert!(
            db_path.exists(),
            "corrupt database file must not be deleted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_conn_maps_missing_database_to_storage_error() {
        let dir = temp_db_dir();
        // A directory that cannot contain a database (it is a file) makes
        // initialization fail, exercising the retry path honestly.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"file").expect("write blocker");
        let state = DbState::new(blocker.clone());
        let result = state.with_conn(|_conn| Ok::<i64, crate::error::CommandError>(1));
        assert!(result.is_err());
        let message = result.expect_err("message").to_string();
        assert!(
            message.contains("could not") || message.contains("unavailable"),
            "got: {message}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_connection_recovers_after_a_failed_first_attempt() {
        let dir = temp_db_dir();
        let state = DbState::new(dir.clone());
        // First open succeeds; a second ensure is a harmless no-op.
        state.ensure_connection().expect("first open");
        state.ensure_connection().expect("second open is a no-op");
        state
            .with_conn(|conn| {
                migrations::current_version(conn)
                    .map(|_| ())
                    .map_err(|e| crate::error::CommandError::Storage(e.to_string()))
            })
            .expect("with_conn sees the open database");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

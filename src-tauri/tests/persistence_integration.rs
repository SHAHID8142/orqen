//! End-to-end persistence integration tests.
//!
//! These exercise the REAL production code paths (open_database, the same
//! command-level helpers the Tauri commands call) against isolated
//! temporary file databases, reopening connections to prove durability.
//! They are distinct from unit tests and from interactive GUI verification.

use orqen_lib::commands::settings::{SettingsInput, load_settings_conn, update_settings_conn};
use orqen_lib::commands::workspace::{
    create_project_conn, delete_project_conn, load_workspace_conn, select_project_conn,
};
use orqen_lib::db::open_database;

fn temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("orqen-integration-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn settings_input(mode: &str, retry: i64, telegram: bool) -> SettingsInput {
    SettingsInput {
        approval_mode: mode.to_string(),
        max_retry: retry,
        telegram_notifications: telegram,
    }
}

/// Scenario walkthrough (Task 5, items 1–8): create → auto-select →
/// reopen → delete selected → reselect → settings across restart →
/// duplicate rejection → atomicity.
#[test]
fn full_persistence_lifecycle() {
    let dir = temp_dir();
    let db_path = dir.join(orqen_lib::db::DB_FILE_NAME);

    // 1 + 2. Create Project A, then Project B — each creation selects it.
    let (id_a, id_b) = {
        let mut conn = open_database(&dir).expect("open");
        let snap_a = create_project_conn(&mut conn, "Project A").expect("create A");
        assert_eq!(snap_a.projects.len(), 1);
        assert_eq!(
            snap_a.selected_project_id.as_deref(),
            Some(snap_a.projects[0].id.as_str()),
            "Project A must be selected on creation"
        );
        let id_a = snap_a.projects[0].id.clone();

        let snap_b = create_project_conn(&mut conn, "Project B").expect("create B");
        assert_eq!(snap_b.projects.len(), 2);
        assert_eq!(
            snap_b.selected_project_id.as_deref(),
            Some(snap_b.projects[1].id.as_str()),
            "Project B must be selected on creation"
        );
        (id_a, snap_b.projects[1].id.clone())
    };
    assert_ne!(id_a, id_b, "project ids must be unique");

    // 3. Restart/reopen: both projects and the selection persist.
    {
        let mut conn = open_database(&dir).expect("reopen");
        let snapshot = load_workspace_conn(&conn).expect("load");
        assert_eq!(snapshot.projects.len(), 2);
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(id_b.as_str()),
            "selection must survive the restart"
        );

        // 4. Delete the selected Project B — selection clears safely.
        let snapshot = delete_project_conn(&mut conn, &id_b).expect("delete B");
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.selected_project_id, None);

        // 5. Select Project A; a settings change rides along for step 6.
        let snapshot = select_project_conn(&mut conn, &id_a).expect("select A");
        assert_eq!(snapshot.selected_project_id.as_deref(), Some(id_a.as_str()));
        let stored = update_settings_conn(&conn, &settings_input("autonomous", 7, false))
            .expect("update settings");
        assert_eq!(stored.approval_mode, "autonomous");
        assert_eq!(stored.max_retry, 7);
        assert!(!stored.telegram_notifications);
    }

    // 5 + 6. Restart: Project A remains selected, settings persist.
    {
        let mut conn = open_database(&dir).expect("reopen again");
        let snapshot = load_workspace_conn(&conn).expect("load");
        assert_eq!(snapshot.projects.len(), 1);
        assert_eq!(snapshot.projects[0].name, "Project A");
        assert_eq!(
            snapshot.selected_project_id.as_deref(),
            Some(id_a.as_str()),
            "Project A must remain selected after restart"
        );
        let settings = load_settings_conn(&conn).expect("load settings");
        assert_eq!(settings.approval_mode, "autonomous");
        assert_eq!(settings.max_retry, 7);
        assert!(!settings.telegram_notifications);

        // 7. Duplicate creation: correct error, no data loss.
        let error =
            create_project_conn(&mut conn, "  Project A  ").expect_err("duplicate must fail");
        match error {
            orqen_lib::error::CommandError::InvalidInput(message) => {
                assert!(message.contains("already exists"), "got: {message}");
            }
            other => panic!("expected InvalidInput, got: {other:?}"),
        }
        let snapshot = load_workspace_conn(&conn).expect("load after duplicate");
        assert_eq!(
            snapshot.projects.len(),
            1,
            "failed create must not change data"
        );
    }

    // The database file with all committed state is on disk.
    assert!(db_path.exists());

    // 8. Transaction rollback: a failed migration leaves no partial schema
    //    (covered in unit tests); here we prove a failed project write is
    //    atomic — the invalid input is rejected before any write happens.
    {
        let mut conn = open_database(&dir).expect("open");
        let before = load_workspace_conn(&conn).expect("load");
        assert!(
            create_project_conn(&mut conn, "   ").is_err(),
            "empty name rejected"
        );
        assert!(
            create_project_conn(&mut conn, &"ক".repeat(81)).is_err(),
            "over-limit rejected"
        );
        let after = load_workspace_conn(&conn).expect("load");
        assert_eq!(
            before.projects.len(),
            after.projects.len(),
            "rejected creates must not touch the database"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Concurrent command execution: two threads issuing mutations through
/// separate DbState handles on the same directory must serialize safely
/// and end with a consistent database (no corruption, both writes present).
#[test]
fn concurrent_mutations_serialize_safely() {
    let dir = temp_dir();
    {
        let mut conn = open_database(&dir).expect("open");
        let _ = create_project_conn(&mut conn, "Seed").expect("seed");
    }

    let dir_for_threads = dir.clone();
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let dir = dir_for_threads.clone();
            std::thread::spawn(move || {
                // Each thread opens its own connection (worst case for
                // contention); WAL + busy_timeout must keep this safe.
                let mut conn = open_database(&dir).expect("thread open");
                let name = format!("Concurrent {i}");
                let snapshot = create_project_conn(&mut conn, &name).expect("create");
                // Concurrent threads may see each other's committed rows —
                // the invariant is that this thread's own project exists and
                // is selected in the returned, committed snapshot.
                assert!(
                    snapshot.projects.iter().any(|p| {
                        p.name == name
                            && Some(p.id.as_str()) == snapshot.selected_project_id.as_deref()
                    }),
                    "own project must exist and be selected in the committed snapshot"
                );
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread must not panic");
    }

    // Final state: exactly the seed + 4 concurrent projects (unique-name
    // constraint means a lost update would surface as an error, not data
    // loss; all four names are distinct so all must exist).
    let conn = open_database(&dir).expect("final open");
    let snapshot = load_workspace_conn(&conn).expect("final load");
    assert_eq!(
        snapshot.projects.len(),
        5,
        "all concurrent writes must persist"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase 3 migration upgrade: a real Phase 2 (v1) database with project
/// and settings data is upgraded to v2 without losing anything, and the
/// new Main Agent preference becomes usable.
#[test]
fn phase2_database_upgrades_to_v2_preserving_data() {
    use orqen_lib::commands::settings::set_main_agent_conn;
    use orqen_lib::db::migrations;

    let dir = temp_dir();
    let db_path = dir.join(orqen_lib::db::DB_FILE_NAME);

    // Build a genuine v1 database directly: only the first migration,
    // one project, default settings — exactly what a Phase 2 install had.
    {
        let conn = rusqlite::Connection::open(&db_path).expect("create v1 db");
        conn.execute_batch(orqen_lib::db::migrations::MIGRATIONS[0])
            .expect("v1 schema");
        conn.pragma_update(None, "user_version", 1)
            .expect("v1 version");
        conn.execute(
            "INSERT INTO projects (id, name, created_at) VALUES ('pre-1', 'Pre-upgrade Project', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("seed project");
    }

    // Upgrade: open_database applies v2 on top of the v1 database.
    {
        let mut conn = open_database(&dir).expect("upgrade open");
        assert_eq!(
            migrations::current_version(&conn).expect("version"),
            migrations::MIGRATIONS.len() as u32,
            "upgrade must reach the current version"
        );

        // Data written before the upgrade is still present and usable.
        let snapshot =
            create_project_conn(&mut conn, "Post-upgrade Project").expect("create after upgrade");
        assert_eq!(
            snapshot.projects.len(),
            2,
            "pre-upgrade project must survive"
        );
        let selected = snapshot
            .projects
            .iter()
            .find(|p| Some(p.id.as_str()) == snapshot.selected_project_id.as_deref())
            .expect("selection must reference a real project");
        assert_eq!(selected.name, "Post-upgrade Project");

        // The new preference column is usable and persists.
        let stored = set_main_agent_conn(&conn, Some("codex")).expect("set main agent");
        assert_eq!(stored.main_agent_id.as_deref(), Some("codex"));
    }

    // Restart: everything survives.
    {
        let conn = open_database(&dir).expect("reopen after upgrade");
        let snapshot = load_workspace_conn(&conn).expect("load");
        assert_eq!(snapshot.projects.len(), 2);
        assert!(
            snapshot
                .projects
                .iter()
                .any(|p| p.name == "Pre-upgrade Project"),
            "the pre-upgrade project must survive the upgrade"
        );
        let settings = load_settings_conn(&conn).expect("load settings");
        assert_eq!(settings.main_agent_id.as_deref(), Some("codex"));
        assert_eq!(settings.max_retry, 3);
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// Main Agent preference lifecycle across restarts.
#[test]
fn main_agent_preference_survives_restart() {
    use orqen_lib::commands::settings::{load_settings_conn, set_main_agent_conn};

    let dir = temp_dir();
    {
        let conn = open_database(&dir).expect("open");
        set_main_agent_conn(&conn, Some("claude-code")).expect("set");
    }
    {
        let conn = open_database(&dir).expect("reopen");
        let settings = load_settings_conn(&conn).expect("load");
        assert_eq!(settings.main_agent_id.as_deref(), Some("claude-code"));

        // Clearing the preference persists too.
        set_main_agent_conn(&conn, None).expect("clear");
    }
    {
        let conn = open_database(&dir).expect("reopen after clear");
        let settings = load_settings_conn(&conn).expect("load");
        assert_eq!(settings.main_agent_id, None);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

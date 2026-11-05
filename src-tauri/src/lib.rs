//! Orqen desktop application entry point (library side).
//!
//! Modules:
//! - `db`: SQLite persistence (connection strategy, migrations).
//! - `commands`: Tauri IPC commands exposed to the frontend.
//! - `error`: shared structured error type for commands.

mod commands;
mod db;
mod error;

use db::DbState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Resolve the OS-managed application data directory for the
            // configured identifier and open the database. A failure here
            // must not abort the launch: DbState stays empty and every
            // persistence command surfaces a storage error with a retry
            // path (no silent in-memory fallback).
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("could not resolve the data directory: {e}"))?;
            let state = DbState::new(data_dir);
            if let Err(message) = state.ensure_connection() {
                eprintln!("database initialization failed: {message}");
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::app_info::echo_ping,
            commands::workspace::load_workspace,
            commands::workspace::create_project,
            commands::workspace::select_project,
            commands::workspace::delete_project,
            commands::settings::load_settings,
            commands::settings::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

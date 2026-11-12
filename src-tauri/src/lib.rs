//! Orqen desktop application entry point (library side).
//!
//! Modules:
//! - `db`: SQLite persistence (connection strategy, migrations).
//! - `commands`: Tauri IPC commands exposed to the frontend.
//! - `error`: shared structured error type for commands.

pub mod agents;
pub mod commands;
pub mod db;
pub mod error;

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
            let state = DbState::new(data_dir.clone());
            if let Err(message) = state.ensure_connection() {
                eprintln!("database initialization failed: {message}");
            }
            app.manage(state);
            app.manage(crate::agents::runner::RunRegistry::default());
            app.manage(crate::agents::eligibility::EligibilityLedger::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agents::list_agents,
            commands::agents::discover_agent,
            commands::agents::verify_agent,
            commands::agents::start_diagnostic_run,
            commands::agents::get_diagnostic_run,
            commands::agents::cancel_diagnostic_run,
            commands::app_info::get_app_info,
            commands::app_info::echo_ping,
            commands::workspace::load_workspace,
            commands::workspace::create_project,
            commands::workspace::select_project,
            commands::workspace::delete_project,
            commands::settings::load_settings,
            commands::settings::update_settings,
            commands::settings::set_main_agent,
            commands::chat::list_chat_modes,
            commands::chat::list_conversations,
            commands::chat::open_conversation,
            commands::chat::create_conversation,
            commands::chat::rename_conversation,
            commands::chat::set_conversation_mode,
            commands::chat::delete_conversation,
            commands::chat::append_message,
            commands::chat::export_conversation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

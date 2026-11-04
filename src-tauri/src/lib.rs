//! Orqen desktop application entry point (library side).
//!
//! Modules:
//! - `commands`: Tauri IPC commands exposed to the frontend.
//! - `error`: shared structured error type for commands.

mod commands;
mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_info::get_app_info,
            commands::app_info::echo_ping,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

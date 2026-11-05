//! Tauri command modules.
//!
//! Each command group lives in its own module and is registered in
//! `crate::run()` via `tauri::generate_handler!`.

pub mod agents;
pub mod app_info;
pub mod settings;
pub mod workspace;

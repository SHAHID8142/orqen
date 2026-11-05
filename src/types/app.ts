/**
 * Shared domain types for the Orqen frontend.
 *
 * Types here mirror the data structures serialized by the Rust backend
 * (see `src-tauri/src/commands/app_info.rs`). When a backend payload
 * changes, update both sides in the same change set.
 */

/** Information about the running Orqen application, served by the Rust core. */
export interface AppInfo {
  /** Product name as configured in `src-tauri/tauri.conf.json`. */
  name: string;
  /** Application version. */
  version: string;
  /** Operating system the application is running on (e.g. "macos", "windows"). */
  os: string;
}

/** Severity-free discriminators for structured errors returned by Tauri commands. */
export type CommandErrorKind = "invalid_input" | "storage" | "internal";

/**
 * Shape of the error payload returned by Rust commands.
 * Mirrors `CommandError` in `src-tauri/src/error.rs`.
 */
export interface CommandErrorPayload {
  kind: CommandErrorKind;
  message: string;
}

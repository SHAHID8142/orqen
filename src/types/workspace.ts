/**
 * Workspace & settings domain types, mirroring the Rust DTOs in
 * `src-tauri/src/commands/{workspace,settings}.rs`. Both sides change in
 * the same commit.
 */

/** A persisted project. */
export interface Project {
  id: string;
  name: string;
  createdAt: string;
}

/** Full workspace snapshot: projects plus the current selection. */
export interface WorkspaceSnapshot {
  projects: Project[];
  selectedProjectId: string | null;
}

/** Persisted settings (saved preferences only — see docs/GOVERNANCE.md). */
export interface Settings {
  approvalMode: "manual" | "autonomous";
  maxRetry: number;
  telegramNotifications: boolean;
}

/** Payload for settings updates. */
export interface SettingsInput {
  approvalMode: "manual" | "autonomous";
  maxRetry: number;
  telegramNotifications: boolean;
}

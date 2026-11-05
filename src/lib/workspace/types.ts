/**
 * Workspace domain model — in-memory only.
 *
 * Nothing here persists across application restarts; persistence arrives
 * in Phase 2 (SQLite, see docs/ROADMAP.md). Types mirror what the UI
 * needs; no speculative fields.
 */

/** A user-created workspace project. */
export interface Project {
  /** Stable unique id. */
  id: string;
  /** Trimmed, validated display name. */
  name: string;
  /** ISO timestamp of creation (display only). */
  createdAt: string;
}

/** Reasons a project name can be rejected. */
export type ProjectNameError = "empty" | "too_long" | "duplicate";

/** Maximum project name length, counted per the D-012 code-point policy. */
export const MAX_PROJECT_NAME_LENGTH = 80;

/** Full in-memory workspace state. */
export interface WorkspaceState {
  projects: readonly Project[];
  /** Id of the currently selected project, or null when none is selected. */
  selectedProjectId: string | null;
}

/** Result of an attempted project creation. */
export type CreateProjectResult =
  | { ok: true; project: Project }
  | { ok: false; error: ProjectNameError };

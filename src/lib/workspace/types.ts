/**
 * Workspace domain model — persisted by the Rust core (SQLite).
 *
 * The store keeps the last valid snapshot from the backend and never
 * falls back to in-memory-only operation: if persistence is unavailable,
 * the state carries the error and the UI offers a retry.
 */
import type { CommandErrorPayload } from "../../types/app";
import type { Project, WorkspaceSnapshot } from "../../types/workspace";

export type { Project, WorkspaceSnapshot };

/** Reasons a project name can be rejected by client-side validation. */
export type ProjectNameError = "empty" | "too_long" | "duplicate";

/** Maximum project name length, counted per the D-012 code-point policy. */
export const MAX_PROJECT_NAME_LENGTH = 80;

/** The last valid snapshot known from the backend. */
export interface WorkspaceData {
  projects: Project[];
  selectedProjectId: string | null;
}

/** Overall persistence phase. `error` means no valid snapshot exists yet. */
export type WorkspacePhase = "loading" | "ready" | "error";

/**
 * The single operation currently in flight (all workspace operations run
 * through one serial queue — see store.ts). Drives busy UI states.
 */
export type PendingAction =
  | { kind: "load" }
  | { kind: "create" }
  | { kind: "select"; id: string }
  | { kind: "delete"; id: string };

/** Full observable workspace state. */
export interface WorkspaceState {
  phase: WorkspacePhase;
  data: WorkspaceData;
  /** Load failure when no valid snapshot exists yet (drives the retry UI). */
  loadError: CommandErrorPayload | null;
  /** Failure of the latest operation (shown near the controls). */
  actionError: CommandErrorPayload | null;
  /** The queued or in-flight operation, or null when idle. */
  pendingAction: PendingAction | null;
}

/** Result of client-side validation for a new project name. */
export type CreateProjectResult =
  | { ok: true }
  | { ok: false; error: ProjectNameError };

/**
 * Client-side validation for instant feedback. The Rust core re-validates
 * authoritatively (including duplicates at the database level).
 */
export function validateProjectName(
  rawName: string,
  existingProjects: readonly Project[],
): ProjectNameError | null {
  const name = rawName.trim();
  const length = [...name].length;
  if (length === 0) {
    return "empty";
  }
  if (length > MAX_PROJECT_NAME_LENGTH) {
    return "too_long";
  }
  if (existingProjects.some((p) => p.name === name)) {
    return "duplicate";
  }
  return null;
}

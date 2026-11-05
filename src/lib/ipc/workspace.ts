/**
 * Typed IPC bridge for persistent workspace operations.
 *
 * Every mutating command returns the fresh backend snapshot so the stores
 * converge on Rust state. Validation happens authoritatively in Rust;
 * the stores add client-side validation for instant feedback only.
 */
import { invoke } from "@tauri-apps/api/core";
import type { WorkspaceSnapshot } from "../../types/workspace";

/** Loads the full workspace snapshot (projects + selection). */
export async function fetchWorkspaceSnapshot(): Promise<WorkspaceSnapshot> {
  return invoke<WorkspaceSnapshot>("load_workspace");
}

/** Creates a project; resolves with the fresh snapshot. */
export async function createProjectRequest(name: string): Promise<WorkspaceSnapshot> {
  return invoke<WorkspaceSnapshot>("create_project", { name });
}

/** Selects a project; resolves with the fresh snapshot. */
export async function selectProjectRequest(id: string): Promise<WorkspaceSnapshot> {
  return invoke<WorkspaceSnapshot>("select_project", { id });
}

/** Deletes a project; resolves with the fresh snapshot. */
export async function deleteProjectRequest(id: string): Promise<WorkspaceSnapshot> {
  return invoke<WorkspaceSnapshot>("delete_project", { id });
}

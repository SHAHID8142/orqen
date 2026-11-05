/**
 * In-memory workspace store.
 *
 * Framework-agnostic observable store (subscribe/getState) so the domain
 * logic is unit-testable without React. Names are validated with the
 * shared code-point length policy (docs/DECISIONS.md D-012).
 */
import type {
  CreateProjectResult,
  Project,
  ProjectNameError,
  WorkspaceState,
} from "./types";
import { MAX_PROJECT_NAME_LENGTH } from "./types";

/** Counts Unicode scalar values (code points), per D-012. */
function codePointLength(value: string): number {
  return [...value].length;
}

/**
 * Validates a project name against the workspace rules: non-empty after
 * trimming, at most `MAX_PROJECT_NAME_LENGTH` code points, unique.
 */
export function validateProjectName(
  rawName: string,
  existingProjects: readonly Project[],
): ProjectNameError | null {
  const name = rawName.trim();
  if (codePointLength(name) === 0) {
    return "empty";
  }
  if (codePointLength(name) > MAX_PROJECT_NAME_LENGTH) {
    return "too_long";
  }
  if (existingProjects.some((p) => p.name === name)) {
    return "duplicate";
  }
  return null;
}

export interface WorkspaceStore {
  getState: () => WorkspaceState;
  subscribe: (listener: () => void) => () => void;
  /** Creates and selects a project; returns a validation error on failure. */
  createProject: (rawName: string) => CreateProjectResult;
  /** Selects an existing project by id. Unknown ids are ignored. */
  selectProject: (id: string) => void;
  /** Deletes a project; clears the selection if the selected one is removed. */
  deleteProject: (id: string) => void;
}

/** Creates a fresh, empty workspace store. */
export function createWorkspaceStore(): WorkspaceStore {
  let state: WorkspaceState = { projects: [], selectedProjectId: null };
  const listeners = new Set<() => void>();

  function setState(next: WorkspaceState) {
    state = next;
    for (const listener of listeners) listener();
  }

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    createProject(rawName) {
      const error = validateProjectName(rawName, state.projects);
      if (error) return { ok: false, error };
      const project: Project = {
        id: crypto.randomUUID(),
        name: rawName.trim(),
        createdAt: new Date().toISOString(),
      };
      setState({
        projects: [...state.projects, project],
        selectedProjectId: project.id,
      });
      return { ok: true, project };
    },
    selectProject(id) {
      if (!state.projects.some((p) => p.id === id)) return;
      if (state.selectedProjectId === id) return;
      setState({ ...state, selectedProjectId: id });
    },
    deleteProject(id) {
      const remaining = state.projects.filter((p) => p.id !== id);
      if (remaining.length === state.projects.length) return;
      setState({
        projects: remaining,
        selectedProjectId:
          state.selectedProjectId === id ? null : state.selectedProjectId,
      });
    },
  };
}

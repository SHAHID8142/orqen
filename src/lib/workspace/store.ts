/**
 * Persistent workspace store.
 *
 * Framework-agnostic observable store bound to React via
 * `useSyncExternalStore`. All data lives in the Rust core (SQLite); the
 * store holds the last valid backend snapshot and applies every fresh
 * snapshot returned by a mutation, so UI and database never diverge.
 * IPC functions are injectable for tests.
 */
import type { CommandErrorPayload } from "../../types/app";
import type { Project, WorkspaceSnapshot } from "./types";
import { validateProjectName, type WorkspaceState } from "./types";

/** IPC surface of the store (matches `lib/ipc/workspace.ts`). */
export interface WorkspaceIpc {
  load: () => Promise<WorkspaceSnapshot>;
  create: (name: string) => Promise<WorkspaceSnapshot>;
  select: (id: string) => Promise<WorkspaceSnapshot>;
  remove: (id: string) => Promise<WorkspaceSnapshot>;
}

export interface WorkspaceStore {
  getState: () => WorkspaceState;
  subscribe: (listener: () => void) => () => void;
  /** Loads the snapshot; also the retry path after a failed load. */
  load: () => Promise<void>;
  /** Creates a project (client-validated) and applies the snapshot. */
  create: (rawName: string) => Promise<void>;
  /** Selects a project and applies the snapshot. */
  select: (id: string) => Promise<void>;
  /** Deletes a project and applies the snapshot. */
  remove: (id: string) => Promise<void>;
}

/** Creates a store bound to the given IPC surface. */
export function createWorkspaceStore(ipc: WorkspaceIpc): WorkspaceStore {
  let state: WorkspaceState = {
    phase: "loading",
    data: { projects: [], selectedProjectId: null },
    loadError: null,
    actionError: null,
    creating: false,
    selectingId: null,
    deletingId: null,
  };
  const listeners = new Set<() => void>();
  let loadInFlight = false;

  function setState(next: Partial<WorkspaceState>) {
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  }

  function applySnapshot(snapshot: WorkspaceSnapshot) {
    setState({
      phase: "ready",
      data: {
        projects: snapshot.projects,
        selectedProjectId: snapshot.selectedProjectId,
      },
    });
  }

  async function load(): Promise<void> {
    if (loadInFlight) return; // prevent duplicate loads
    loadInFlight = true;
    const hadData = state.phase === "ready";
    setState({ phase: "loading", loadError: null });
    try {
      const snapshot = await ipc.load();
      applySnapshot(snapshot);
      setState({ loadError: null });
    } catch (error) {
      const loadError = normalizeError(error);
      // A failed reload keeps the previous valid state; only the first
      // load failure puts the store into the error phase.
      if (hadData) {
        setState({ phase: "ready", loadError });
      } else {
        setState({
          phase: "error",
          loadError,
          data: { projects: [], selectedProjectId: null },
        });
      }
    } finally {
      loadInFlight = false;
    }
  }

  async function create(rawName: string): Promise<void> {
    if (state.creating || state.phase !== "ready") return;
    const validationError = validateProjectName(rawName, state.data.projects);
    if (validationError) {
      setState({ actionError: validationMessage(validationError) });
      return;
    }
    const name = rawName.trim(); // match the Rust-side normalization
    setState({ creating: true, actionError: null });
    try {
      const snapshot = await ipc.create(name);
      setState({ creating: false });
      applySnapshot(snapshot);
    } catch (error) {
      setState({ creating: false, actionError: normalizeError(error) });
    }
  }

  async function select(id: string): Promise<void> {
    if (state.selectingId !== null || state.phase !== "ready") return;
    if (state.data.selectedProjectId === id) return; // no-op selection
    setState({ selectingId: id, actionError: null });
    try {
      const snapshot = await ipc.select(id);
      setState({ selectingId: null });
      applySnapshot(snapshot);
    } catch (error) {
      setState({ selectingId: null, actionError: normalizeError(error) });
    }
  }

  async function remove(id: string): Promise<void> {
    if (state.deletingId !== null || state.phase !== "ready") return;
    setState({ deletingId: id, actionError: null });
    try {
      const snapshot = await ipc.remove(id);
      setState({ deletingId: null });
      applySnapshot(snapshot);
    } catch (error) {
      setState({ deletingId: null, actionError: normalizeError(error) });
    }
  }

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    load,
    create,
    select,
    remove,
  };
}

/** Client-side validation messages (instant feedback; Rust re-validates). */
function validationMessage(error: ReturnType<typeof validateProjectName>): CommandErrorPayload {
  switch (error) {
    case "empty":
      return { kind: "invalid_input", message: "Project name must not be empty." };
    case "too_long":
      return {
        kind: "invalid_input",
        message: "Project name must not exceed 80 characters.",
      };
    case "duplicate":
      return { kind: "invalid_input", message: "A project with this name already exists." };
    default:
      return { kind: "internal", message: "Invalid project name." };
  }
}

function normalizeError(error: unknown): CommandErrorPayload {
  if (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error &&
    typeof (error as { kind: unknown }).kind === "string" &&
    typeof (error as { message: unknown }).message === "string"
  ) {
    const payload = error as { kind: string; message: string };
    return { kind: payload.kind as CommandErrorPayload["kind"], message: payload.message };
  }
  return {
    kind: "internal",
    message: error instanceof Error ? error.message : String(error),
  };
}

export type { Project };

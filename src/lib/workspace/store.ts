/**
 * Persistent workspace store.
 *
 * Framework-agnostic observable store bound to React via
 * `useSyncExternalStore`. All data lives in the Rust core (SQLite); the
 * store holds the last valid backend snapshot and applies every fresh
 * snapshot returned by an operation, so UI and database never diverge.
 * IPC functions are injectable for tests.
 *
 * Concurrency strategy (docs/ARCHITECTURE.md §Persistence): every
 * operation — load, create, select, delete — runs through one serial
 * queue. At most one request is in flight, responses apply strictly in
 * issue order, and no snapshot can be overwritten by an older response.
 * Requests issued while one is in flight are queued (not discarded) and
 * the UI disables the controls meanwhile; the only pre-queue rejections
 * are client-side validation failures and mutations attempted while the
 * persisted state is unavailable, both surfaced as `actionError`.
 */
import type { CommandErrorPayload } from "../../types/app";
import type { WorkspaceSnapshot } from "./types";
import { validateProjectName, type WorkspaceState } from "./types";
import { createSerialQueue, type SerialQueue } from "../serialQueue";

/** IPC surface of the store (matches `lib/ipc/workspace.ts`). */
export interface WorkspaceIpc {
  load: () => Promise<WorkspaceSnapshot>;
  create: (name: string) => Promise<WorkspaceSnapshot>;
  select: (id: string) => Promise<WorkspaceSnapshot>;
  remove: (id: string) => Promise<WorkspaceSnapshot>;
}

/** The operation currently in flight (drives busy UI states). */
export type PendingAction =
  | { kind: "load" }
  | { kind: "create" }
  | { kind: "select"; id: string }
  | { kind: "delete"; id: string };

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
  const queue: SerialQueue = createSerialQueue();
  let state: WorkspaceState = {
    phase: "loading",
    data: { projects: [], selectedProjectId: null },
    loadError: null,
    actionError: null,
    pendingAction: { kind: "load" },
  };
  const listeners = new Set<() => void>();

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
    return queue.run(async () => {
      const hadData = state.phase === "ready";
      setState({ phase: "loading", loadError: null, pendingAction: { kind: "load" } });
      try {
        const snapshot = await ipc.load();
        applySnapshot(snapshot);
        setState({ loadError: null, pendingAction: null });
      } catch (error) {
        const loadError = normalizeError(error);
        // A failed reload keeps the previous valid state; only the first
        // load failure puts the store into the error phase.
        if (hadData) {
          setState({ phase: "ready", loadError, pendingAction: null });
        } else {
          setState({
            phase: "error",
            loadError,
            pendingAction: null,
            data: { projects: [], selectedProjectId: null },
          });
        }
      }
    });
  }

  async function create(rawName: string): Promise<void> {
    return queue.run(async () => {
      // Mutations require the persisted snapshot; without it there is
      // nothing safe to mutate against.
      if (state.phase !== "ready") {
        setState({
          actionError: {
            kind: "storage",
            message: "Projects are unavailable right now — retry loading first.",
          },
        });
        return;
      }
      const validationError = validateProjectName(rawName, state.data.projects);
      if (validationError) {
        setState({ actionError: validationMessage(validationError) });
        return;
      }
      const name = rawName.trim(); // match the Rust-side normalization
      setState({ pendingAction: { kind: "create" }, actionError: null });
      try {
        const snapshot = await ipc.create(name);
        applySnapshot(snapshot);
        setState({ pendingAction: null });
      } catch (error) {
        setState({ pendingAction: null, actionError: normalizeError(error) });
      }
    });
  }

  async function select(id: string): Promise<void> {
    return queue.run(async () => {
      if (state.phase !== "ready") {
        setState({
          actionError: {
            kind: "storage",
            message: "Projects are unavailable right now — retry loading first.",
          },
        });
        return;
      }
      if (state.data.selectedProjectId === id) return; // no-op selection
      setState({ pendingAction: { kind: "select", id }, actionError: null });
      try {
        const snapshot = await ipc.select(id);
        applySnapshot(snapshot);
        setState({ pendingAction: null });
      } catch (error) {
        setState({ pendingAction: null, actionError: normalizeError(error) });
      }
    });
  }

  async function remove(id: string): Promise<void> {
    return queue.run(async () => {
      if (state.phase !== "ready") {
        setState({
          actionError: {
            kind: "storage",
            message: "Projects are unavailable right now — retry loading first.",
          },
        });
        return;
      }
      if (!state.data.projects.some((p) => p.id === id)) return; // unknown id: nothing to delete
      setState({ pendingAction: { kind: "delete", id }, actionError: null });
      try {
        const snapshot = await ipc.remove(id);
        applySnapshot(snapshot);
        setState({ pendingAction: null });
      } catch (error) {
        setState({ pendingAction: null, actionError: normalizeError(error) });
      }
    });
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

/**
 * Agent registry store.
 *
 * Same observable/serial-queue pattern as the workspace and settings
 * stores. Holds the registry records and per-agent discovery/verification
 * results. The Main Agent preference itself lives in the settings store
 * (it is persisted there); this store only drives discovery and
 * verification. IPC functions are injectable for tests.
 */
import type { CommandErrorPayload } from "../../types/app";
import type { AgentRecord } from "../../types/agents";
import { createSerialQueue, type SerialQueue } from "../serialQueue";

/** IPC surface of the store (matches `lib/ipc/agents.ts`). */
export interface AgentsIpc {
  load: () => Promise<AgentRecord[]>;
  discover: (id: string) => Promise<AgentRecord>;
  verify: (id: string) => Promise<AgentRecord>;
}

/** The single registry operation currently in flight. */
export type AgentsPendingAction =
  | { kind: "load" }
  | { kind: "discover"; id: string }
  | { kind: "verify"; id: string };

export interface AgentsState {
  phase: "loading" | "ready" | "error";
  records: AgentRecord[];
  loadError: CommandErrorPayload | null;
  actionError: CommandErrorPayload | null;
  pendingAction: AgentsPendingAction | null;
}

export interface AgentsStore {
  getState: () => AgentsState;
  subscribe: (listener: () => void) => () => void;
  /** Loads the registry; also the retry path after a failed load. */
  load: () => Promise<void>;
  /** Locates the agent's executable (never executes it). */
  discover: (id: string) => Promise<void>;
  /** Explicit `--version` verification of a discovered agent. */
  verify: (id: string) => Promise<void>;
}

export function createAgentsStore(ipc: AgentsIpc): AgentsStore {
  const queue: SerialQueue = createSerialQueue();
  let state: AgentsState = {
    phase: "loading",
    records: [],
    loadError: null,
    actionError: null,
    pendingAction: { kind: "load" },
  };
  const listeners = new Set<() => void>();

  function setState(next: Partial<AgentsState>) {
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  }

  function replaceRecord(record: AgentRecord) {
    setState({
      records: state.records.map((r) => (r.id === record.id ? record : r)),
    });
  }

  async function load(): Promise<void> {
    return queue.run(async () => {
      const hadData = state.phase === "ready";
      setState({ phase: "loading", loadError: null, pendingAction: { kind: "load" } });
      try {
        const records = await ipc.load();
        setState({ phase: "ready", records, loadError: null, pendingAction: null });
      } catch (error) {
        const loadError = normalizeError(error);
        if (hadData) {
          setState({ phase: "ready", loadError, pendingAction: null });
        } else {
          setState({ phase: "error", loadError, pendingAction: null, records: [] });
        }
      }
    });
  }

  async function discover(id: string): Promise<void> {
    return queue.run(async () => {
      if (state.phase !== "ready") {
        setState({
          actionError: {
            kind: "storage",
            message: "Agents are unavailable right now — retry loading first.",
          },
        });
        return;
      }
      if (!state.records.some((r) => r.id === id)) {
        setState({
          actionError: { kind: "invalid_input", message: `Unknown agent "${id}".` },
        });
        return;
      }
      setState({ pendingAction: { kind: "discover", id }, actionError: null });
      try {
        replaceRecord(await ipc.discover(id));
        setState({ pendingAction: null });
      } catch (error) {
        setState({ pendingAction: null, actionError: normalizeError(error) });
      }
    });
  }

  async function verify(id: string): Promise<void> {
    return queue.run(async () => {
      if (state.phase !== "ready") {
        setState({
          actionError: {
            kind: "storage",
            message: "Agents are unavailable right now — retry loading first.",
          },
        });
        return;
      }
      if (!state.records.some((r) => r.id === id)) {
        setState({
          actionError: { kind: "invalid_input", message: `Unknown agent "${id}".` },
        });
        return;
      }
      setState({ pendingAction: { kind: "verify", id }, actionError: null });
      try {
        replaceRecord(await ipc.verify(id));
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
    discover,
    verify,
  };
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

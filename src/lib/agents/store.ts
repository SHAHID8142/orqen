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
import type {
  AgentRecord,
  RunOutcomeDto,
  RunStatusDto,
} from "../../types/agents";
import { createSerialQueue, type SerialQueue } from "../serialQueue";

/** IPC surface of the store (matches `lib/ipc/agents.ts`). */
export interface AgentsIpc {
  load: () => Promise<AgentRecord[]>;
  discover: (id: string) => Promise<AgentRecord>;
  verify: (id: string) => Promise<AgentRecord>;
  startRun: (agentId: string) => Promise<string>;
  runStatus: (runId: string) => Promise<RunStatusDto>;
  cancelRun: (runId: string) => Promise<void>;
}

/** The single registry operation currently in flight. */
export type AgentsPendingAction =
  | { kind: "load" }
  | { kind: "discover"; id: string }
  | { kind: "verify"; id: string };

/** Diagnostic test-run state machine (see docs/ARCHITECTURE.md). */
export type RunPhase =
  | "idle"
  | "awaiting_confirmation"
  | "starting"
  | "running"
  | "succeeded"
  | "failed"
  | "timed_out"
  | "cancelled";

export interface DiagnosticRunState {
  phase: RunPhase;
  runId: string | null;
  agentId: string | null;
  result: RunOutcomeDto | null;
  error: CommandErrorPayload | null;
  /** True when the user asked to cancel while the Start IPC was still
   * pending (no run id existed yet). The intent is honored the moment
   * the run id arrives — it is never silently ignored. */
  cancelRequested: boolean;
}

const IDLE_RUN: DiagnosticRunState = {
  phase: "idle",
  runId: null,
  agentId: null,
  result: null,
  error: null,
  cancelRequested: false,
};


export interface AgentsState {
  phase: "loading" | "ready" | "error";
  records: AgentRecord[];
  loadError: CommandErrorPayload | null;
  actionError: CommandErrorPayload | null;
  pendingAction: AgentsPendingAction | null;
  run: DiagnosticRunState;
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
  /** Opens the confirmation step for a diagnostic test run. */
  requestTestRun: (agentId: string) => void;
  /** Closes the confirmation step without running anything. */
  cancelRequest: () => void;
  /** Confirms and executes the diagnostic run; polls until terminal. */
  confirmTestRun: () => Promise<void>;
  /** Cancels a running diagnostic. */
  cancelRun: () => Promise<void>;
  /** Returns a terminal run to the idle state. */
  dismissRun: () => void;
}

export function createAgentsStore(ipc: AgentsIpc): AgentsStore {
  const queue: SerialQueue = createSerialQueue();
  let state: AgentsState = {
    phase: "loading",
    records: [],
    loadError: null,
    actionError: null,
    pendingAction: { kind: "load" },
    run: { ...IDLE_RUN, phase: "idle" },
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

  // ---- Diagnostic test-run state machine ----
  // Long-running (model latency): NOT on the serial queue — a run must
  // not block discovery/verification. Duplicate submissions are
  // prevented by the run phase itself (only non-idle/terminal phases
  // may start, and the UI disables the controls).

  function requestTestRun(agentId: string) {
    if (state.run.phase !== "idle") return;
    setState({ run: { ...IDLE_RUN, phase: "awaiting_confirmation", agentId } });
  }

  function cancelRequest() {
    if (state.run.phase !== "awaiting_confirmation") return;
    setState({ run: { ...IDLE_RUN } });
  }

  function dismissRun() {
    if (
      state.run.phase === "running" ||
      state.run.phase === "starting" ||
      state.run.phase === "awaiting_confirmation"
    ) {
      return;
    }
    setState({ run: { ...IDLE_RUN } });
  }

  async function confirmTestRun(): Promise<void> {
    const agentId = state.run.phase === "awaiting_confirmation" ? state.run.agentId : null;
    if (agentId === null || state.run.phase === "starting" || state.run.phase === "running") {
      return;
    }
    setState({ run: { ...IDLE_RUN, phase: "starting", agentId } });
    let runId: string;
    try {
      runId = await ipc.startRun(agentId);
    } catch (error) {
      // The run never started, so a pending cancel intent is moot — the
      // truthful terminal state is the start failure itself.
      setState({
        run: {
          ...IDLE_RUN,
          phase: "failed",
          agentId,
          error: normalizeError(error),
        },
      });
      return;
    }
    // A cancel requested while the Start IPC was pending is honored now:
    // the run id exists, so the backend can actually terminate the run.
    const cancelWasRequested = state.run.cancelRequested;
    setState({
      run: { ...IDLE_RUN, phase: "running", runId, agentId, cancelRequested: cancelWasRequested },
    });
    if (cancelWasRequested) {
      try {
        await ipc.cancelRun(runId);
      } catch (error) {
        setState({
          run: {
            phase: "running",
            runId,
            agentId,
            result: null,
            error: normalizeError(error),
            cancelRequested: cancelWasRequested,
          },
        });
      }
    }
    // Poll until terminal (bounded by the backend's own 120s deadline).
    for (;;) {
      await new Promise((resolve) => setTimeout(resolve, 700));
      let status: RunStatusDto;
      try {
        status = await ipc.runStatus(runId);
      } catch (error) {
        setState({
          run: {
            phase: "failed",
            runId,
            agentId,
            result: null,
            error: normalizeError(error),
            cancelRequested: state.run.cancelRequested,
          },
        });
        return;
      }
      if (status.phase === "finished") {
        setState({
          run: {
            phase: status.result.outcome,
            runId,
            agentId,
            result: status.result,
            error: null,
            cancelRequested: state.run.cancelRequested,
          },
        });
        return;
      }
      if (status.phase === "unknown") {
        setState({
          run: {
            phase: "failed",
            runId,
            agentId,
            result: null,
            error: { kind: "internal", message: "the run is no longer tracked (session ended)." },
            cancelRequested: state.run.cancelRequested,
          },
        });
        return;
      }
      // still starting/running — keep polling
    }
  }

  async function cancelRun(): Promise<void> {
    // Starting phase: the Start IPC is still pending, so no run id (and
    // no backend handle) exists yet. Record the intent — confirmTestRun
    // cancels the run the moment the id arrives. Never silently ignored.
    if (state.run.phase === "starting" && state.run.runId === null) {
      if (!state.run.cancelRequested) {
        setState({ run: { ...state.run, cancelRequested: true } });
      }
      return;
    }
    const runId = state.run.runId;
    if (runId === null || (state.run.phase !== "running" && state.run.phase !== "starting")) {
      return;
    }
    try {
      await ipc.cancelRun(runId);
    } catch (error) {
      setState({ run: { ...state.run, error: normalizeError(error) } });
    }
    // The poll loop transitions to cancelled when the backend reports it.
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
    requestTestRun,
    cancelRequest,
    confirmTestRun,
    cancelRun,
    dismissRun,
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

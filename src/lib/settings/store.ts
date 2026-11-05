/**
 * Persistent settings store (same observable pattern as the workspace
 * store). IPC functions are injectable for tests.
 */
import type { CommandErrorPayload } from "../../types/app";
import type { Settings, SettingsInput, SettingsState } from "./types";
import { MAX_RETRY_LIMIT, MIN_RETRY } from "./types";

/** IPC surface of the store (matches `lib/ipc/settings.ts`). */
export interface SettingsIpc {
  load: () => Promise<Settings>;
  update: (input: SettingsInput) => Promise<Settings>;
}

export interface SettingsStore {
  getState: () => SettingsState;
  subscribe: (listener: () => void) => () => void;
  /** Loads settings; also the retry path after a failed load. */
  load: () => Promise<void>;
  /** Updates settings and applies the stored result. */
  update: (input: SettingsInput) => Promise<void>;
}

/** Creates a store bound to the given IPC surface. */
export function createSettingsStore(ipc: SettingsIpc): SettingsStore {
  let state: SettingsState = {
    phase: "loading",
    data: { approvalMode: "manual", maxRetry: 3, telegramNotifications: true },
    loadError: null,
    actionError: null,
    saving: false,
  };
  const listeners = new Set<() => void>();
  let loadInFlight = false;

  function setState(next: Partial<SettingsState>) {
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  }

  async function load(): Promise<void> {
    if (loadInFlight) return;
    loadInFlight = true;
    const hadData = state.phase === "ready";
    setState({ phase: "loading", loadError: null });
    try {
      const settings = await ipc.load();
      setState({ phase: "ready", data: settings, loadError: null });
    } catch (error) {
      const loadError = normalizeError(error);
      if (hadData) {
        setState({ phase: "ready", loadError });
      } else {
        setState({ phase: "error", loadError });
      }
    } finally {
      loadInFlight = false;
    }
  }

  async function update(input: SettingsInput): Promise<void> {
    if (state.saving || state.phase !== "ready") return;
    if (input.approvalMode !== "manual" && input.approvalMode !== "autonomous") {
      setState({
        actionError: { kind: "invalid_input", message: "Approval mode must be manual or autonomous." },
      });
      return;
    }
    if (
      !Number.isInteger(input.maxRetry) ||
      input.maxRetry < MIN_RETRY ||
      input.maxRetry > MAX_RETRY_LIMIT
    ) {
      setState({
        actionError: {
          kind: "invalid_input",
          message: `Maximum retry must be a whole number between ${MIN_RETRY} and ${MAX_RETRY_LIMIT}.`,
        },
      });
      return;
    }
    setState({ saving: true, actionError: null });
    try {
      const stored = await ipc.update(input);
      setState({ saving: false, data: stored });
    } catch (error) {
      setState({ saving: false, actionError: normalizeError(error) });
    }
  }

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    load,
    update,
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

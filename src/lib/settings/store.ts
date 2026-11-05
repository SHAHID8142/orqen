/**
 * In-memory settings store (same observable pattern as the workspace store).
 */
import type { ApprovalMode, SettingsState } from "./types";
import { DEFAULT_SETTINGS, MAX_RETRY_LIMIT, MIN_RETRY } from "./types";

export interface SettingsStore {
  getState: () => SettingsState;
  subscribe: (listener: () => void) => () => void;
  setApprovalMode: (mode: ApprovalMode) => void;
  /** Sets the retry preference, clamped into 1..MAX_RETRY_LIMIT. */
  setMaxRetry: (value: number) => void;
  setTelegramNotifications: (enabled: boolean) => void;
  /** Restores the governance defaults. */
  resetToDefaults: () => void;
}

/** Creates a settings store initialized with the governance defaults. */
export function createSettingsStore(
  initial: SettingsState = DEFAULT_SETTINGS,
): SettingsStore {
  let state: SettingsState = initial;
  const listeners = new Set<() => void>();

  function setState(next: SettingsState) {
    state = next;
    for (const listener of listeners) listener();
  }

  function clampRetry(value: number): number {
    if (!Number.isFinite(value)) return state.maxRetry;
    return Math.min(MAX_RETRY_LIMIT, Math.max(MIN_RETRY, Math.round(value)));
  }

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setApprovalMode(mode) {
      if (state.approvalMode === mode) return;
      setState({ ...state, approvalMode: mode });
    },
    setMaxRetry(value) {
      const clamped = clampRetry(value);
      if (clamped === state.maxRetry) return;
      setState({ ...state, maxRetry: clamped });
    },
    setTelegramNotifications(enabled) {
      if (state.telegramNotifications === enabled) return;
      setState({ ...state, telegramNotifications: enabled });
    },
    resetToDefaults() {
      setState(DEFAULT_SETTINGS);
    },
  };
}

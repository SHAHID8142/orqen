/**
 * Unit tests for the in-memory settings store.
 */
import { describe, expect, it, vi } from "vitest";
import { createSettingsStore } from "./store";
import { DEFAULT_SETTINGS, MAX_RETRY_LIMIT, MIN_RETRY } from "./types";

describe("createSettingsStore", () => {
  it("starts with the governance defaults", () => {
    const store = createSettingsStore();
    expect(store.getState()).toEqual({
      approvalMode: "manual",
      maxRetry: 3,
      telegramNotifications: true,
    });
  });

  it("changes the approval mode", () => {
    const store = createSettingsStore();
    store.setApprovalMode("autonomous");
    expect(store.getState().approvalMode).toBe("autonomous");
  });

  it("clamps the retry preference into the allowed range", () => {
    const store = createSettingsStore();
    store.setMaxRetry(0);
    expect(store.getState().maxRetry).toBe(MIN_RETRY);
    store.setMaxRetry(999);
    expect(store.getState().maxRetry).toBe(MAX_RETRY_LIMIT);
    store.setMaxRetry(7.4);
    expect(store.getState().maxRetry).toBe(7);
    store.setMaxRetry(Number.NaN);
    expect(store.getState().maxRetry).toBe(7); // unchanged on non-finite input
  });

  it("toggles telegram notifications and resets to defaults", () => {
    const store = createSettingsStore();
    store.setTelegramNotifications(false);
    store.setApprovalMode("autonomous");
    store.setMaxRetry(9);
    store.resetToDefaults();
    expect(store.getState()).toEqual(DEFAULT_SETTINGS);
  });

  it("notifies subscribers on changes; unsubscribed listeners stop firing", () => {
    const store = createSettingsStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);
    store.setApprovalMode("autonomous");
    expect(listener).toHaveBeenCalled();

    listener.mockClear();
    unsubscribe();
    store.setMaxRetry(5);
    expect(listener).not.toHaveBeenCalled();
  });

  it("does not notify when a value is set to its current value", () => {
    const store = createSettingsStore();
    const listener = vi.fn();
    store.subscribe(listener);
    store.setApprovalMode("manual");
    store.setMaxRetry(3);
    expect(listener).not.toHaveBeenCalled();
  });
});

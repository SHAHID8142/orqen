/**
 * Unit tests for the persistent settings store against fake IPC surfaces.
 */
import { describe, expect, it, vi } from "vitest";
import { createSettingsStore, type SettingsIpc } from "./store";
import { MAX_RETRY_LIMIT, MIN_RETRY } from "./types";
import type { Settings, SettingsInput } from "../../types/workspace";

const DEFAULTS: Settings = {
  approvalMode: "manual",
  maxRetry: 3,
  telegramNotifications: true,
};

function makeIpc(overrides: Partial<SettingsIpc> = {}): SettingsIpc & {
  load: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
} {
  return {
    load: vi.fn().mockResolvedValue(DEFAULTS),
    update: vi.fn(),
    ...overrides,
  } as never;
}

describe("settings store — loading", () => {
  it("starts loading and applies the persisted settings", async () => {
    const ipc = makeIpc();
    const store = createSettingsStore(ipc);
    expect(store.getState().phase).toBe("loading");

    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data).toEqual(DEFAULTS);
  });

  it("enters the error phase on first-load failure and recovers on retry", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const store = createSettingsStore(ipc);
    await store.load();
    expect(store.getState().phase).toBe("error");

    await store.load(); // retry succeeds
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data).toEqual(DEFAULTS);
  });

  it("keeps previous valid settings when a reload fails after being ready", async () => {
    const changed: Settings = { approvalMode: "autonomous", maxRetry: 7, telegramNotifications: false };
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(changed) });
    const store = createSettingsStore(ipc);
    await store.load();

    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "later failure" });
    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data).toEqual(changed);
    expect(store.getState().loadError?.kind).toBe("storage");
  });
});

describe("settings store — updates", () => {
  it("applies the stored result after a successful update", async () => {
    const stored: Settings = { approvalMode: "autonomous", maxRetry: 7, telegramNotifications: false };
    const ipc = makeIpc();
    ipc.update.mockResolvedValue(stored);
    const store = createSettingsStore(ipc);
    await store.load();

    await store.update(stored);
    expect(ipc.update).toHaveBeenCalledWith(stored);
    expect(store.getState().data).toEqual(stored);
    expect(store.getState().actionError).toBeNull();
  });

  it("rejects out-of-range values client-side without invoking the backend", async () => {
    const ipc = makeIpc();
    const store = createSettingsStore(ipc);
    await store.load();

    await store.update({ approvalMode: "manual", maxRetry: 0, telegramNotifications: true });
    await store.update({
      approvalMode: "manual",
      maxRetry: MAX_RETRY_LIMIT + 1,
      telegramNotifications: true,
    });
    // A malformed mode could only arrive from a compromised caller; the
    // store must still reject it before reaching the backend.
    await store.update({
      approvalMode: "yolo" as SettingsInput["approvalMode"],
      maxRetry: 3,
      telegramNotifications: true,
    });
    expect(ipc.update).not.toHaveBeenCalled();
    expect(store.getState().actionError?.kind).toBe("invalid_input");
    expect(store.getState().data).toEqual(DEFAULTS);
  });

  it("keeps previous valid settings when an update fails", async () => {
    const ipc = makeIpc();
    const store = createSettingsStore(ipc);
    await store.load();

    ipc.update.mockRejectedValue({ kind: "storage", message: "write failed" });
    await store.update({ approvalMode: "autonomous", maxRetry: 5, telegramNotifications: false });
    expect(store.getState().data).toEqual(DEFAULTS);
    expect(store.getState().actionError?.kind).toBe("storage");
  });

  it("ignores a second update while one is saving", async () => {
    let resolveUpdate!: (value: Settings) => void;
    const ipc = makeIpc();
    ipc.update.mockReturnValue(
      new Promise<Settings>((resolve) => {
        resolveUpdate = resolve;
      }),
    );
    const store = createSettingsStore(ipc);
    await store.load();

    const first = store.update({ approvalMode: "autonomous", maxRetry: 5, telegramNotifications: false });
    await store.update({ approvalMode: "manual", maxRetry: 3, telegramNotifications: true });
    resolveUpdate({ approvalMode: "autonomous", maxRetry: 5, telegramNotifications: false });
    await first;
    expect(ipc.update).toHaveBeenCalledTimes(1);
    expect(store.getState().saving).toBe(false);
  });

  it("does not issue duplicate loads while one is in flight", async () => {
    let resolveLoad!: (value: Settings) => void;
    const ipc = makeIpc();
    ipc.load.mockReturnValue(
      new Promise<Settings>((resolve) => {
        resolveLoad = resolve;
      }),
    );
    const store = createSettingsStore(ipc);
    const first = store.load();
    const second = store.load();
    resolveLoad(DEFAULTS);
    await Promise.all([first, second]);
    expect(ipc.load).toHaveBeenCalledTimes(1);
  });
});

describe("settings bounds", () => {
  it("keeps the documented range constants", () => {
    expect(MIN_RETRY).toBe(1);
    expect(MAX_RETRY_LIMIT).toBe(10);
  });
});

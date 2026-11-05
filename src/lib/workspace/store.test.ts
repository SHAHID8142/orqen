/**
 * Unit tests for the persistent workspace store against fake IPC surfaces
 * (deferred promises control timing; the store logic runs for real).
 */
import { describe, expect, it, vi } from "vitest";
import { createWorkspaceStore, type WorkspaceIpc } from "./store";
import { MAX_PROJECT_NAME_LENGTH } from "./types";
import type { WorkspaceSnapshot } from "../../types/workspace";

function snapshot(
  projects: Array<{ id: string; name: string }>,
  selectedProjectId: string | null = null,
): WorkspaceSnapshot {
  return {
    projects: projects.map((p) => ({ ...p, createdAt: "2026-01-01T00:00:00Z" })),
    selectedProjectId,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function makeIpc(overrides: Partial<WorkspaceIpc> = {}): WorkspaceIpc & {
  load: ReturnType<typeof vi.fn>;
  create: ReturnType<typeof vi.fn>;
  select: ReturnType<typeof vi.fn>;
  remove: ReturnType<typeof vi.fn>;
} {
  return {
    load: vi.fn().mockResolvedValue(snapshot([])),
    create: vi.fn(),
    select: vi.fn(),
    remove: vi.fn(),
    ...overrides,
  } as never;
}

describe("workspace store — loading", () => {
  it("starts in the loading phase with empty data", () => {
    const store = createWorkspaceStore(makeIpc());
    expect(store.getState().phase).toBe("loading");
    expect(store.getState().data).toEqual({ projects: [], selectedProjectId: null });
  });

  it("applies the backend snapshot on successful load", async () => {
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Demo" }])) });
    const store = createWorkspaceStore(ipc);
    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data.projects).toHaveLength(1);
    expect(store.getState().loadError).toBeNull();
  });

  it("enters the error phase on first-load failure and recovers on retry", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const store = createWorkspaceStore(ipc);

    await store.load();
    expect(store.getState().phase).toBe("error");
    expect(store.getState().loadError).toEqual({ kind: "storage", message: "db unavailable" });

    // Retry path: the same load action recovers.
    ipc.load.mockResolvedValueOnce(snapshot([{ id: "p1", name: "Recovered" }]));
    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data.projects[0]?.name).toBe("Recovered");
    expect(store.getState().loadError).toBeNull();
  });

  it("keeps the previous valid state when a reload fails after being ready", async () => {
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Kept" }])) });
    const store = createWorkspaceStore(ipc);
    await store.load();

    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "later failure" });
    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data.projects[0]?.name).toBe("Kept");
    expect(store.getState().loadError?.kind).toBe("storage");
  });

  it("does not issue duplicate loads while one is in flight", async () => {
    const pending = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc({ load: vi.fn().mockReturnValue(pending.promise) });
    const store = createWorkspaceStore(ipc);

    const first = store.load();
    const second = store.load();
    pending.resolve(snapshot([]));
    await Promise.all([first, second]);
    expect(ipc.load).toHaveBeenCalledTimes(1);
  });
});

describe("workspace store — create", () => {
  it("rejects invalid names client-side without invoking the backend", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Demo" }])),
    });
    const store = createWorkspaceStore(ipc);
    await store.load();

    await store.create("   ");
    await store.create("a".repeat(MAX_PROJECT_NAME_LENGTH + 1));
    await store.create("Demo"); // duplicate against loaded data
    expect(ipc.create).not.toHaveBeenCalled();
    expect(store.getState().actionError?.kind).toBe("invalid_input");
  });

  it("applies the backend snapshot after a successful create", async () => {
    const pending = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc();
    ipc.create.mockReturnValue(pending.promise);
    const store = createWorkspaceStore(ipc);
    await store.load();

    const run = store.create("Alpha");
    await vi.waitFor(() => expect(store.getState().creating).toBe(true));
    pending.resolve(snapshot([{ id: "p1", name: "Alpha" }], "p1"));
    await run;

    const state = store.getState();
    expect(state.creating).toBe(false);
    expect(state.data.projects[0]?.name).toBe("Alpha");
    expect(state.data.selectedProjectId).toBe("p1");
    expect(state.actionError).toBeNull();
  });

  it("keeps the last valid state and surfaces the error when a create fails", async () => {
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Kept" }])) });
    const store = createWorkspaceStore(ipc);
    await store.load();

    ipc.create.mockRejectedValue({ kind: "storage", message: "write failed" });
    await store.create("Broken");

    const state = store.getState();
    expect(state.creating).toBe(false);
    expect(state.actionError).toEqual({ kind: "storage", message: "write failed" });
    expect(state.data.projects[0]?.name).toBe("Kept");
  });

  it("ignores a second create while one is in flight", async () => {
    const pending = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc();
    ipc.create.mockReturnValue(pending.promise);
    const store = createWorkspaceStore(ipc);
    await store.load();

    const first = store.create("Alpha");
    await store.create("Beta"); // must be ignored
    pending.resolve(snapshot([{ id: "p1", name: "Alpha" }]));
    await first;
    expect(ipc.create).toHaveBeenCalledTimes(1);
  });
});

describe("workspace store — select and delete", () => {
  it("skips selecting the already-selected project without an IPC call", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "A" }], "p1")),
    });
    const store = createWorkspaceStore(ipc);
    await store.load();

    await store.select("p1");
    expect(ipc.select).not.toHaveBeenCalled();
  });

  it("keeps the previous selection when select fails", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "A" }], "p1")),
    });
    const store = createWorkspaceStore(ipc);
    await store.load();

    ipc.select.mockRejectedValue({ kind: "invalid_input", message: "Unknown project." });
    await store.select("ghost");
    expect(store.getState().data.selectedProjectId).toBe("p1");
    expect(store.getState().actionError?.message).toContain("Unknown project.");
  });

  it("applies the snapshot after delete (backend clears the selection)", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(
        snapshot(
          [
            { id: "p1", name: "A" },
            { id: "p2", name: "B" },
          ],
          "p1",
        ),
      ),
    });
    const store = createWorkspaceStore(ipc);
    await store.load();

    ipc.remove.mockResolvedValue(snapshot([{ id: "p2", name: "B" }]));
    await store.remove("p1");
    expect(store.getState().data.projects).toHaveLength(1);
    expect(store.getState().data.selectedProjectId).toBeNull();
  });

  it("keeps the last valid state when delete fails", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "A" }], "p1")),
    });
    const store = createWorkspaceStore(ipc);
    await store.load();

    ipc.remove.mockRejectedValue({ kind: "storage", message: "write failed" });
    await store.remove("p1");
    expect(store.getState().data.projects).toHaveLength(1);
    expect(store.getState().actionError?.kind).toBe("storage");
  });
});

describe("workspace store — guards", () => {
  it("ignores mutations while the store is not ready", async () => {
    const pending = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc({ load: vi.fn().mockReturnValue(pending.promise) });
    const store = createWorkspaceStore(ipc);

    await store.create("X"); // ignored: still loading
    await store.select("x");
    await store.remove("x");
    expect(ipc.create).not.toHaveBeenCalled();
    expect(ipc.select).not.toHaveBeenCalled();
    expect(ipc.remove).not.toHaveBeenCalled();
    pending.resolve(snapshot([]));
    await store.load();
  });

  it("notifies subscribers on every state change", async () => {
    const ipc = makeIpc();
    const store = createWorkspaceStore(ipc);
    const listener = vi.fn();
    store.subscribe(listener);
    await store.load();
    expect(listener).toHaveBeenCalled();
  });
});

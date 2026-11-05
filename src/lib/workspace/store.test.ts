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

/** Lets queued store operations start before mid-flight assertions. */
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

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
    expect(store.getState().pendingAction).toEqual({ kind: "load" });
  });

  it("applies the backend snapshot on successful load", async () => {
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Demo" }])) });
    const store = createWorkspaceStore(ipc);
    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().data.projects).toHaveLength(1);
    expect(store.getState().loadError).toBeNull();
    expect(store.getState().pendingAction).toBeNull();
  });

  it("enters the error phase on first-load failure and recovers on retry", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const store = createWorkspaceStore(ipc);

    await store.load();
    expect(store.getState().phase).toBe("error");
    expect(store.getState().loadError).toEqual({ kind: "storage", message: "db unavailable" });
    expect(store.getState().pendingAction).toBeNull();

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

  it("serializes loads instead of issuing them concurrently", async () => {
    const first = deferred<WorkspaceSnapshot>();
    const second = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc({ load: vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise) });
    const store = createWorkspaceStore(ipc);

    const run1 = store.load();
    const run2 = store.load();
    await flush();
    expect(ipc.load).toHaveBeenCalledTimes(1); // second is queued, not sent
    first.resolve(snapshot([{ id: "p1", name: "First" }]));
    await run1;
    expect(ipc.load).toHaveBeenCalledTimes(2); // queued load now runs
    second.resolve(snapshot([{ id: "p1", name: "First" }]));
    await run2;
    expect(store.getState().data.projects[0]?.name).toBe("First");
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
    await vi.waitFor(() =>
      expect(store.getState().pendingAction).toEqual({ kind: "create" }),
    );
    pending.resolve(snapshot([{ id: "p1", name: "Alpha" }], "p1"));
    await run;

    const state = store.getState();
    expect(state.pendingAction).toBeNull();
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
    expect(state.pendingAction).toBeNull();
    expect(state.actionError).toEqual({ kind: "storage", message: "write failed" });
    expect(state.data.projects[0]?.name).toBe("Kept");
  });

  it("queues a create issued while another is in flight (no silent discard)", async () => {
    const firstCreate = deferred<WorkspaceSnapshot>();
    const secondCreate = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc();
    ipc.create
      .mockReturnValueOnce(firstCreate.promise)
      .mockReturnValueOnce(secondCreate.promise);
    const store = createWorkspaceStore(ipc);
    await store.load();

    const run1 = store.create("Alpha");
    const run2 = store.create("Beta"); // queued, not discarded
    await flush();
    expect(ipc.create).toHaveBeenCalledTimes(1);

    firstCreate.resolve(snapshot([{ id: "p1", name: "Alpha" }], "p1"));
    await run1;
    expect(ipc.create).toHaveBeenCalledTimes(2); // queued create now runs
    secondCreate.resolve(
      snapshot(
        [
          { id: "p1", name: "Alpha" },
          { id: "p2", name: "Beta" },
        ],
        "p2",
      ),
    );
    await run2;
    // Final state converges on the LAST persisted snapshot.
    expect(store.getState().data.projects).toHaveLength(2);
    expect(store.getState().data.selectedProjectId).toBe("p2");
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

  it("ignores deletes for unknown ids without an IPC call", async () => {
    const ipc = makeIpc();
    const store = createWorkspaceStore(ipc);
    await store.load();
    await store.remove("ghost");
    expect(ipc.remove).not.toHaveBeenCalled();
  });
});

describe("workspace store — ordering and races", () => {
  it("applies snapshots strictly in issue order even when responses resolve out of order", async () => {
    // Two creates issued back-to-back: the second must not start (or
    // resolve) until the first settles, so snapshots can never apply out
    // of order regardless of backend response timing.
    const create1 = deferred<WorkspaceSnapshot>();
    const create2 = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc();
    ipc.create
      .mockReturnValueOnce(create1.promise)
      .mockReturnValueOnce(create2.promise);
    const store = createWorkspaceStore(ipc);
    await store.load();

    const run1 = store.create("Alpha");
    const run2 = store.create("Beta");
    await flush();

    expect(ipc.create).toHaveBeenCalledTimes(1); // second is queued
    create1.resolve(snapshot([{ id: "p1", name: "Alpha" }], "p1"));
    await run1;
    await flush();
    expect(ipc.create).toHaveBeenCalledTimes(2); // queued create now runs
    create2.resolve(
      snapshot(
        [
          { id: "p1", name: "Alpha" },
          { id: "p2", name: "Beta" },
        ],
        "p2",
      ),
    );
    await run2;

    // Final state converges on the last-issued operation's snapshot.
    expect(store.getState().data.projects).toHaveLength(2);
    expect(store.getState().data.selectedProjectId).toBe("p2");
    expect(store.getState().pendingAction).toBeNull();
  });

  it("rejects mutations queued while the workspace is unavailable, with feedback", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const store = createWorkspaceStore(ipc);
    await store.load(); // first load fails -> error phase

    await store.create("X");
    expect(ipc.create).not.toHaveBeenCalled();
    expect(store.getState().actionError?.message).toContain("unavailable");
  });

  it("runs a reload issued during a create strictly after it", async () => {
    // Initial load resolves empty; the reload issued during the create
    // returns the post-create backend state.
    const createPending = deferred<WorkspaceSnapshot>();
    const ipc = makeIpc({
      load: vi
        .fn()
        .mockResolvedValueOnce(snapshot([]))
        .mockResolvedValueOnce(snapshot([{ id: "p1", name: "Queued" }], "p1")),
    });
    ipc.create.mockReturnValue(createPending.promise);
    const store = createWorkspaceStore(ipc);
    await store.load();

    const createRun = store.create("Queued");
    const reloadRun = store.load(); // queued behind the create
    await flush();
    expect(ipc.load).toHaveBeenCalledTimes(1); // reload not started yet

    createPending.resolve(snapshot([{ id: "p1", name: "Queued" }], "p1"));
    await createRun;
    await reloadRun; // reload runs after the create settles

    // Final state reflects the reload that observed the created project.
    expect(store.getState().data.projects).toHaveLength(1);
    expect(store.getState().data.projects[0]?.name).toBe("Queued");
    expect(store.getState().pendingAction).toBeNull();
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

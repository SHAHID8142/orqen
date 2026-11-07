/**
 * Unit tests for the agent registry store against fake IPC surfaces.
 */
import { describe, expect, it, vi } from "vitest";
import { createAgentsStore, type AgentsIpc } from "./store";
import type { AgentRecord } from "../../types/agents";

function record(overrides: Partial<AgentRecord> = {}): AgentRecord {
  return {
    id: "codex",
    displayName: "OpenAI Codex CLI",
    provider: "OpenAI",
    adapter: "cli",
    executableName: "codex",
    capabilities: {
      discovery: true,
      verification: true,
      execution: false,
      orchestration: false,
      diagnosticExecution: false,
      diagnosticExecutionNote: "diagnostics disabled in this build",
    },
    discovery: { status: "not_checked" },
    verification: null,
    checkedAt: null,
    ...overrides,
  };
}

function makeIpc(overrides: Partial<AgentsIpc> = {}): AgentsIpc & {
  load: ReturnType<typeof vi.fn>;
  discover: ReturnType<typeof vi.fn>;
  verify: ReturnType<typeof vi.fn>;
  startRun: ReturnType<typeof vi.fn>;
  runStatus: ReturnType<typeof vi.fn>;
  cancelRun: ReturnType<typeof vi.fn>;
} {
  return {
    load: vi.fn().mockResolvedValue([record(), record({ id: "claude-code", displayName: "Claude Code", provider: "Anthropic", executableName: "claude" })]),
    discover: vi.fn(),
    verify: vi.fn(),
    startRun: vi.fn().mockResolvedValue("run-1"),
    runStatus: vi.fn().mockResolvedValue({ phase: "finished", result: { agentId: "codex", startedAt: "2026-01-01T00:00:00Z", finishedAt: "2026-01-01T00:00:05Z", durationMs: 5000, outcome: "succeeded", exitCode: 0, output: "OK", error: null } }),
    cancelRun: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as never;
}

describe("agents store — loading", () => {
  it("starts loading and applies the registry", async () => {
    const ipc = makeIpc();
    const store = createAgentsStore(ipc);
    expect(store.getState().phase).toBe("loading");

    await store.load();
    expect(store.getState().phase).toBe("ready");
    expect(store.getState().records).toHaveLength(2);
    expect(store.getState().records[0]?.discovery.status).toBe("not_checked");
    expect(store.getState().pendingAction).toBeNull();
  });

  it("enters the error phase on first-load failure and recovers on retry", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const store = createAgentsStore(ipc);
    await store.load();
    expect(store.getState().phase).toBe("error");
    expect(store.getState().loadError?.kind).toBe("storage");

    await store.load(); // retry
    expect(store.getState().phase).toBe("ready");
  });
});

describe("agents store — discovery and verification", () => {
  it("replaces the record with the discovered state", async () => {
    const ipc = makeIpc();
    ipc.discover.mockResolvedValue(
      record({ discovery: { status: "found", path: "/usr/local/bin/codex" } }),
    );
    const store = createAgentsStore(ipc);
    await store.load();

    await store.discover("codex");
    expect(ipc.discover).toHaveBeenCalledWith("codex");
    expect(store.getState().records[0]?.discovery).toEqual({
      status: "found",
      path: "/usr/local/bin/codex",
    });
    expect(store.getState().pendingAction).toBeNull();
  });

  it("applies not-found and error discovery states truthfully", async () => {
    const ipc = makeIpc();
    ipc.discover
      .mockResolvedValueOnce(record({ id: "codex", discovery: { status: "not_found" } }))
      .mockResolvedValueOnce(
        record({
          id: "claude-code",
          discovery: { status: "error", message: "The PATH environment variable is not set" },
        }),
      );
    const store = createAgentsStore(ipc);
    await store.load();

    await store.discover("codex");
    await store.discover("claude-code");
    expect(store.getState().records[0]?.discovery.status).toBe("not_found");
    expect(store.getState().records[1]?.discovery.status).toBe("error");
  });

  it("replaces the record after a successful verification", async () => {
    const ipc = makeIpc();
    ipc.verify.mockResolvedValue(
      record({
        discovery: { status: "found", path: "/usr/local/bin/codex" },
        verification: { status: "verified", version: "codex-cli 0.42.0" },
      }),
    );
    const store = createAgentsStore(ipc);
    await store.load();

    await store.verify("codex");
    expect(ipc.verify).toHaveBeenCalledWith("codex");
    expect(store.getState().records[0]?.verification).toEqual({
      status: "verified",
      version: "codex-cli 0.42.0",
    });
  });

  it("applies failed verification states without pretending success", async () => {
    const ipc = makeIpc();
    ipc.verify.mockResolvedValue(
      record({
        verification: { status: "failed", message: "verification timed out after 10s" },
      }),
    );
    const store = createAgentsStore(ipc);
    await store.load();

    await store.verify("codex");
    expect(store.getState().records[0]?.verification).toEqual({
      status: "failed",
      message: "verification timed out after 10s",
    });
    expect(store.getState().actionError).toBeNull();
  });

  it("surfaces backend failures without losing the registry", async () => {
    const ipc = makeIpc();
    const store = createAgentsStore(ipc);
    await store.load();

    ipc.verify.mockRejectedValue({ kind: "storage", message: "write failed" });
    await store.verify("codex");
    expect(store.getState().records).toHaveLength(2);
    expect(store.getState().actionError?.kind).toBe("storage");
  });
});

describe("agents store — guards and ordering", () => {
  it("rejects unknown agent ids without an IPC call", async () => {
    const ipc = makeIpc();
    const store = createAgentsStore(ipc);
    await store.load();

    await store.discover("ghost");
    await store.verify("ghost");
    expect(ipc.discover).not.toHaveBeenCalled();
    expect(ipc.verify).not.toHaveBeenCalled();
    expect(store.getState().actionError?.kind).toBe("invalid_input");
  });

  it("serializes a verify issued while a discover is in flight", async () => {
    const discoverPending = deferred<AgentRecord>();
    const ipc = makeIpc();
    ipc.discover.mockReturnValue(discoverPending.promise);
    ipc.verify.mockResolvedValue(record({ verification: { status: "verified", version: "1" } }));
    const store = createAgentsStore(ipc);
    await store.load();

    const discoverRun = store.discover("codex");
    const verifyRun = store.verify("codex");
    expect(ipc.verify).not.toHaveBeenCalled(); // queued behind discover

    discoverPending.resolve(
      record({ discovery: { status: "found", path: "/usr/local/bin/codex" } }),
    );
    await discoverRun;
    await verifyRun;
    expect(ipc.verify).toHaveBeenCalledTimes(1);
    expect(store.getState().records[0]?.verification?.status).toBe("verified");
  });

  it("rejects operations queued while the registry is unavailable, with feedback", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const store = createAgentsStore(ipc);
    await store.load(); // fails -> error phase

    await store.discover("codex");
    expect(ipc.discover).not.toHaveBeenCalled();
    expect(store.getState().actionError?.message).toContain("unavailable");
  });
});

/** Deferred promise for controlling backend timing in tests. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}


describe("agents store — diagnostic test-run state machine", () => {
  it("walks idle → awaiting_confirmation → starting → running → succeeded", async () => {
    let resolveRunStatus!: (value: { phase: "finished"; result: { agentId: string; startedAt: string; finishedAt: string; durationMs: number; outcome: "succeeded"; exitCode: number; output: string; error: null } }) => void;
    const ipc = makeIpc({
      runStatus: vi.fn().mockReturnValue(
        new Promise((resolve) => {
          resolveRunStatus = resolve;
        }),
      ),
    });
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    expect(store.getState().run.phase).toBe("awaiting_confirmation");

    const confirm = store.confirmTestRun();
    await vi.waitFor(() => expect(store.getState().run.phase).toBe("running"));
    expect(ipc.startRun).toHaveBeenCalledWith("codex");

    resolveRunStatus({
      phase: "finished",
      result: {
        agentId: "codex",
        startedAt: "2026-01-01T00:00:00Z",
        finishedAt: "2026-01-01T00:00:05Z",
        durationMs: 5000,
        outcome: "succeeded",
        exitCode: 0,
        output: "Agent available.",
        error: null,
      },
    });
    await confirm;
    expect(store.getState().run.phase).toBe("succeeded");
    expect(store.getState().run.result?.output).toBe("Agent available.");
  });

  it("rejects a second run while one is awaiting confirmation", async () => {
    const ipc = makeIpc();
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    store.requestTestRun("claude-code"); // ignored: already awaiting
    expect(store.getState().run.agentId).toBe("codex");
    expect(ipc.startRun).not.toHaveBeenCalled();
  });

  it("surfaces a failed run truthfully", async () => {
    const ipc = makeIpc({
      startRun: vi.fn().mockRejectedValue({ kind: "storage", message: "the executable could not be started" }),
    });
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    await store.confirmTestRun();
    expect(store.getState().run.phase).toBe("failed");
    expect(store.getState().run.error?.message).toContain("could not be started");
    expect(store.getState().run.result).toBeNull();
  });

  it("transitions to cancelled when the run is cancelled while running", async () => {
    const ipc = makeIpc({
      runStatus: vi.fn().mockImplementation(() => {
        // First poll: running; after cancel: finished-cancelled.
        return Promise.resolve(
          cancelledFlag
            ? {
                phase: "finished",
                result: {
                  agentId: "codex",
                  startedAt: "2026-01-01T00:00:00Z",
                  finishedAt: "2026-01-01T00:00:03Z",
                  durationMs: 3000,
                  outcome: "cancelled",
                  exitCode: null,
                  output: "",
                  error: "the run was cancelled.",
                },
              }
            : {
                phase: "running",
                agentId: "codex",
                startedAt: "2026-01-01T00:00:00Z",
              },
        );
      }),
    });
    let cancelledFlag = false;
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    const confirm = store.confirmTestRun();
    await vi.waitFor(() => expect(store.getState().run.phase).toBe("running"));

    cancelledFlag = true;
    await store.cancelRun();
    expect(ipc.cancelRun).toHaveBeenCalledWith("run-1");
    await confirm;
    expect(store.getState().run.phase).toBe("cancelled");
  });

  it("honors a cancel requested while the Start IPC is still pending", async () => {
    let cancelledFlag = false;
    const startDeferred = deferred<string>();
    const ipc = makeIpc({
      startRun: vi.fn().mockReturnValue(startDeferred.promise),
      runStatus: vi.fn().mockImplementation(() =>
        Promise.resolve(
          cancelledFlag
            ? {
                phase: "finished",
                result: {
                  agentId: "codex",
                  startedAt: "2026-01-01T00:00:00Z",
                  finishedAt: "2026-01-01T00:00:01Z",
                  durationMs: 1000,
                  outcome: "cancelled",
                  exitCode: null,
                  output: "",
                  error: "the run was cancelled.",
                },
              }
            : {
                phase: "running",
                agentId: "codex",
                startedAt: "2026-01-01T00:00:00Z",
              },
        ),
      ),
    });
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    const confirm = store.confirmTestRun();
    await vi.waitFor(() => expect(store.getState().run.phase).toBe("starting"));

    // Cancel while the Start IPC is pending: no run id exists yet, so no
    // backend call is possible — but the intent must be recorded, never
    // silently ignored.
    await store.cancelRun();
    expect(store.getState().run.cancelRequested).toBe(true);
    expect(ipc.cancelRun).not.toHaveBeenCalled();

    // The late Start response must not overwrite the cancel intent: the
    // store cancels the run the moment the id arrives.
    cancelledFlag = true;
    startDeferred.resolve("run-9");
    await vi.waitFor(() => expect(ipc.cancelRun).toHaveBeenCalledWith("run-9"));
    expect(store.getState().run.phase).toBe("running");
    expect(store.getState().run.cancelRequested).toBe(true);

    await confirm;
    expect(store.getState().run.phase).toBe("cancelled");
  });

  it("reports a start failure truthfully even when a cancel was requested during starting", async () => {
    const startDeferred = deferred<string>();
    const ipc = makeIpc({
      startRun: vi.fn().mockReturnValue(startDeferred.promise),
    });
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    const confirm = store.confirmTestRun();
    await vi.waitFor(() => expect(store.getState().run.phase).toBe("starting"));
    await store.cancelRun();
    expect(store.getState().run.cancelRequested).toBe(true);

    // The run never started, so the truthful terminal state is the start
    // failure itself — nothing was running to cancel.
    startDeferred.reject({ kind: "storage", message: "the executable could not be started" });
    await confirm;
    expect(store.getState().run.phase).toBe("failed");
    expect(store.getState().run.error?.message).toContain("could not be started");
    expect(ipc.cancelRun).not.toHaveBeenCalled();
  });

  it("dismisses a terminal run back to idle", async () => {
    const ipc = makeIpc();
    const store = createAgentsStore(ipc);
    await store.load();

    store.requestTestRun("codex");
    await store.confirmTestRun();
    await vi.waitFor(() => expect(store.getState().run.phase).toBe("succeeded"));

    store.dismissRun();
    expect(store.getState().run.phase).toBe("idle");
    expect(store.getState().run.result).toBeNull();
  });
})

/**
 * Agents surface tests against fake IPC: registry loading, discovery and
 * verification states, Main Agent preference wiring, and truthfulness.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { AgentsProvider } from "../../lib/agents/AgentsContext";
import { createAgentsStore, type AgentsIpc } from "../../lib/agents/store";
import { SettingsProvider } from "../../lib/settings/SettingsContext";
import { createSettingsStore, type SettingsIpc } from "../../lib/settings/store";
import Agents from "./Agents";
import type { AgentRecord } from "../../types/agents";
import type { Settings } from "../../types/workspace";

function agentRecord(overrides: Partial<AgentRecord> = {}): AgentRecord {
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

/** Lets queued store operations start before mid-flight assertions. */
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

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

function makeAgentsIpc(overrides: Partial<AgentsIpc> = {}): AgentsIpc & {
  load: ReturnType<typeof vi.fn>;
  discover: ReturnType<typeof vi.fn>;
  verify: ReturnType<typeof vi.fn>;
} {
  return {
    load: vi
      .fn()
      .mockResolvedValue([
        agentRecord(),
        agentRecord({
          id: "claude-code",
          displayName: "Claude Code",
          provider: "Anthropic",
          executableName: "claude",
        }),
      ]),
    discover: vi.fn(),
    verify: vi.fn(),
    ...overrides,
  } as never;
}

function makeSettingsIpc(
  overrides: Partial<SettingsIpc> = {},
): SettingsIpc & {
  load: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
  setMainAgent: ReturnType<typeof vi.fn>;
} {
  return {
    load: vi.fn().mockResolvedValue({
      approvalMode: "manual",
      maxRetry: 3,
      telegramNotifications: true,
      mainAgentId: null,
    }),
    update: vi.fn(),
    setMainAgent: vi.fn().mockImplementation(async (agentId) => ({
      approvalMode: "manual",
      maxRetry: 3,
      telegramNotifications: true,
      mainAgentId: agentId,
    })),
    ...overrides,
  } as never;
}

function renderAgents(agentsIpc = makeAgentsIpc(), settingsIpc = makeSettingsIpc()) {
  const agentsStore = createAgentsStore(agentsIpc);
  const settingsStore = createSettingsStore(settingsIpc);
  render(
    <AgentsProvider store={agentsStore}>
      <SettingsProvider store={settingsStore}>
        <Agents />
      </SettingsProvider>
    </AgentsProvider>,
  );
  return { agentsStore, settingsStore, agentsIpc, settingsIpc };
}

afterEach(() => {
  cleanup();
});

describe("Agents surface — persistence states", () => {
  it("shows a loading state before the registry arrives", () => {
    renderAgents();
    expect(screen.getByTestId("agents-loading")).toBeInTheDocument();
  });

  it("shows an error card with retry when loading fails", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const { agentsStore } = renderAgents(agentsIpc);
    void agentsStore.load();

    await waitFor(() => {
      expect(screen.getByTestId("agents-error")).toBeInTheDocument();
    });
    expect(screen.getByRole("alert")).toHaveTextContent("db unavailable");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.getByRole("article", { name: "OpenAI Codex CLI" })).toBeInTheDocument();
    });
  });
});

describe("Agents surface — registry display", () => {
  it("shows both registered agents with not-checked status and the honesty note", async () => {
    const { agentsStore } = renderAgents();
    await agentsStore.load();
    await waitFor(() => {
      expect(screen.getByRole("article", { name: "OpenAI Codex CLI" })).toBeInTheDocument();
    });

    expect(screen.getByRole("article", { name: "Claude Code" })).toBeInTheDocument();
    expect(screen.getAllByText("Not checked")).toHaveLength(2);
    expect(
      screen.getByText(/Agent execution and orchestration will be available/i),
    ).toBeInTheDocument();
    // Capability truthfulness: no execution claims anywhere.
    expect(screen.queryByText(/execution available/i)).not.toBeInTheDocument();
  });

  it("shows found state with the executable path after discovery", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.discover.mockResolvedValue(
      agentRecord({ discovery: { status: "found", path: "/usr/local/bin/codex" }, checkedAt: "2026-01-01T00:00:00Z" }),
    );
    const { agentsStore } = renderAgents(agentsIpc);
    await agentsStore.load();

    const card = screen.getByRole("article", { name: "OpenAI Codex CLI" });
    fireEvent.click(within(card).getByRole("button", { name: "Check availability" }));
    await waitFor(() => {
      expect(within(card).getByText("Executable found")).toBeInTheDocument();
    });
    expect(within(card).getByText("/usr/local/bin/codex")).toBeInTheDocument();
    expect(within(card).getByText(/Last checked/)).toBeInTheDocument();
  });

  it("shows not-found truthfully and disables Verify CLI", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.discover.mockResolvedValue(agentRecord({ discovery: { status: "not_found" } }));
    const { agentsStore } = renderAgents(agentsIpc);
    await agentsStore.load();

    const card = screen.getByRole("article", { name: "OpenAI Codex CLI" });
    fireEvent.click(within(card).getByRole("button", { name: "Check availability" }));
    await waitFor(() => {
      expect(within(card).getByText("Not found")).toBeInTheDocument();
    });
    expect(within(card).getByRole("button", { name: "Verify CLI" })).toBeDisabled();
  });

  it("shows the verified version after an explicit Verify CLI action", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.discover.mockResolvedValue(
      agentRecord({ discovery: { status: "found", path: "/usr/local/bin/codex" } }),
    );
    agentsIpc.verify.mockResolvedValue(
      agentRecord({
        discovery: { status: "found", path: "/usr/local/bin/codex" },
        verification: { status: "verified", version: "codex-cli 0.42.0" },
        checkedAt: "2026-01-01T00:00:00Z",
      }),
    );
    const { agentsStore } = renderAgents(agentsIpc);
    await agentsStore.load();

    await agentsStore.discover("codex");
    const card = screen.getByRole("article", { name: "OpenAI Codex CLI" });
    fireEvent.click(within(card).getByRole("button", { name: "Verify CLI" }));
    await waitFor(() => {
      expect(within(card).getByText("codex-cli 0.42.0")).toBeInTheDocument();
    });
    expect(agentsIpc.verify).toHaveBeenCalledWith("codex");
  });

  it("shows a disabled adapter's backend reason instead of a run button", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.load.mockResolvedValue([
      agentRecord({
        id: "codex",
        displayName: "OpenAI Codex CLI",
        discovery: { status: "found", path: "/usr/local/bin/codex" },
        verification: { status: "verified", version: "codex-cli 0.42.0" },
        capabilities: {
          discovery: true,
          verification: true,
          execution: true,
          orchestration: false,
          diagnosticExecution: false,
          diagnosticExecutionNote:
            "diagnostic execution for Codex is disabled: read confinement cannot be enforced per invocation",
        },
      }),
    ]);
    const { agentsStore } = renderAgents(agentsIpc);
    await agentsStore.load();

    // No run button may exist for a disabled adapter...
    expect(screen.queryByRole("button", { name: /Run test with/ })).not.toBeInTheDocument();
    // ...and the backend's reason must be shown verbatim.
    const notes = screen.getByTestId("diagnostic-disabled-notes");
    expect(notes).toHaveTextContent(
      "diagnostic execution for Codex is disabled: read confinement cannot be enforced per invocation",
    );
  });

  it("surfaces failed verification without claiming success", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.discover.mockResolvedValue(
      agentRecord({ discovery: { status: "found", path: "/usr/local/bin/codex" } }),
    );
    agentsIpc.verify.mockResolvedValue(
      agentRecord({
        discovery: { status: "found", path: "/usr/local/bin/codex" },
        verification: { status: "failed", message: "verification timed out after 10s" },
      }),
    );
    const { agentsStore } = renderAgents(agentsIpc);
    await agentsStore.load();
    await agentsStore.discover("codex");

    const card = screen.getByRole("article", { name: "OpenAI Codex CLI" });
    fireEvent.click(within(card).getByRole("button", { name: "Verify CLI" }));
    await waitFor(() => {
      expect(within(card).getByRole("alert")).toHaveTextContent(
        "verification timed out after 10s",
      );
    });
    expect(within(card).queryByText(/codex-cli/)).not.toBeInTheDocument();
  });
});

describe("Agents surface — settings failures (Main Agent UX)", () => {
  it("shows an actionable error and retry when settings loading fails", async () => {
    const settingsIpc = makeSettingsIpc();
    settingsIpc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    await Promise.all([agentsStore.load(), settingsStore.load()]);

    await waitFor(() => {
      expect(screen.getByTestId("main-agent-error")).toBeInTheDocument();
    });
    const mainAgentError = screen.getByTestId("main-agent-error");
    expect(mainAgentError).toHaveTextContent("db unavailable");

    // The Retry control must be enabled and actionable even while the
    // preference radios stay disabled (regression: it used to render
    // inside the disabled fieldset).
    const retry = within(mainAgentError).getByRole("button", {
      name: "Retry",
    });
    expect(retry).toBeEnabled();
    fireEvent.click(retry);

    await waitFor(() => {
      const group = screen.getByRole("group", { name: "Preferred Main Agent" });
      expect(within(group).getByLabelText(/^None /)).toBeEnabled();
    });
    // Full flow: failed load -> Retry -> successful load -> selection works.
    fireEvent.click(within(screen.getByRole("group", { name: "Preferred Main Agent" })).getByLabelText("Claude Code"));
    await waitFor(() => {
      expect(
        within(screen.getByRole("group", { name: "Preferred Main Agent" })).getByLabelText(
          "Claude Code",
        ),
      ).toBeChecked();
    });
    expect(settingsIpc.setMainAgent).toHaveBeenCalledWith("claude-code");
  });

  it("keeps the Retry control outside the disabled fieldset", async () => {
    const settingsIpc = makeSettingsIpc();
    settingsIpc.load.mockRejectedValueOnce({ kind: "storage", message: "db unavailable" });
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    void agentsStore.load();
    void settingsStore.load(); // the settings failure under test

    await waitFor(() => {
      expect(screen.getByTestId("main-agent-error")).toBeInTheDocument();
    });
    const fieldset = screen.getByRole("group", { name: "Preferred Main Agent" });
    expect(fieldset).toBeDisabled();
    const retry = within(screen.getByTestId("main-agent-error")).getByRole(
      "button",
      { name: "Retry" },
    );
    expect(retry).toBeEnabled();
    expect(retry.closest("fieldset")).toBeNull();
  });

  it("shows a save failure and preserves the previous preference", async () => {
    const settingsIpc = makeSettingsIpc();
    settingsIpc.setMainAgent.mockRejectedValueOnce({
      kind: "storage",
      message: "write failed",
    });
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    await Promise.all([agentsStore.load(), settingsStore.load()]);
    const group = screen.getByRole("group", { name: "Preferred Main Agent" });
    await waitFor(() => expect(within(group).getByLabelText("Claude Code")).toBeEnabled());

    fireEvent.click(within(group).getByLabelText("Claude Code"));
    await waitFor(() => {
      expect(screen.getByTestId("main-agent-action-error")).toHaveTextContent("write failed");
    });
    // The last valid preference (None) is preserved.
    expect(settingsStore.getState().data.mainAgentId).toBeNull();
    expect(within(group).getByLabelText(/^None /)).toBeChecked();
  });

  it("queues rapid selections in order so the last one wins", async () => {
    const pending = deferred<Settings>();
    const settingsIpc = makeSettingsIpc();
    settingsIpc.setMainAgent.mockReturnValueOnce(pending.promise);
    settingsIpc.setMainAgent.mockResolvedValueOnce({
      approvalMode: "manual",
      maxRetry: 3,
      telegramNotifications: true,
      mainAgentId: "codex",
    });
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    await Promise.all([agentsStore.load(), settingsStore.load()]);
    const group = screen.getByRole("group", { name: "Preferred Main Agent" });
    await waitFor(() => expect(within(group).getByLabelText("Claude Code")).toBeEnabled());
    fireEvent.click(within(group).getByLabelText("Claude Code")); // in flight
    fireEvent.click(within(group).getByLabelText("OpenAI Codex CLI")); // queued
    pending.resolve({
      approvalMode: "manual",
      maxRetry: 3,
      telegramNotifications: true,
      mainAgentId: "claude-code",
    });

    await waitFor(() => {
      expect(settingsIpc.setMainAgent).toHaveBeenCalledTimes(2);
    });
    expect(settingsIpc.setMainAgent).toHaveBeenNthCalledWith(1, "claude-code");
    expect(settingsIpc.setMainAgent).toHaveBeenNthCalledWith(2, "codex");
    // Final state converges on the last-issued selection.
    expect(settingsStore.getState().data.mainAgentId).toBe("codex");
  });

  it("keeps radios disabled while settings load (delayed load)", async () => {
    let resolveLoad!: (value: Settings) => void;
    const settingsIpc = makeSettingsIpc({
      load: vi.fn().mockReturnValue(
        new Promise<Settings>((resolve) => {
          resolveLoad = resolve;
        }),
      ),
    });
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    const settingsLoad = settingsStore.load(); // starts, hangs on the deferred IPC
    await agentsStore.load();
    await flush();
    // Agents registry ready, settings still loading.
    expect(screen.getByTestId("main-agent-loading")).toBeInTheDocument();
    const group = screen.getByRole("group", { name: "Preferred Main Agent" });
    expect(within(group).getByLabelText(/^None /)).toBeDisabled();

    resolveLoad({
      approvalMode: "manual",
      maxRetry: 3,
      telegramNotifications: true,
      mainAgentId: null,
    });
    await settingsLoad;
    await waitFor(() => {
      expect(within(group).getByLabelText(/^None /)).toBeEnabled();
    });
  });
});

describe("Agents surface — Main Agent preference", () => {
  it("offers None plus every registered agent, defaulting to None", async () => {
    const { agentsStore } = renderAgents();
    await agentsStore.load();
    const fieldset = screen.getByRole("group", { name: "Preferred Main Agent" });
    await waitFor(() => {
      expect(within(fieldset).getByLabelText(/^None /)).toBeInTheDocument();
    });
    expect(within(fieldset).getByLabelText("OpenAI Codex CLI")).not.toBeChecked();
    expect(within(fieldset).getByLabelText("Claude Code")).not.toBeChecked();
    expect(screen.getByText(/selecting an agent starts nothing today/i)).toBeInTheDocument();
  });

  it("saves the selected Main Agent through the settings store", async () => {
    const settingsIpc = makeSettingsIpc();
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    await agentsStore.load();
    await settingsStore.load();
    await waitFor(() =>
      expect(within(screen.getByRole("group", { name: "Preferred Main Agent" })).getByLabelText("Claude Code")).toBeEnabled(),
    );

    fireEvent.click(
      within(screen.getByRole("group", { name: "Preferred Main Agent" })).getByLabelText("Claude Code"),
    );
    await waitFor(() => {
      expect(
        within(screen.getByRole("group", { name: "Preferred Main Agent" })).getByLabelText(
          "Claude Code",
        ),
      ).toBeChecked();
    });
    expect(settingsIpc.setMainAgent).toHaveBeenCalledWith("claude-code");
    expect(settingsStore.getState().data.mainAgentId).toBe("claude-code");
  });

  it("marks an unavailable preferred agent while preserving the preference", async () => {
    const agentsIpc = makeAgentsIpc();
    agentsIpc.load.mockResolvedValue([
      agentRecord({ id: "codex", discovery: { status: "not_found" } }),
      agentRecord({
        id: "claude-code",
        displayName: "Claude Code",
        provider: "Anthropic",
        executableName: "claude",
        discovery: { status: "found", path: "/usr/local/bin/claude" },
      }),
    ]);
    const settingsIpc = makeSettingsIpc({
      load: vi.fn().mockResolvedValue({
        approvalMode: "manual",
        maxRetry: 3,
        telegramNotifications: true,
        mainAgentId: "codex",
      }),
    });
    const agentsStore = createAgentsStore(agentsIpc);
    const settingsStore = createSettingsStore(settingsIpc);
    render(
      <AgentsProvider store={agentsStore}>
        <SettingsProvider store={settingsStore}>
          <Agents />
        </SettingsProvider>
      </AgentsProvider>,
    );
    await Promise.all([agentsStore.load(), settingsStore.load()]);
    const fieldset = screen.getByRole("group", { name: "Preferred Main Agent" });
    await waitFor(() => {
      expect(within(fieldset).getByLabelText(/OpenAI Codex CLI/)).toBeChecked();
    });
    expect(within(fieldset).getByText(/executable currently unavailable/i)).toBeInTheDocument();
  });

  it("clears the preference with the None option", async () => {
    const settingsIpc = makeSettingsIpc({
      load: vi.fn().mockResolvedValue({
        approvalMode: "manual",
        maxRetry: 3,
        telegramNotifications: true,
        mainAgentId: "codex",
      }),
    });
    const { agentsStore, settingsStore } = renderAgents(makeAgentsIpc(), settingsIpc);
    await Promise.all([agentsStore.load(), settingsStore.load()]);
    await waitFor(() => expect(screen.getByLabelText(/^None /)).toBeEnabled());

    fireEvent.click(screen.getByLabelText(/^None /));
    await waitFor(() => {
      expect(screen.getByLabelText(/^None /)).toBeChecked();
    });
    expect(settingsIpc.setMainAgent).toHaveBeenCalledWith(null);
  });
});

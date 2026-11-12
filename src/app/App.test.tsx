/**
 * Application shell tests: navigation between the six surfaces,
 * active-state semantics, header project reflection, sidebar behavior
 * and keyboard support. IPC is mocked; everything else runs for real.
 */
import { act } from "react";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import { createAppChatStore } from "../lib/chat/ChatContext";
import { createWorkspaceStore } from "../lib/workspace/store";
import type { WorkspaceSnapshot } from "../types/workspace";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

/** Minimal backend simulation so the app runs end-to-end in tests. */
function installBackendSimulation(options: {
  loadWorkspace?: () => Promise<WorkspaceSnapshot>;
  seedProject?: boolean;
} = {}) {
  const projects: Array<{ id: string; name: string; createdAt: string }> = [];
  let selectedProjectId: string | null = null;
  if (options.seedProject) {
    projects.push({ id: "p-demo", name: "Demo project", createdAt: "2026-01-01T00:00:00Z" });
    selectedProjectId = "p-demo";
  }
  let chatConversations: Array<{ id: string; projectId: string; title: string; mode: string; createdAt: string; updatedAt: string; messageCount: number }> = [];
  const chatMessages: Array<{ id: string; conversationId: string; role: string; mode: string; content: string; createdAt: string }> = [];
  const settings = {
    approvalMode: "manual" as "manual" | "autonomous",
    maxRetry: 3,
    telegramNotifications: true,
    mainAgentId: null as string | null,
  };
  const snap = (): WorkspaceSnapshot => ({
    projects: projects.map((p) => ({ ...p })),
    selectedProjectId,
  });

  invokeMock.mockImplementation(async (command: string, args?: unknown) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    switch (command) {
      case "get_app_info":
        return { name: "Orqen", version: "0.1.0", os: "macos" };
      case "echo_ping":
        return "ping";
      case "load_workspace":
        return options.loadWorkspace ? options.loadWorkspace() : snap();
      case "create_project": {
        const project = {
          id: crypto.randomUUID(),
          name: String(payload.name).trim(),
          createdAt: "2026-01-01T00:00:00Z",
        };
        projects.push(project);
        selectedProjectId = project.id;
        return snap();
      }
      case "select_project": {
        const id = String(payload.id);
        if (!projects.some((p) => p.id === id)) {
          throw { kind: "invalid_input", message: "Unknown project." };
        }
        selectedProjectId = id;
        return snap();
      }
      case "delete_project": {
        const id = String(payload.id);
        if (!projects.some((p) => p.id === id)) {
          throw { kind: "invalid_input", message: "Unknown project." };
        }
        const index = projects.findIndex((p) => p.id === id);
        projects.splice(index, 1);
        if (selectedProjectId === id) selectedProjectId = null;
        return snap();
      }
      case "list_agents":
        return [
          {
            id: "codex",
            displayName: "OpenAI Codex CLI",
            provider: "OpenAI",
            adapter: "cli",
            executableName: "codex",
            capabilities: { discovery: true, verification: true, execution: false, orchestration: false },
            discovery: { status: "not_checked" },
            verification: null,
            checkedAt: null,
          },
          {
            id: "claude-code",
            displayName: "Claude Code",
            provider: "Anthropic",
            adapter: "cli",
            executableName: "claude",
            capabilities: { discovery: true, verification: true, execution: false, orchestration: false },
            discovery: { status: "not_checked" },
            verification: null,
            checkedAt: null,
          },
        ];
      case "list_chat_modes":
        return [
          { id: "interview", purpose: "Requirements gathering toward a Project Blueprint." },
          { id: "discussion", purpose: "Open exploration and proposals among user and agents; no commits." },
        ];
      case "list_conversations":
        return payload.projectId === "p-demo" ? chatConversations : [];
      case "create_conversation": {
        const conversation = {
          id: crypto.randomUUID(),
          projectId: String(payload.projectId),
          title: String(payload.title),
          mode: String(payload.mode),
          createdAt: "2026-01-02T00:00:00Z",
          updatedAt: "2026-01-02T00:00:00Z",
          messageCount: 0,
        };
        chatConversations = [conversation, ...chatConversations];
        return conversation;
      }
      case "append_message": {
        const conversationId = String(payload.conversationId);
        const message = {
          id: crypto.randomUUID(),
          conversationId,
          role: "user",
          mode: "discussion",
          content: String(payload.content),
          createdAt: "2026-01-02T00:01:00Z",
        };
        chatMessages.push(message);
        const conversation = chatConversations.find((c) => c.id === conversationId);
        if (conversation) conversation.messageCount = chatMessages.length;
        return {
          conversation,
          messages: chatMessages.filter((msg) => msg.conversationId === conversationId),
        };
      }
      case "export_conversation": {
        const conversationId = String(payload.id);
        const conversation = chatConversations.find((c) => c.id === conversationId);
        return {
          conversationId,
          title: conversation?.title ?? "Export",
          filename: "transcript.md",
          content: "# Conversation export\n\n## Transcript\n",
        };
      }
      case "discover_agent":
        return {
          id: String(payload.id),
          displayName: String(payload.id) === "codex" ? "OpenAI Codex CLI" : "Claude Code",
          provider: String(payload.id) === "codex" ? "OpenAI" : "Anthropic",
          adapter: "cli",
          executableName: String(payload.id) === "codex" ? "codex" : "claude",
          capabilities: { discovery: true, verification: true, execution: false, orchestration: false },
          discovery: { status: "found", path: `/usr/local/bin/${String(payload.id)}` },
          verification: null,
          checkedAt: "2026-01-01T00:00:00Z",
        };
      case "verify_agent":
        return {
          id: String(payload.id),
          displayName: String(payload.id) === "codex" ? "OpenAI Codex CLI" : "Claude Code",
          provider: String(payload.id) === "codex" ? "OpenAI" : "Anthropic",
          adapter: "cli",
          executableName: String(payload.id) === "codex" ? "codex" : "claude",
          capabilities: { discovery: true, verification: true, execution: false, orchestration: false },
          discovery: { status: "found", path: `/usr/local/bin/${String(payload.id)}` },
          verification: { status: "verified", version: "test 1.0.0" },
          checkedAt: "2026-01-01T00:00:00Z",
        };
      case "set_main_agent": {
        const agentId = (payload as { agentId: string | null }).agentId ?? null;
        settings.mainAgentId = agentId;
        return { ...settings };
      }
      case "load_settings":
        return { ...settings };
      case "update_settings": {
        const input = payload.settings as typeof settings;
        Object.assign(settings, input);
        return { ...settings };
      }
      default:
        throw new Error(`unexpected command: ${command}`);
    }
  });
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("App shell navigation", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    installBackendSimulation();
  });

  /** Lets queued store operations start before mid-flight assertions. */
  const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

  function renderApp(chatStore?: import("../lib/chat/store").ChatStore) {
    return render(
      <App
        workspace={createWorkspaceStore({
          load: () => invokeMock("load_workspace") as Promise<WorkspaceSnapshot>,
          create: (name) =>
            invokeMock("create_project", { name }) as Promise<WorkspaceSnapshot>,
          select: (id) => invokeMock("select_project", { id }) as Promise<WorkspaceSnapshot>,
          remove: (id) => invokeMock("delete_project", { id }) as Promise<WorkspaceSnapshot>,
        })}
        chat={chatStore}
      />,
    );
  }

  /** Renders the app with matchMedia stubbed to the narrow breakpoint. */
  function renderWithNarrowWindow() {
    vi.spyOn(window, "matchMedia").mockReturnValue({
      matches: true,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    } as unknown as MediaQueryList);
    return renderApp();
  }

  /** MatchMedia stub whose matches state tests can flip mid-test. */
  function stubControllableMatchMedia(initialMatches: boolean) {
    const listeners = new Set<(event: { matches: boolean }) => void>();
    const stub = {
      matches: initialMatches,
      addEventListener: (
        _type: string,
        listener: (event: { matches: boolean }) => void,
      ) => listeners.add(listener),
      removeEventListener: (
        _type: string,
        listener: (event: { matches: boolean }) => void,
      ) => listeners.delete(listener),
    };
    vi.spyOn(window, "matchMedia").mockReturnValue(
      stub as unknown as MediaQueryList,
    );
    return {
      setMatches(matches: boolean) {
        stub.matches = matches;
        for (const listener of listeners) listener({ matches });
      },
    };
  }

  it("renders all six navigation surfaces and starts on Overview", () => {
    renderApp();

    const nav = screen.getByRole("navigation", { name: "Workspace sections" });
    expect(nav).toBeInTheDocument();
    for (const label of ["Overview", "Projects", "Agents", "Chat", "Workflows", "Settings"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.getByRole("heading", { level: 1, name: "Overview" }),
    ).toBeInTheDocument();
  });

  it("marks the active surface with aria-current and switches views without reloads", async () => {
    renderApp();
    const hrefBefore = window.location.href;

    expect(screen.getByRole("button", { name: "Overview" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    fireEvent.click(screen.getByRole("button", { name: "Projects" }));
    expect(
      screen.getByRole("heading", { level: 1, name: "Projects" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Projects" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Overview" })).not.toHaveAttribute(
      "aria-current",
    );

    fireEvent.click(screen.getByRole("button", { name: "Agents" }));
    expect(
      screen.getByRole("heading", { level: 1, name: "Agents" }),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("article", { name: "OpenAI Codex CLI" })).toBeInTheDocument();
    });
    expect(
      screen.getByRole("article", { name: "Claude Code" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Agent execution and orchestration will be available/i),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Workflows" }));
    expect(
      screen.getByRole("heading", { level: 1, name: "Workflows" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("workflows-not-implemented")).toHaveTextContent(
      /not implemented yet/i,
    );

    // No full-page reload: the document URL never changed.
    expect(window.location.href).toBe(hrefBefore);
  });

  it("moves focus to the new view's heading on navigation", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(document.activeElement).toHaveTextContent("Settings");
  });

  it("provides a skip link to the main content", () => {
    renderApp();
    const skipLink = screen.getByRole("link", { name: "Skip to main content" });
    expect(skipLink).toHaveAttribute("href", "#main-content");
  });

  it("reflects the selected project in the header", async () => {
    renderApp();

    const selector = screen.getByRole("combobox", { name: "Project" });
    await waitFor(() => expect(selector).toHaveTextContent("No projects yet"));
    // An empty workspace keeps the selector disabled until a project exists.
    expect(selector).toBeDisabled();

    // Create a project via the Projects surface and check the header updates.
    fireEvent.click(screen.getByRole("button", { name: "Projects" }));
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Header Demo" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() => {
      expect(screen.getByTestId("header-project")).toHaveTextContent("Header Demo");
    });
    expect(screen.getByRole("combobox", { name: "Project" })).toBeEnabled();
  });

  it("disables the project selector while the workspace is loading", async () => {
    let resolveLoad!: (value: WorkspaceSnapshot) => void;
    installBackendSimulation({
      loadWorkspace: () =>
        new Promise<WorkspaceSnapshot>((resolve) => {
          resolveLoad = resolve;
        }),
    });
    renderApp();
    await flush(); // let the queued load reach the (pending) IPC call

    const selector = screen.getByRole("combobox", { name: "Project" });
    expect(selector).toBeDisabled();
    expect(selector).toHaveTextContent("Loading…");

    resolveLoad({
      projects: [{ id: "p1", name: "Ready", createdAt: "2026-01-01T00:00:00Z" }],
      selectedProjectId: "p1",
    });
    await waitFor(() => expect(selector).toBeEnabled());
    expect(selector).toHaveTextContent("Ready");
  });

  it("offers a retry control when the workspace cannot load", async () => {
    invokeMock.mockReset();
    let fail = true;
    installBackendSimulation({
      loadWorkspace: () =>
        fail
          ? Promise.reject({ kind: "storage", message: "database unavailable" })
          : Promise.resolve({ projects: [], selectedProjectId: null }),
    });
    renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Projects" }));
    await waitFor(() => {
      expect(screen.getByTestId("projects-error")).toHaveTextContent(
        "Could not load projects",
      );
    });
    expect(screen.getByRole("alert")).toHaveTextContent("database unavailable");

    fail = false;
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    });
  });

  it("toggles the sidebar with aria-expanded state", () => {
    renderApp();
    const toggle = screen.getByRole("button", { name: /navigation/i });

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByRole("navigation", { name: "Workspace sections" }),
    ).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("closes the overlay sidebar with Escape on narrow windows and restores focus", () => {
    const changeListeners = new Set<(event: { matches: boolean }) => void>();
    const mediaStub = {
      matches: true,
      addEventListener: (_: string, listener: (event: { matches: boolean }) => void) =>
        changeListeners.add(listener),
      removeEventListener: (_: string, listener: (event: { matches: boolean }) => void) =>
        changeListeners.delete(listener),
    };
    vi.spyOn(window, "matchMedia").mockReturnValue(
      mediaStub as unknown as MediaQueryList,
    );

    renderApp();
    const toggle = screen.getByRole("button", { name: /navigation/i });

    // On narrow windows the overlay sidebar starts closed.
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByRole("navigation", { name: "Workspace sections" }),
    ).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();
    expect(document.activeElement).toBe(toggle);
  });

  it("keeps the aria-controls target in the document while the sidebar is hidden", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // close
    const sidebar = document.getElementById("workspace-sidebar");
    expect(sidebar).not.toBeNull();
    expect(sidebar).toHaveAttribute("hidden");
  });

  it("moves focus to the first nav item when the narrow overlay opens", () => {
    renderWithNarrowWindow();

    fireEvent.click(screen.getByRole("button", { name: /navigation/i }));

    expect(document.activeElement).toHaveTextContent("Overview");
    expect(document.activeElement).toHaveAttribute("aria-current", "page");
  });

  it("restores focus to the toggle when the narrow backdrop is clicked", () => {
    const { container } = renderWithNarrowWindow();

    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // open
    expect(container.querySelector(".sidebar-backdrop")).toBeInTheDocument();

    fireEvent.click(container.querySelector(".sidebar-backdrop")!);
    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: /navigation/i }),
    );
  });

  it("keeps background content inert only while the narrow overlay is open", () => {
    const { container } = renderWithNarrowWindow();

    const inertTargets = () =>
      [container.querySelector("main"), container.querySelector("header"), container.querySelector(".skip-link")].map(
        (el) => el!.hasAttribute("inert"),
      );

    // Closed: nothing inert.
    expect(inertTargets()).toEqual([false, false, false]);

    // Open (exact names: the drawer's own "Close navigation" button also
    // exists once open).
    fireEvent.click(screen.getByRole("button", { name: "Show navigation" }));
    // Modal drawer: main, header (toggle included) and skip link are inert,
    // so Tab/Shift+Tab cannot reach obscured controls.
    expect(inertTargets()).toEqual([true, true, true]);

    fireEvent.click(screen.getByRole("button", { name: "Hide navigation" })); // close
    expect(inertTargets()).toEqual([false, false, false]);
  });

  it("never applies inert in desktop mode, even with the docked sidebar open", () => {
    const { container } = renderApp();
    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // close
    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // open

    for (const el of [container.querySelector("main"), container.querySelector("header"), container.querySelector(".skip-link")]) {
      expect(el).not.toHaveAttribute("inert");
    }
  });

  it("applies dialog semantics only in overlay mode", () => {
    const { unmount } = renderWithNarrowWindow();

    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // open
    const drawer = document.getElementById("workspace-sidebar")!;
    expect(drawer).toHaveAttribute("role", "dialog");
    expect(drawer).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("button", { name: "Close navigation" })).toBeInTheDocument();

    unmount();
    renderApp();
    const docked = document.getElementById("workspace-sidebar")!;
    expect(docked).not.toHaveAttribute("role");
    expect(docked).not.toHaveAttribute("aria-modal");
    expect(screen.queryByRole("button", { name: "Close navigation" })).not.toBeInTheDocument();
  });

  it("closes via the in-drawer close button and restores focus to the toggle", () => {
    renderWithNarrowWindow();

    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // open
    fireEvent.click(screen.getByRole("button", { name: "Close navigation" }));

    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: /navigation/i }),
    );
  });

  it("wraps Tab at the drawer boundaries in overlay mode", () => {
    renderWithNarrowWindow();

    fireEvent.click(screen.getByRole("button", { name: "Show navigation" })); // open
    const drawer = document.getElementById("workspace-sidebar")!;

    // Tab on the LAST drawer element wraps to the FIRST (the close button
    // is first in DOM order; the last is the Settings nav item).
    const settingsItem = screen.getByRole("button", { name: "Settings" });
    settingsItem.focus();
    fireEvent.keyDown(drawer, { key: "Tab" });
    expect(document.activeElement).toBe(
      drawer.querySelector(".sidebar-close"),
    );

    // Shift+Tab on the FIRST drawer element wraps to the LAST.
    fireEvent.keyDown(drawer, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(settingsItem);
  });

  it("does not intercept Tab in docked desktop mode", () => {
    renderApp(); // wide viewport, docked sidebar open

    const settingsItem = screen.getByRole("button", { name: "Settings" });
    settingsItem.focus();
    fireEvent.keyDown(
      document.getElementById("workspace-sidebar")!,
      { key: "Tab" },
    );
    // Native behavior untouched: focus stays where the browser puts it
    // (jsdom does not move focus), and the handler must not wrap it.
    expect(document.activeElement).toBe(settingsItem);
  });

  it("navigation selection closes the overlay and focuses the view heading, not the toggle", () => {
    renderWithNarrowWindow();

    fireEvent.click(screen.getByRole("button", { name: /navigation/i })); // open
    fireEvent.click(screen.getByRole("button", { name: "Projects" }));

    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { level: 1, name: "Projects" }),
    ).toHaveFocus();
    expect(document.activeElement).not.toBe(
      screen.getByRole("button", { name: /navigation/i }),
    );
  });

  it("restores focus to the toggle when the viewport narrows while focus is in the sidebar", () => {
    const media = stubControllableMatchMedia(false); // start wide, docked open
    renderApp();

    // Keyboard user is sitting on a nav item when the window narrows.
    screen.getByRole("button", { name: "Projects" }).focus();
    act(() => {
      media.setMatches(true); // wide -> narrow: sidebar collapses
    });

    expect(
      screen.queryByRole("navigation", { name: "Workspace sections" }),
    ).not.toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "Show navigation" }),
    );
  });

  it("removes overlay containment when the viewport widens while the drawer is open", () => {
    const media = stubControllableMatchMedia(true); // narrow from the start
    const { container } = renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Show navigation" })); // open
    expect(container.querySelector("main")).toHaveAttribute("inert");
    expect(document.getElementById("workspace-sidebar")).toHaveAttribute("role", "dialog");

    act(() => {
      media.setMatches(false); // narrow -> wide: drawer becomes docked
    });

    expect(
      screen.getByRole("navigation", { name: "Workspace sections" }),
    ).toBeInTheDocument(); // stays open, now docked
    expect(container.querySelector("main")).not.toHaveAttribute("inert");
    expect(container.querySelector("header")).not.toHaveAttribute("inert");
    expect(document.getElementById("workspace-sidebar")).not.toHaveAttribute("role");
  });
});

describe("App shell — Chat surface (Phase 4)", () => {
  function renderChatApp(seedProject: boolean) {
    installBackendSimulation({ seedProject });
    // A fresh chat store per test: module-level singletons would leak
    // state across tests. (renderApp lives in the sibling describe, so
    // this helper renders independently.)
    render(
      <App
        workspace={createWorkspaceStore({
          load: () => invokeMock("load_workspace") as Promise<WorkspaceSnapshot>,
          create: (name) =>
            invokeMock("create_project", { name }) as Promise<WorkspaceSnapshot>,
          select: (id) => invokeMock("select_project", { id }) as Promise<WorkspaceSnapshot>,
          remove: (id) => invokeMock("delete_project", { id }) as Promise<WorkspaceSnapshot>,
        })}
        chat={createAppChatStore()}
      />,
    );
  }

  it("navigates to Chat and renders the layout with the backend mode catalog", async () => {
    renderChatApp(true);
    fireEvent.click(screen.getByRole("button", { name: "Chat" }));
    expect(screen.getByRole("heading", { level: 1, name: "Chat" })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByTestId("chat-new-form")).toBeInTheDocument();
    });
    // The mode select is populated from the backend catalog.
    const modeSelect = screen.getByLabelText("Starting mode");
    expect(modeSelect).toHaveTextContent("Interview");
    expect(screen.getByText(/they never grant execution privileges/i)).toBeInTheDocument();
    // The conversation list is empty on a fresh project.
    expect(screen.getByTestId("chat-list-empty")).toBeInTheDocument();
  });

  it("creates a conversation, sends a message, and exports the transcript", async () => {
    renderChatApp(true);
    fireEvent.click(screen.getByRole("button", { name: "Chat" }));
    await waitFor(() => {
      expect(screen.getByLabelText("New conversation title")).toBeInTheDocument();
    });

    fireEvent.change(screen.getByLabelText("New conversation title"), {
      target: { value: "Owner interview" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Start conversation" }));
    // The title appears in BOTH the list entry and the thread heading.
    await waitFor(() => {
      expect(screen.getAllByText("Owner interview").length).toBeGreaterThanOrEqual(2);
    });

    fireEvent.change(screen.getByLabelText(/Message \(recorded as a user turn/), {
      target: { value: "Hello from the acceptance flow." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => {
      expect(screen.getByText("Hello from the acceptance flow.")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "Export transcript" }));
    await waitFor(() => {
      expect(screen.getByTestId("chat-export")).toBeInTheDocument();
    });
    expect(screen.getByText(/transcript\.md/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByTestId("chat-export")).not.toBeInTheDocument();
  });

  it("shows the no-project state when nothing is selected", () => {
    renderChatApp(false);
    fireEvent.click(screen.getByRole("button", { name: "Chat" }));
    expect(screen.getByTestId("chat-no-project")).toBeInTheDocument();
    expect(screen.getByText(/Chat conversations belong to a project/)).toBeInTheDocument();
  });
})

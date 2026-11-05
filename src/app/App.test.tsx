/**
 * Application shell tests: navigation between the five surfaces,
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
} = {}) {
  const projects: Array<{ id: string; name: string; createdAt: string }> = [];
  let selectedProjectId: string | null = null;
  const settings = {
    approvalMode: "manual" as "manual" | "autonomous",
    maxRetry: 3,
    telegramNotifications: true,
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

  function renderApp() {
    return render(
      <App
        workspace={createWorkspaceStore({
          load: () => invokeMock("load_workspace") as Promise<WorkspaceSnapshot>,
          create: (name) =>
            invokeMock("create_project", { name }) as Promise<WorkspaceSnapshot>,
          select: (id) => invokeMock("select_project", { id }) as Promise<WorkspaceSnapshot>,
          remove: (id) => invokeMock("delete_project", { id }) as Promise<WorkspaceSnapshot>,
        })}
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

  it("renders all five navigation surfaces and starts on Overview", () => {
    renderApp();

    const nav = screen.getByRole("navigation", { name: "Workspace sections" });
    expect(nav).toBeInTheDocument();
    for (const label of ["Overview", "Projects", "Agents", "Workflows", "Settings"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.getByRole("heading", { level: 1, name: "Overview" }),
    ).toBeInTheDocument();
  });

  it("marks the active surface with aria-current and switches views without reloads", () => {
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
    expect(screen.getByTestId("agents-not-installed")).toHaveTextContent(
      /not installed yet/i,
    );

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

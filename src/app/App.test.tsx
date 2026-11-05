/**
 * Application shell tests: navigation between the five surfaces,
 * active-state semantics, header project reflection, sidebar behavior
 * and keyboard support. IPC is mocked; everything else runs for real.
 */
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import App from "./App";
import { createWorkspaceStore } from "../lib/workspace/store";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("App shell navigation", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_app_info") {
        return { name: "Orqen", version: "0.1.0", os: "macos" };
      }
      if (command === "echo_ping") {
        return "ping";
      }
      throw new Error(`unexpected command: ${command}`);
    });
  });

  function renderApp() {
    return render(<App workspace={createWorkspaceStore()} />);
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

  it("reflects the selected project in the header", () => {
    renderApp();

    const selector = screen.getByRole("combobox", { name: "Project" });
    expect(selector).toBeDisabled();
    expect(selector).toHaveTextContent("No projects yet");

    // Create a project via the Projects surface and check the header updates.
    fireEvent.click(screen.getByRole("button", { name: "Projects" }));
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Header Demo" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    expect(screen.getByTestId("header-project")).toHaveTextContent("Header Demo");
    expect(screen.getByRole("combobox", { name: "Project" })).toBeEnabled();
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
});

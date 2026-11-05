/**
 * Projects surface tests against fake IPC: loading/error/retry states,
 * persistent create/select/delete flows, focus management and busy guards.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { WorkspaceProvider } from "../../lib/workspace/WorkspaceContext";
import {
  createWorkspaceStore,
  type WorkspaceIpc,
} from "../../lib/workspace/store";
import Projects from "./Projects";
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

function renderProjects(ipc: WorkspaceIpc) {
  const store = createWorkspaceStore(ipc);
  render(
    <WorkspaceProvider store={store}>
      <Projects />
    </WorkspaceProvider>,
  );
  return { store, ipc };
}

afterEach(() => {
  cleanup();
});

describe("Projects surface — persistence states", () => {
  it("shows a loading state before the snapshot arrives", () => {
    const ipc = makeIpc();
    renderProjects(ipc);
    expect(screen.getByTestId("projects-loading")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Projects" })).toHaveAttribute(
      "tabindex",
      "-1",
    );
  });

  it("shows an error card with retry when loading fails, then recovers", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "database unavailable" });
    const { store } = renderProjects(ipc);
    void store.load();

    await waitFor(() => {
      expect(screen.getByTestId("projects-error")).toBeInTheDocument();
    });
    expect(screen.getByRole("alert")).toHaveTextContent("database unavailable");
    expect(screen.queryByLabelText("New project name")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    });
  });
});

describe("Projects surface — persistent operations", () => {
  it("shows the empty state when no projects exist", async () => {
    const { store } = renderProjects(makeIpc());
    await store.load();
    await waitFor(() => {
      expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("New project name")).toBeEnabled();
  });

  it("creates a project persistently, selects it, and announces it", async () => {
    const ipc = makeIpc();
    ipc.create.mockResolvedValue(snapshot([{ id: "p1", name: "Demo App" }], "p1"));
    const { store } = renderProjects(ipc);
    await store.load();

    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "  Demo App  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() => {
      expect(screen.getByTestId("selected-note")).toHaveTextContent(
        "Active project: Demo App",
      );
    });
    expect(ipc.create).toHaveBeenCalledWith("Demo App");
    expect(screen.getByRole("status")).toHaveTextContent(/Project "Demo App" created/);
    expect(screen.getByLabelText("New project name")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Demo App" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("clears the input only when creation succeeds", async () => {
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Demo" }])) });
    const { store } = renderProjects(ipc);
    await store.load();

    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Demo" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(/already exists/);
    });
    expect(screen.getByLabelText("New project name")).toHaveValue("Demo");
  });

  it("shows a backend create failure without losing state", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Kept" }])),
    });
    ipc.create.mockRejectedValue({ kind: "storage", message: "write failed" });
    const { store } = renderProjects(ipc);
    await store.load();

    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Broken" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("write failed");
    });
    expect(screen.getByRole("button", { name: "Kept" })).toBeInTheDocument();
  });

  it("supports selection through the persisted store", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(
        snapshot(
          [
            { id: "p1", name: "A" },
            { id: "p2", name: "B" },
          ],
          "p2",
        ),
      ),
    });
    ipc.select.mockResolvedValue(
      snapshot(
        [
          { id: "p1", name: "A" },
          { id: "p2", name: "B" },
        ],
        "p1",
      ),
    );
    const { store } = renderProjects(ipc);
    await store.load();

    fireEvent.click(screen.getByRole("button", { name: "A" }));
    await waitFor(() => {
      expect(screen.getByTestId("selected-note")).toHaveTextContent(
        "Active project: A",
      );
    });
    expect(ipc.select).toHaveBeenCalledWith("p1");
  });
});

describe("Projects surface — delete focus management", () => {
  it("deletes only after confirmation and focuses a surviving element", async () => {
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
    ipc.remove.mockResolvedValue(snapshot([{ id: "p2", name: "B" }]));
    const { store } = renderProjects(ipc);
    await store.load();

    fireEvent.click(screen.getByRole("button", { name: "Delete project A" }));
    // Focus lands on Cancel (the safe choice).
    expect(document.activeElement).toHaveTextContent("Cancel");

    // Cancel keeps the project and restores focus to the Delete button.
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(document.activeElement).toBe(
        screen.getByRole("button", { name: "Delete project A" }),
      );
    });

    // Confirm: backend snapshot applied, focus on the surviving project.
    fireEvent.click(screen.getByRole("button", { name: "Delete project A" }));
    fireEvent.click(
      within(screen.getByRole("group", { name: "Confirm deleting A" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent(/deleted/);
    });
    expect(screen.getByRole("button", { name: "B" })).toBeInTheDocument();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "B" }));
  });

  it("focuses the name input when the last project is deleted", async () => {
    const ipc = makeIpc({
      load: vi.fn().mockResolvedValue(snapshot([{ id: "p1", name: "Only" }])),
    });
    ipc.remove.mockResolvedValue(snapshot([]));
    const { store } = renderProjects(ipc);
    await store.load();

    fireEvent.click(screen.getByRole("button", { name: "Delete project Only" }));
    fireEvent.click(
      within(screen.getByRole("group", { name: "Confirm deleting Only" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );

    await waitFor(() => {
      expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    });
    expect(document.activeElement).toBe(screen.getByLabelText("New project name"));
  });
});

/**
 * Projects surface tests: creation, validation, selection, deletion with
 * confirmation, and empty states — against a fresh in-memory store.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { WorkspaceProvider } from "../../lib/workspace/WorkspaceContext";
import { createWorkspaceStore, type WorkspaceStore } from "../../lib/workspace/store";
import Projects from "./Projects";

function renderProjects(store: WorkspaceStore = createWorkspaceStore()) {
  return render(
    <WorkspaceProvider store={store}>
      <Projects />
    </WorkspaceProvider>,
  );
}

afterEach(() => {
  cleanup();
});

describe("Projects surface", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows the empty state when no projects exist", () => {
    renderProjects();
    expect(screen.getByTestId("projects-empty")).toHaveTextContent(
      "No projects yet",
    );
    expect(screen.getByLabelText("New project name")).toBeInTheDocument();
  });

  it("creates a project, selects it, and announces it", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "  Demo App  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    const list = screen.getByRole("list", { name: "Projects" });
    expect(within(list).getByText("Demo App")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Demo App" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByTestId("selected-note")).toHaveTextContent(
      "Active project: Demo App",
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      /Project "Demo App" created/,
    );
    expect(screen.getByLabelText("New project name")).toHaveValue("");
  });

  it("rejects empty names with a visible alert and keeps the input", () => {
    renderProjects();
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Project name must not be empty.",
    );
    expect(screen.queryByRole("list", { name: "Projects" })).not.toBeInTheDocument();
  });

  it("rejects duplicate names", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Demo" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "  Demo  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    // Uniqueness compares trimmed names exactly (case-sensitive by design).
    expect(screen.getByRole("alert")).toHaveTextContent(/already exists/);
  });

  it("supports multiple projects and explicit selection", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "A" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "B" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    // B was auto-selected on creation; switch back to A.
    fireEvent.click(screen.getByRole("button", { name: "A" }));
    expect(screen.getByTestId("selected-note")).toHaveTextContent(
      "Active project: A",
    );
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("deletes a project only after inline confirmation", () => {
    const store = createWorkspaceStore();
    renderProjects(store);
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Doomed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    fireEvent.click(screen.getByRole("button", { name: "Delete project Doomed" }));

    const confirm = screen.getByRole("group", {
      name: "Confirm deleting Doomed",
    });
    expect(confirm).toHaveTextContent("This cannot be undone");

    // Cancel keeps the project.
    fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("list", { name: "Projects" })).toBeInTheDocument();

    // Confirm removes it and returns to the empty state.
    fireEvent.click(screen.getByRole("button", { name: "Delete project Doomed" }));
    fireEvent.click(
      within(
        screen.getByRole("group", { name: "Confirm deleting Doomed" }),
      ).getByRole("button", { name: "Delete" }),
    );
    expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(/deleted/);
  });

  it("moves focus to Cancel when the delete confirmation appears", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Doomed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete project Doomed" }));

    expect(document.activeElement).toHaveTextContent("Cancel");
  });

  it("restores focus to the item's Delete button when cancellation is chosen", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Doomed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete project Doomed" }));

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(
      screen.queryByRole("group", { name: "Confirm deleting Doomed" }),
    ).not.toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "Delete project Doomed" }),
    );
  });

  it("focuses the first surviving project after a confirmed deletion", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "A" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "B" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    fireEvent.click(screen.getByRole("button", { name: "Delete project A" }));
    fireEvent.click(
      within(
        screen.getByRole("group", { name: "Confirm deleting A" }),
      ).getByRole("button", { name: "Delete" }),
    );

    expect(screen.queryByRole("button", { name: "A" })).not.toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "B" }),
    );
  });

  it("focuses the name input when the last project is deleted", () => {
    renderProjects();
    fireEvent.change(screen.getByLabelText("New project name"), {
      target: { value: "Only" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));

    fireEvent.click(screen.getByRole("button", { name: "Delete project Only" }));
    fireEvent.click(
      within(
        screen.getByRole("group", { name: "Confirm deleting Only" }),
      ).getByRole("button", { name: "Delete" }),
    );

    expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByLabelText("New project name"),
    );
  });
});

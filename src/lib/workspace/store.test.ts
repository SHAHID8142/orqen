/**
 * Unit tests for the in-memory workspace store (no React involved).
 */
import { describe, expect, it, vi } from "vitest";
import { createWorkspaceStore, validateProjectName } from "./store";
import { MAX_PROJECT_NAME_LENGTH } from "./types";

describe("validateProjectName", () => {
  it("accepts a normal name", () => {
    expect(validateProjectName("My App", [])).toBeNull();
  });

  it("rejects empty and whitespace-only names", () => {
    expect(validateProjectName("", [])).toBe("empty");
    expect(validateProjectName("   ", [])).toBe("empty");
  });

  it("rejects names beyond the code-point limit", () => {
    expect(validateProjectName("a".repeat(MAX_PROJECT_NAME_LENGTH + 1), [])).toBe(
      "too_long",
    );
    // Regression for D-012: 60 emoji are 120 UTF-16 units but 60 code points.
    expect(validateProjectName("🙂".repeat(60), [])).toBeNull();
  });

  it("rejects duplicate names after trimming", () => {
    const existing = [
      { id: "p1", name: "Orqen Demo", createdAt: new Date().toISOString() },
    ];
    expect(validateProjectName("  Orqen Demo  ", existing)).toBe("duplicate");
  });

  it("accepts Bangla and emoji names", () => {
    expect(validateProjectName("বাংলা প্রজেক্ট", [])).toBeNull();
    expect(validateProjectName("🙂👍🏽 project", [])).toBeNull();
  });
});

describe("createWorkspaceStore", () => {
  it("starts empty with no selection", () => {
    const store = createWorkspaceStore();
    expect(store.getState()).toEqual({ projects: [], selectedProjectId: null });
  });

  it("creates a trimmed project and selects it", () => {
    const store = createWorkspaceStore();
    const result = store.createProject("  Demo  ");
    expect(result).toEqual({
      ok: true,
      project: { id: expect.any(String), name: "Demo", createdAt: expect.any(String) },
    });
    const state = store.getState();
    expect(state.projects).toHaveLength(1);
    expect(state.selectedProjectId).toBe(state.projects[0]?.id);
  });

  it("rejects invalid names without changing state", () => {
    const store = createWorkspaceStore();
    store.createProject("Demo");
    const before = store.getState();

    expect(store.createProject("")).toEqual({ ok: false, error: "empty" });
    expect(store.createProject("Demo")).toEqual({ ok: false, error: "duplicate" });
    expect(store.createProject("a".repeat(MAX_PROJECT_NAME_LENGTH + 1))).toEqual({
      ok: false,
      error: "too_long",
    });
    expect(store.getState()).toEqual(before);
  });

  it("supports multiple projects and switching selection", () => {
    const store = createWorkspaceStore();
    const a = store.createProject("A");
    const b = store.createProject("B");
    if (!a.ok || !b.ok) throw new Error("setup failed");
    expect(store.getState().selectedProjectId).toBe(b.project.id);
    store.selectProject(a.project.id);
    expect(store.getState().selectedProjectId).toBe(a.project.id);
    store.selectProject("nonexistent");
    expect(store.getState().selectedProjectId).toBe(a.project.id);
  });

  it("notifies subscribers on every state change and on unsubscribe stops", () => {
    const store = createWorkspaceStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    store.createProject("A");
    expect(listener).toHaveBeenCalled();
    listener.mockClear();

    unsubscribe();
    store.createProject("B");
    expect(listener).not.toHaveBeenCalled();
  });

  it("deletes a project and clears selection when the selected one is removed", () => {
    const store = createWorkspaceStore();
    const a = store.createProject("A");
    store.createProject("B");
    if (!a.ok) throw new Error("setup failed");

    store.deleteProject(a.project.id);
    const state = store.getState();
    expect(state.projects.map((p) => p.name)).toEqual(["B"]);
    expect(state.selectedProjectId).not.toBeNull(); // selection moved off A only

    store.selectProject(state.projects[0]!.id);
    store.deleteProject(state.projects[0]!.id);
    expect(store.getState()).toEqual({ projects: [], selectedProjectId: null });
  });

  it("ignores deletion of unknown ids", () => {
    const store = createWorkspaceStore();
    const before = store.getState();
    store.deleteProject("nonexistent");
    expect(store.getState()).toEqual(before);
  });
});

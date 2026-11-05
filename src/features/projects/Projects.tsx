import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  useWorkspace,
  useWorkspaceStore,
} from "../../lib/workspace/WorkspaceContext";
import type { ProjectNameError } from "../../lib/workspace/types";
import { MAX_PROJECT_NAME_LENGTH } from "../../lib/workspace/types";
import "./Projects.css";

const ERROR_TEXT: Record<ProjectNameError, string> = {
  empty: "Project name must not be empty.",
  too_long: `Project name must not exceed ${MAX_PROJECT_NAME_LENGTH} characters.`,
  duplicate: "A project with this name already exists.",
};

/**
 * Where keyboard focus should land after the current interaction settles
 * (delete confirmation flows remount rows, so focus is restored by effect).
 */
type FocusTarget =
  | { kind: "delete-button"; projectId: string }
  | { kind: "select-button"; projectId: string }
  | { kind: "name-input" };

/**
 * Projects surface: in-memory creation, selection and deletion.
 * Session state only — nothing persists across restarts (Phase 2 scope).
 */
function Projects() {
  const workspace = useWorkspace();
  const store = useWorkspaceStore();
  const [nameInput, setNameInput] = useState("");
  const [nameError, setNameError] = useState<ProjectNameError | null>(null);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [pendingFocus, setPendingFocus] = useState<FocusTarget | null>(null);

  const nameInputRef = useRef<HTMLInputElement>(null);
  const selectButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const deleteButtonRefs = useRef(new Map<string, HTMLButtonElement>());

  useEffect(() => {
    if (!pendingFocus) return;
    const target =
      pendingFocus.kind === "name-input"
        ? nameInputRef.current
        : pendingFocus.kind === "select-button"
          ? (selectButtonRefs.current.get(pendingFocus.projectId) ?? null)
          : (deleteButtonRefs.current.get(pendingFocus.projectId) ?? null);
    target?.focus();
    setPendingFocus(null);
  }, [pendingFocus]);

  const selectedProject = workspace.projects.find(
    (p) => p.id === workspace.selectedProjectId,
  );

  function handleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const result = store.createProject(nameInput);
    if (!result.ok) {
      setNameError(result.error);
      return;
    }
    setNameError(null);
    setNameInput("");
    setAnnouncement(`Project "${result.project.name}" created and selected.`);
  }

  function handleCancelDelete(projectId: string) {
    setPendingDeleteId(null);
    setPendingFocus({ kind: "delete-button", projectId });
  }

  function handleDeleteConfirmed() {
    if (!pendingDeleteId) return;
    const target = workspace.projects.find((p) => p.id === pendingDeleteId);
    const remaining = workspace.projects.filter((p) => p.id !== pendingDeleteId);
    store.deleteProject(pendingDeleteId);
    setPendingDeleteId(null);
    setPendingFocus(
      remaining[0]
        ? { kind: "select-button", projectId: remaining[0].id }
        : { kind: "name-input" },
    );
    if (target) setAnnouncement(`Project "${target.name}" deleted.`);
  }

  return (
    <section aria-labelledby="projects-heading" className="projects">
      <h1 id="projects-heading" tabIndex={-1}>
        Projects
      </h1>
      <p className="surface-lead">
        In-memory workspaces for the current session. Nothing is stored on
        disk yet — persistence arrives in Phase 2.
      </p>

      <form className="project-create" onSubmit={handleCreate} noValidate>
        <label htmlFor="project-name">New project name</label>
        <div className="project-create-row">
          <input
            id="project-name"
            ref={nameInputRef}
            type="text"
            value={nameInput}
            onChange={(event) => {
              setNameInput(event.target.value);
              setNameError(null);
            }}
            aria-invalid={nameError ? true : undefined}
            aria-describedby={nameError ? "project-name-error" : undefined}
            placeholder="e.g. My App"
            autoComplete="off"
          />
          <button type="submit">Create project</button>
        </div>
        {nameError && (
          <p id="project-name-error" className="field-error" role="alert">
            {ERROR_TEXT[nameError]}
          </p>
        )}
      </form>

      <p className="visually-hidden" role="status" aria-live="polite">
        {announcement}
      </p>

      {workspace.projects.length === 0 ? (
        <div className="empty-state" data-testid="projects-empty">
          <h2>No projects yet</h2>
          <p>Create your first project above to start organizing work.</p>
        </div>
      ) : (
        <ul className="project-list" aria-label="Projects">
          {workspace.projects.map((project) => {
            const isSelected = project.id === workspace.selectedProjectId;
            return (
              <li
                key={project.id}
                className={`project-item${isSelected ? " selected" : ""}`}
              >
                {pendingDeleteId === project.id ? (
                  <div className="delete-confirm" role="group" aria-label={`Confirm deleting ${project.name}`}>
                    <span>Delete “{project.name}”? This cannot be undone.</span>
                    <div className="delete-confirm-actions">
                      <button type="button" className="danger" onClick={handleDeleteConfirmed}>
                        Delete
                      </button>
                      {/* Focus lands here when the confirmation appears:
                          the safe choice for a destructive action. */}
                      <button
                        type="button"
                        autoFocus
                        onClick={() => handleCancelDelete(project.id)}
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <button
                      type="button"
                      className="project-select-button"
                      aria-label={project.name}
                      aria-pressed={isSelected}
                      ref={(el) => {
                        if (el) selectButtonRefs.current.set(project.id, el);
                        else selectButtonRefs.current.delete(project.id);
                      }}
                      onClick={() => store.selectProject(project.id)}
                    >
                      <span className="project-name">{project.name}</span>
                      <span className="project-meta">
                        {isSelected ? "Selected" : "Created " + project.createdAt.slice(0, 10)}
                      </span>
                    </button>
                    <button
                      type="button"
                      className="project-delete"
                      aria-label={`Delete project ${project.name}`}
                      ref={(el) => {
                        if (el) deleteButtonRefs.current.set(project.id, el);
                        else deleteButtonRefs.current.delete(project.id);
                      }}
                      onClick={() => setPendingDeleteId(project.id)}
                    >
                      Delete
                    </button>
                  </>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {selectedProject && (
        <p className="selected-note" data-testid="selected-note">
          Active project: <strong>{selectedProject.name}</strong>
        </p>
      )}
    </section>
  );
}

export default Projects;

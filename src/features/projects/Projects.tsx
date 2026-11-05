import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  useWorkspace,
  useWorkspaceStore,
} from "../../lib/workspace/WorkspaceContext";
import "./Projects.css";

/**
 * Where keyboard focus should land after the current interaction settles
 * (delete confirmation flows remount rows, so focus is restored by effect).
 */
type FocusTarget =
  | { kind: "delete-button"; projectId: string }
  | { kind: "select-button"; projectId: string }
  | { kind: "name-input" };

/**
 * Projects surface: persistent creation, selection and deletion.
 * All operations run through the Rust core (SQLite); the store applies
 * the fresh backend snapshot after every mutation.
 */
function Projects() {
  const workspace = useWorkspace();
  const store = useWorkspaceStore();
  const [nameInput, setNameInput] = useState("");
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

  const ready = workspace.phase === "ready";
  const projects = workspace.data.projects;
  const selectedProject = projects.find(
    (p) => p.id === workspace.data.selectedProjectId,
  );
  const busy = workspace.pendingAction !== null;

  async function handleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!ready) return;
    await store.create(nameInput);
    // The store surfaces validation/backend errors via actionError; clear
    // the input only when the name was accepted (project list changed).
    const accepted = store.getState().actionError === null;
    if (accepted) {
      const createdName = nameInput.trim();
      setNameInput("");
      setAnnouncement(`Project "${createdName}" created and selected.`);
    }
  }

  async function handleSelect(id: string) {
    if (!ready) return;
    await store.select(id);
  }

  async function handleCancelDelete(projectId: string) {
    setPendingDeleteId(null);
    setPendingFocus({ kind: "delete-button", projectId });
  }

  async function handleDeleteConfirmed() {
    if (!pendingDeleteId || !ready) return;
    const target = projects.find((p) => p.id === pendingDeleteId);
    await store.remove(pendingDeleteId);
    setPendingDeleteId(null);
    if (store.getState().actionError === null) {
      if (target) setAnnouncement(`Project "${target.name}" deleted.`);
      // Focus a meaningful surviving element (state is already updated).
      const surviving = store.getState().data.projects;
      setPendingFocus(
        surviving[0]
          ? { kind: "select-button", projectId: surviving[0].id }
          : { kind: "name-input" },
      );
    }
  }

  const actionError = workspace.actionError ?? workspace.loadError;
  const nameError =
    actionError?.kind === "invalid_input" ? actionError : null;

  if (workspace.phase === "loading") {
    return (
      <section aria-labelledby="projects-heading" aria-busy="true" className="projects">
        <h1 id="projects-heading" tabIndex={-1}>
          Projects
        </h1>
        <p className="surface-lead" data-testid="projects-loading">
          Loading projects…
        </p>
      </section>
    );
  }

  if (workspace.phase === "error") {
    return (
      <section aria-labelledby="projects-heading" className="projects">
        <h1 id="projects-heading" tabIndex={-1}>
          Projects
        </h1>
        <div className="error-card" role="alert" data-testid="projects-error">
          <h2>Could not load projects</h2>
          <p>
            {workspace.loadError?.message ??
              "The local database is unavailable. Your data is untouched."}
          </p>
          <button type="button" onClick={() => void store.load()}>
            Retry
          </button>
        </div>
      </section>
    );
  }

  return (
    <section aria-labelledby="projects-heading" className="projects">
      <h1 id="projects-heading" tabIndex={-1}>
        Projects
      </h1>
      <p className="surface-lead">
        Persisted locally via the Rust core — projects survive application
        restarts.
      </p>

      <form
        className="project-create"
        onSubmit={(event) => {
          void handleCreate(event);
        }}
        noValidate
      >
        <label htmlFor="project-name">New project name</label>
        <div className="project-create-row">
          <input
            id="project-name"
            ref={nameInputRef}
            type="text"
            value={nameInput}
            onChange={(event) => {
              setNameInput(event.target.value);
            }}
            aria-invalid={nameError ? true : undefined}
            aria-describedby={nameError ? "project-name-error" : undefined}
            placeholder="e.g. My App"
            autoComplete="off"
            disabled={busy}
          />
          <button type="submit" disabled={busy}>
            {workspace.pendingAction?.kind === "create" ? "Creating…" : "Create project"}
          </button>
        </div>
        {actionError && (
          <p
            id="project-name-error"
            className="field-error"
            role="alert"
            data-testid="projects-action-error"
          >
            {actionError.message}
          </p>
        )}
      </form>

      <p className="visually-hidden" role="status" aria-live="polite">
        {announcement}
      </p>

      {projects.length === 0 ? (
        <div className="empty-state" data-testid="projects-empty">
          <h2>No projects yet</h2>
          <p>Create your first project above to start organizing work.</p>
        </div>
      ) : (
        <ul className="project-list" aria-label="Projects">
          {projects.map((project) => {
            const isSelected = project.id === workspace.data.selectedProjectId;
            return (
              <li
                key={project.id}
                className={`project-item${isSelected ? " selected" : ""}`}
              >
                {pendingDeleteId === project.id ? (
                  <div
                    className="delete-confirm"
                    role="group"
                    aria-label={`Confirm deleting ${project.name}`}
                  >
                    <span>Delete “{project.name}”? This cannot be undone.</span>
                    <div className="delete-confirm-actions">
                      <button
                        type="button"
                        className="danger"
                        disabled={busy}
                        onClick={() => {
                          void handleDeleteConfirmed();
                        }}
                      >
                        Delete
                      </button>
                      {/* Focus lands here when the confirmation appears:
                          the safe choice for a destructive action. */}
                      <button
                        type="button"
                        autoFocus
                        disabled={busy}
                        onClick={() => {
                          void handleCancelDelete(project.id);
                        }}
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
                      disabled={busy}
                      onClick={() => {
                        void handleSelect(project.id);
                      }}
                    >
                      <span className="project-name">{project.name}</span>
                      <span className="project-meta">
                        {isSelected
                          ? "Selected"
                          : "Created " + project.createdAt.slice(0, 10)}
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
                      disabled={busy}
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

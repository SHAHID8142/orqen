import { useEffect, useRef, useState } from "react";
import type { ChangeEvent, ReactNode } from "react";
import { useWorkspace, useWorkspaceStore } from "../lib/workspace/WorkspaceContext";
import SidebarNav from "./SidebarNav";
import { NARROW_WIDTH_QUERY, type ViewId } from "./views";

/**
 * Application shell: header, collapsible sidebar navigation, main content.
 *
 * Accessibility: skip link, labeled controls, focus moves to the active
 * view's heading on navigation. On narrow widths the sidebar is an
 * overlay: opening moves focus to the first nav item, background content
 * becomes `inert`, and every close path (Escape, backdrop, toggle)
 * restores focus to the toggle. The sidebar stays mounted with `hidden`
 * so `aria-controls` always points at a real element — no focus traps,
 * no hidden tab stops.
 */
function AppShell({
  activeView,
  onNavigate,
  children,
}: {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
  children: ReactNode;
}) {
  const workspace = useWorkspace();
  const workspaceStore = useWorkspaceStore();
  const [isNarrow, setIsNarrow] = useState(
    () => window.matchMedia(NARROW_WIDTH_QUERY).matches,
  );
  const [sidebarOpen, setSidebarOpen] = useState(
    () => !window.matchMedia(NARROW_WIDTH_QUERY).matches,
  );
  const toggleRef = useRef<HTMLButtonElement>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const mainRef = useRef<HTMLElement>(null);

  // Track the narrow-width breakpoint so the sidebar can switch between
  // docked and overlay behavior without CSS/JS duplication.
  useEffect(() => {
    const query = window.matchMedia(NARROW_WIDTH_QUERY);
    const onChange = (event: MediaQueryListEvent) => {
      setIsNarrow(event.matches);
      setSidebarOpen(!event.matches);
    };
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const closeOverlayAndRestoreFocus = () => {
    setSidebarOpen(false);
    toggleRef.current?.focus();
  };

  // Escape closes the overlay sidebar and returns focus to the toggle.
  useEffect(() => {
    if (!sidebarOpen || !isNarrow) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setSidebarOpen(false);
        toggleRef.current?.focus();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [sidebarOpen, isNarrow]);

  // Opening the narrow overlay moves focus into it (first nav item).
  useEffect(() => {
    if (!sidebarOpen || !isNarrow) return;
    sidebarRef.current?.querySelector<HTMLElement>(".nav-item")?.focus();
  }, [sidebarOpen, isNarrow]);

  // Focus the active view's heading after navigation (skip link target
  // stays available for explicit jumps).
  useEffect(() => {
    const heading = mainRef.current?.querySelector("h1");
    heading?.focus();
  }, [activeView]);

  function handleNavigate(view: ViewId) {
    onNavigate(view);
    if (isNarrow) setSidebarOpen(false);
  }

  function handleProjectSelect(event: ChangeEvent<HTMLSelectElement>) {
    if (event.target.value) {
      workspaceStore.selectProject(event.target.value);
    }
  }

  const selectedProject = workspace.projects.find(
    (p) => p.id === workspace.selectedProjectId,
  );

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        Skip to main content
      </a>

      <header className="app-header">
        <button
          ref={toggleRef}
          type="button"
          className="sidebar-toggle"
          aria-expanded={sidebarOpen}
          aria-controls="workspace-sidebar"
          onClick={() => setSidebarOpen((open) => !open)}
        >
          <span aria-hidden="true">☰</span>
          <span className="visually-hidden">
            {sidebarOpen ? "Hide navigation" : "Show navigation"}
          </span>
        </button>

        <div className="app-brand">
          <span className="app-logo" aria-hidden="true">
            O
          </span>
          <span className="app-name">Orqen</span>
          {selectedProject && (
            <span className="app-project" data-testid="header-project">
              {selectedProject.name}
            </span>
          )}
        </div>

        <div className="app-header-controls">
          <label className="project-select">
            <span>Project</span>
            <select
              value={workspace.selectedProjectId ?? ""}
              onChange={handleProjectSelect}
              disabled={workspace.projects.length === 0}
            >
              {workspace.projects.length === 0 ? (
                <option value="">No projects yet</option>
              ) : (
                workspace.selectedProjectId === null && (
                  <option value="">Select a project…</option>
                )
              )}
              {workspace.projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </label>
          <span className="phase-badge">Phase 1 · Workspace</span>
        </div>
      </header>

      <aside
        id="workspace-sidebar"
        ref={sidebarRef}
        className={`app-sidebar${isNarrow ? " overlay" : ""}`}
        hidden={!sidebarOpen}
      >
        <SidebarNav activeView={activeView} onNavigate={handleNavigate} />
        <p className="sidebar-footnote">
          In-memory session state — persistence arrives in Phase 2.
        </p>
      </aside>
      {isNarrow && sidebarOpen && (
        <div
          className="sidebar-backdrop"
          onClick={closeOverlayAndRestoreFocus}
          aria-hidden="true"
        />
      )}

      <main
        id="main-content"
        ref={mainRef}
        className="app-main"
        tabIndex={-1}
        // While the narrow-width overlay is open, keep background content
        // out of the tab order (React 19 boolean `inert`).
        inert={isNarrow && sidebarOpen ? true : undefined}
      >
        {children}
      </main>
    </div>
  );
}

export default AppShell;

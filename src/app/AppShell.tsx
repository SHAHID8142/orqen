import { useCallback, useEffect, useRef, useState } from "react";
import type { ChangeEvent, KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";
import { useWorkspace, useWorkspaceStore } from "../lib/workspace/WorkspaceContext";
import SidebarNav from "./SidebarNav";
import { NARROW_WIDTH_QUERY, type ViewId } from "./views";

/**
 * Application shell: header, collapsible sidebar navigation, main content.
 *
 * Narrow widths turn the sidebar into a modal drawer (WAI-ARIA APG modal
 * drawer pattern): focus moves into the drawer on open, and every element
 * outside it (skip link, header, main) becomes `inert`, so Tab and
 * Shift+Tab cannot reach obscured controls — containment is the platform
 * mechanism, not a JS tab-trap. The drawer offers a close button and
 * Escape; every close path without navigation restores focus to the
 * toggle *after* the commit that removes `inert`. On desktop the sidebar
 * is a plain docked region with normal keyboard navigation.
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
  // Set when a close is pending that should hand focus back to the toggle;
  // consumed by an effect so focus happens after `inert` is removed.
  const [pendingFocusRestore, setPendingFocusRestore] = useState(false);

  const toggleRef = useRef<HTMLButtonElement>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const mainRef = useRef<HTMLElement>(null);

  const overlayActive = isNarrow && sidebarOpen;

  // Track the narrow-width breakpoint so the sidebar can switch between
  // docked and overlay behavior without CSS/JS duplication.
  useEffect(() => {
    const query = window.matchMedia(NARROW_WIDTH_QUERY);
    const onChange = (event: MediaQueryListEvent) => {
      // If focus sits inside the sidebar while it collapses (wide ->
      // narrow), it would be stranded on <body> — hand it to the toggle.
      const focusInSidebar =
        sidebarRef.current?.contains(document.activeElement) ?? false;
      setIsNarrow(event.matches);
      setSidebarOpen(!event.matches);
      if (event.matches && focusInSidebar) setPendingFocusRestore(true);
    };
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  // Consume pending focus restoration after the commit that removed
  // `inert` from the header — focusing an inert element is a no-op.
  useEffect(() => {
    if (!pendingFocusRestore) return;
    toggleRef.current?.focus();
    setPendingFocusRestore(false);
  }, [pendingFocusRestore]);

  // Escape closes the modal drawer.
  useEffect(() => {
    if (!overlayActive) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setSidebarOpen(false);
        setPendingFocusRestore(true);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [overlayActive]);

  // Opening the modal drawer moves focus into it (first nav item).
  useEffect(() => {
    if (!overlayActive) return;
    sidebarRef.current?.querySelector<HTMLElement>(".nav-item")?.focus();
  }, [overlayActive]);

  // Focus the active view's heading after navigation (skip link target
  // stays available for explicit jumps).
  useEffect(() => {
    const heading = mainRef.current?.querySelector("h1");
    heading?.focus();
  }, [activeView]);

  const closeOverlayAndRestoreFocus = useCallback(() => {
    setSidebarOpen(false);
    setPendingFocusRestore(true);
  }, []);

  /**
   * Explicit Tab/Shift+Tab wrap at the drawer boundaries, overlay mode only.
   *
   * Chromium does not complete the focus wrap-around across `inert`
   * regions — Tab from the drawer's last element strands focus on
   * `<body>` (observed in a real browser). The drawer therefore owns its
   * own boundary wrap; interior Tab presses are left to the browser.
   */
  function handleDrawerKeyDown(event: ReactKeyboardEvent<HTMLElement>) {
    if (!overlayActive || event.key !== "Tab" || !sidebarRef.current) return;
    const tabbables = sidebarRef.current.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    if (tabbables.length === 0) return;
    const first = tabbables[0]!;
    const last = tabbables[tabbables.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function handleNavigate(view: ViewId) {
    onNavigate(view);
    if (isNarrow) setSidebarOpen(false);
  }

  function handleProjectSelect(event: ChangeEvent<HTMLSelectElement>) {
    if (event.target.value) {
      void workspaceStore.select(event.target.value);
    }
  }

  const workspaceReady = workspace.phase === "ready";
  const projects = workspace.data.projects;
  const selectedProject = projects.find(
    (p) => p.id === workspace.data.selectedProjectId,
  );

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content" inert={overlayActive ? true : undefined}>
        Skip to main content
      </a>

      <header className="app-header" inert={overlayActive ? true : undefined}>
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
              value={workspace.data.selectedProjectId ?? ""}
              onChange={handleProjectSelect}
              // Unavailable until the persisted workspace has loaded; busy
              // while a selection change is in flight.
              disabled={
                !workspaceReady ||
                workspace.pendingAction !== null ||
                projects.length === 0
              }
            >
              {!workspaceReady && <option value="">Loading…</option>}
              {workspaceReady && projects.length === 0 && (
                <option value="">No projects yet</option>
              )}
              {workspaceReady &&
                workspace.data.selectedProjectId === null &&
                projects.length > 0 && <option value="">Select a project…</option>}
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </label>
          <span className="phase-badge">Phase 2 · Persistence</span>
        </div>
      </header>

      <aside
        id="workspace-sidebar"
        ref={sidebarRef}
        className={`app-sidebar${isNarrow ? " overlay" : ""}`}
        hidden={!sidebarOpen}
        role={overlayActive ? "dialog" : undefined}
        aria-modal={overlayActive ? true : undefined}
        aria-label="Workspace navigation"
        onKeyDown={handleDrawerKeyDown}
      >
        {overlayActive && (
          <button
            type="button"
            className="sidebar-close"
            aria-label="Close navigation"
            onClick={closeOverlayAndRestoreFocus}
          >
            ✕
          </button>
        )}
        <SidebarNav activeView={activeView} onNavigate={handleNavigate} />
        <p className="sidebar-footnote">
          Projects and settings persist locally (SQLite, via the Rust core).
        </p>
      </aside>
      {overlayActive && (
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
        // Modal drawer: content is out of the tab order and a11y tree
        // while the overlay is open (React 19 boolean `inert`).
        inert={overlayActive ? true : undefined}
      >
        {children}
      </main>
    </div>
  );
}

export default AppShell;

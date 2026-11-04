import FoundationScreen from "../features/foundation/FoundationScreen";

/**
 * Application shell.
 *
 * In later phases this shell hosts workspace navigation and module views
 * (chat, agents, workflows). Phase 0 renders only the foundation screen.
 */
function App() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="app-brand">
          <span className="app-logo" aria-hidden="true">
            O
          </span>
          <span className="app-name">Orqen</span>
        </div>
        <span className="phase-badge">Phase 0 · Foundation</span>
      </header>
      <main className="app-main">
        <FoundationScreen />
      </main>
      <footer className="app-footer">
        <span>Orqen — local-first AI multi-agent development workspace.</span>
        <span>Current status: docs/STATUS.md · Plans: docs/ROADMAP.md</span>
      </footer>
    </div>
  );
}

export default App;

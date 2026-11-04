import { useEffect, useState } from "react";
import {
  echoPing,
  fetchAppInfo,
  normalizeCommandError,
} from "../../lib/ipc/appInfo";
import type { AppInfo, CommandErrorPayload } from "../../types/app";

/**
 * Phase 0 foundation screen.
 *
 * States shown here are real: they come from live IPC calls to the Rust
 * core. Nothing is simulated — if the backend is unavailable the screen
 * shows an error state.
 */
type RuntimeState =
  | { phase: "loading" }
  | { phase: "ready"; info: AppInfo; ipcRoundTrip: boolean }
  | { phase: "error"; error: CommandErrorPayload };

/** Capabilities actually implemented in this build. */
const IMPLEMENTED: readonly string[] = [
  "Desktop application shell (Tauri 2 + React + TypeScript)",
  "Rust core with structured IPC commands",
  "TypeScript strict mode, ESLint, Vitest quality gates",
  "Project documentation and governance set (docs/)",
];

/** Capabilities planned for later phases — not implemented, not simulated. */
const PLANNED: readonly string[] = [
  "Agent registry and provider adapters",
  "Chat, interview and orchestration",
  "Workflow execution and task chunking",
  "Quality verification and shared learning",
  "SQLite persistence, settings and remote access",
];

function FoundationScreen() {
  const [runtime, setRuntime] = useState<RuntimeState>({ phase: "loading" });

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const [info, echoed] = await Promise.all([
          fetchAppInfo(),
          echoPing("ping"),
        ]);
        if (!cancelled) {
          setRuntime({
            phase: "ready",
            info,
            ipcRoundTrip: echoed === "ping",
          });
        }
      } catch (error) {
        if (!cancelled) {
          setRuntime({ phase: "error", error: normalizeCommandError(error) });
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section aria-labelledby="foundation-heading">
      <div className="hero">
        <h1 id="foundation-heading">Orqen is initialized.</h1>
        <p>
          The desktop foundation is running. Agent capabilities arrive in
          later phases — this screen only reports what is real.
        </p>
      </div>

      <div className="runtime-card">
        <h2>Runtime status</h2>
        <ul className="runtime-list">
          <li>
            <span className="runtime-label">Application</span>
            <span
              className="runtime-value"
              data-testid="runtime-app"
            >
              {runtime.phase === "loading"
                ? "Loading…"
                : runtime.phase === "ready"
                  ? `${runtime.info.name} v${runtime.info.version}`
                  : "Unavailable"}
            </span>
          </li>
          <li>
            <span className="runtime-label">Platform</span>
            <span className="runtime-value" data-testid="runtime-platform">
              {runtime.phase === "ready" ? runtime.info.os : "—"}
            </span>
          </li>
          <li>
            <span className="runtime-label">Rust IPC</span>
            {runtime.phase === "loading" ? (
              <span className="status-loading">Checking…</span>
            ) : runtime.phase === "ready" && runtime.ipcRoundTrip ? (
              <span className="status-ok">Connected</span>
            ) : runtime.phase === "ready" ? (
              <span className="status-err">Round trip failed</span>
            ) : (
              <span className="status-err">Failed</span>
            )}
          </li>
        </ul>
        {runtime.phase === "error" && (
          <p className="runtime-error" role="alert">
            Could not load runtime information ({runtime.error.kind}):{" "}
            {runtime.error.message}
          </p>
        )}
      </div>

      <div className="panels">
        <section className="panel" aria-labelledby="implemented-heading">
          <h2 id="implemented-heading">Implemented in this build</h2>
          <ul>
            {IMPLEMENTED.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
          <p className="panel-note">Verified by the Phase 0 quality gates.</p>
        </section>

        <section className="panel planned" aria-labelledby="planned-heading">
          <h2 id="planned-heading">Planned — not implemented yet</h2>
          <ul>
            {PLANNED.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
          <p className="panel-note">
            See docs/ROADMAP.md for the phased delivery plan.
          </p>
        </section>
      </div>
    </section>
  );
}

export default FoundationScreen;

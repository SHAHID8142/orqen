import { useAgents, useAgentsStore } from "../../lib/agents/AgentsContext";
import { useSettings, useSettingsStore } from "../../lib/settings/SettingsContext";
import type { AgentRecord, RunOutcome } from "../../types/agents";
import "./Agents.css";

function StatusChip({ record, pending }: { record: AgentRecord; pending: boolean }) {
  if (pending) return <span className="chip chip-checking">Checking…</span>;
  switch (record.discovery.status) {
    case "not_checked":
      return <span className="chip">Not checked</span>;
    case "found":
      return <span className="chip chip-ok">Executable found</span>;
    case "not_found":
      return <span className="chip chip-miss">Not found</span>;
    case "error":
      return <span className="chip chip-miss">Error</span>;
  }
}

function AgentCard({
  record,
  pending,
  onDiscover,
  onVerify,
}: {
  record: AgentRecord;
  pending: boolean;
  onDiscover: (id: string) => void;
  onVerify: (id: string) => void;
}) {
  const verified = record.verification?.status === "verified" ? record.verification : null;
  const failed = record.verification?.status === "failed" ? record.verification : null;
  const path =
    record.discovery.status === "found" ? record.discovery.path : null;
  const errorMessage = record.discovery.status === "error" ? record.discovery.message : null;

  return (
    <article className="agent-card" aria-label={record.displayName}>
      <header className="agent-card-header">
        <div>
          <h3 className="agent-name">{record.displayName}</h3>
          <p className="agent-provider">
            {record.provider} · CLI adapter · <code>{record.executableName}</code>
          </p>
        </div>
        <StatusChip record={record} pending={pending} />
      </header>

      <dl className="agent-details">
        {path && (
          <div className="agent-detail">
            <dt>Executable</dt>
            <dd>
              <code>{path}</code>
            </dd>
          </div>
        )}
        {verified && (
          <div className="agent-detail">
            <dt>Verified version</dt>
            <dd>{verified.version}</dd>
          </div>
        )}
        {record.checkedAt && (
          <div className="agent-detail">
            <dt>Last checked</dt>
            <dd>{record.checkedAt.slice(0, 19).replace("T", " ")} UTC</dd>
          </div>
        )}
      </dl>

      {errorMessage && (
        <p className="agent-error" role="alert">
          {errorMessage}
        </p>
      )}
      {failed && (
        <p className="agent-error" role="alert">
          {failed.message}
        </p>
      )}

      <div className="agent-actions">
        <button
          type="button"
          disabled={pending}
          onClick={() => onDiscover(record.id)}
        >
          {pending ? "Checking…" : "Check availability"}
        </button>
        <button
          type="button"
          disabled={pending || record.discovery.status !== "found"}
          onClick={() => onVerify(record.id)}
        >
          Verify CLI
        </button>
      </div>
      <p className="agent-note">
        Checking only locates the executable — it says nothing about
        authentication or connectivity. Verify CLI runs a noninteractive{" "}
        <code>--version</code> probe.
      </p>
    </article>
  );
}

function RunOutcomeLabel({ outcome }: { outcome: RunOutcome }) {
  switch (outcome) {
    case "succeeded":
      return <span className="chip chip-ok">Succeeded</span>;
    case "failed":
      return <span className="chip chip-miss">Failed</span>;
    case "timed_out":
      return <span className="chip chip-miss">Timed out</span>;
    case "cancelled":
      return <span className="chip">Cancelled</span>;
  }
}

function AgentTestRunSection() {
  const agents = useAgents();
  const agentsStore = useAgentsStore();
  const settings = useSettings();
  const settingsStore = useSettingsStore();
  const run = agents.run;
  const settingsReady = settings.phase === "ready";

  // Eligible: registry-loaded, executable found, verified, AND this build
  // can run a safe diagnostic for the adapter. Disabled adapters must
  // not offer a run button — their reason is shown instead, from the
  // backend DTO (never invented in the UI).
  const eligible = agents.records.filter(
    (record) =>
      record.discovery.status === "found" &&
      record.verification?.status === "verified" &&
      record.capabilities.diagnosticExecution,
  );
  // Found + verified agents whose diagnostics this build does not run:
  // their backend-provided reason is displayed verbatim.
  const disabledButVerified = agents.records.filter(
    (record) =>
      record.discovery.status === "found" &&
      record.verification?.status === "verified" &&
      !record.capabilities.diagnosticExecution,
  );
  const preferred = settings.data.mainAgentId;

  return (
    <section className="test-run" aria-labelledby="test-run-heading">
      <h2 id="test-run-heading">Agent Test Run</h2>
      <div className="notice-card" role="note">
        <p>
          Runs a <strong>fixed diagnostic prompt</strong> in an isolated
          temporary directory. This may contact the provider&apos;s service
          and consume your existing account quota or credits. It never reads
          or modifies project files, and it is not an authorization for
          autonomous coding.
        </p>
      </div>

      {settings.phase === "loading" && (
        <p className="option-note">Loading preferences…</p>
      )}
      {settings.phase === "error" && (
        <div className="error-card" role="alert">
          <p>Preferences are unavailable — retry from the error message above.</p>
        <button type="button" onClick={() => void settingsStore.load()}>
          Retry
        </button>
        </div>
      )}

      {run.phase === "idle" && (
        <div className="test-run-idle">
          {disabledButVerified.length > 0 && (
            <ul className="test-run-disabled" data-testid="diagnostic-disabled-notes">
              {disabledButVerified.map((record) => (
                <li key={record.id} className="option-note">
                  <strong>{record.displayName}</strong>: {record.capabilities.diagnosticExecutionNote}
                </li>
              ))}
            </ul>
          )}
          {eligible.length === 0 ? (
            <p className="option-note">
              No eligible agent yet — an agent becomes eligible after its
              executable is found, its version is verified, and its
              adapter permits safe diagnostics in this build.
            </p>
          ) : (
            <ul className="test-run-agents">
              {eligible.map((record) => (
                <li key={record.id}>
                  <button
                    type="button"
                    disabled={!settingsReady || settings.saving}
                    onClick={() => agentsStore.requestTestRun(record.id)}
                  >
                    Run test with {record.displayName}
                    {preferred === record.id && (
                      <span className="option-note"> — preferred Main Agent</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {run.phase === "awaiting_confirmation" && (
        <div
          className="confirm-card"
          role="group"
          aria-label="Confirm diagnostic run"
          data-testid="run-confirmation"
        >
          <h3>Confirm diagnostic run</h3>
          <p>
            Agent:{" "}
            <strong>
              {agents.records.find((r) => r.id === run.agentId)?.displayName ??
                run.agentId}
            </strong>{" "}
            — this will contact the provider&apos;s service and may consume
            your account quota or credits.
          </p>
          <div className="confirm-actions">
            <button
              type="button"
              className="danger"
              autoFocus
              onClick={() => void agentsStore.confirmTestRun()}
            >
              Confirm & run
            </button>
            <button type="button" onClick={() => agentsStore.cancelRequest()}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {(run.phase === "starting" || run.phase === "running") && (
        <div className="running-card" aria-busy="true" data-testid="run-running">
          <p>
            {run.phase === "starting"
              ? "Starting diagnostic run…"
              : "Diagnostic running…"}{" "}
            {run.cancelRequested && <strong>Cancelling…</strong>}{" "}
            <span className="option-note">
              The prompt is fixed and runs in an isolated directory.
            </span>
          </p>
          <button
            type="button"
            disabled={run.cancelRequested}
            onClick={() => void agentsStore.cancelRun()}
          >
            Cancel run
          </button>
        </div>
      )}

      {(run.phase === "succeeded" ||
        run.phase === "failed" ||
        run.phase === "timed_out" ||
        run.phase === "cancelled") && (
        <div className="result-card" data-testid="run-result" role="status">
          <h3>
            Diagnostic result <RunOutcomeLabel outcome={run.phase} />
          </h3>
          {run.result && (
            <dl className="agent-details">
              <div className="agent-detail">
                <dt>Agent</dt>
                <dd>{run.result.agentId}</dd>
              </div>
              <div className="agent-detail">
                <dt>Duration</dt>
                <dd>{(run.result.durationMs / 1000).toFixed(1)}s</dd>
              </div>
              {run.result.exitCode !== null && (
                <div className="agent-detail">
                  <dt>Exit code</dt>
                  <dd>{run.result.exitCode}</dd>
                </div>
              )}
            </dl>
          )}
          {run.error && (
            <p className="agent-error" role="alert">
              {run.error.message}
            </p>
          )}
          {run.result?.output && (
            <>
              <h4 className="output-heading">Agent output</h4>
              <pre className="agent-output">{run.result.output}</pre>
            </>
          )}
          <div className="confirm-actions">
            <button type="button" onClick={() => agentsStore.dismissRun()}>
              Dismiss
            </button>
          </div>
          <p className="agent-note">
            A successful diagnostic confirms availability only — it is not an
            authorization for autonomous coding.
          </p>
        </div>
      )}
    </section>
  );
}

function Agents() {
  const agents = useAgents();
  const agentsStore = useAgentsStore();
  const settings = useSettings();
  const settingsStore = useSettingsStore();

  const settingsReady = settings.phase === "ready";
  const settingsError = settings.phase === "error";
  const pendingId =
    agents.pendingAction?.kind === "discover" || agents.pendingAction?.kind === "verify"
      ? agents.pendingAction.id
      : null;

  function handleDiscover(id: string) {
    void agentsStore.discover(id);
  }
  function handleVerify(id: string) {
    void agentsStore.verify(id);
  }
  function handleMainAgent(agentId: string | null) {
    void settingsStore.setMainAgent(agentId);
  }

  if (agents.phase === "loading") {
    return (
      <section aria-labelledby="agents-heading" aria-busy="true" className="agents">
        <h1 id="agents-heading" tabIndex={-1}>
          Agents
        </h1>
        <p className="surface-lead" data-testid="agents-loading">
          Loading agent registry…
        </p>
      </section>
    );
  }

  if (agents.phase === "error") {
    return (
      <section aria-labelledby="agents-heading" className="agents">
        <h1 id="agents-heading" tabIndex={-1}>
          Agents
        </h1>
        <div className="error-card" role="alert" data-testid="agents-error">
          <h2>Could not load the agent registry</h2>
          <p>
            {agents.loadError?.message ??
              "The local database is unavailable. Your data is untouched."}
          </p>
          <button type="button" onClick={() => void agentsStore.load()}>
            Retry
          </button>
        </div>
      </section>
    );
  }

  return (
    <section aria-labelledby="agents-heading" className="agents">
      <h1 id="agents-heading" tabIndex={-1}>
        Agents
      </h1>
      <p className="surface-lead">
        Registered AI coding CLIs on this computer. Checking availability
        never launches a coding task.
      </p>

      <div className="notice-card" role="note">
        <p>
          Agent execution and orchestration will be available in a later
          feature. Today Orqen can discover these CLIs, verify their
          version, and remember your preferred Main Agent.
        </p>
      </div>

      {agents.actionError && (
        <p className="field-error" role="alert" data-testid="agents-action-error">
          {agents.actionError.message}
        </p>
      )}

      <div className="agent-list">
        {agents.records.map((record) => (
          <AgentCard
            key={record.id}
            record={record}
            pending={pendingId === record.id}
            onDiscover={handleDiscover}
            onVerify={handleVerify}
          />
        ))}
      </div>

      <AgentTestRunSection />

      {/* Settings failure recovery lives OUTSIDE the disabled fieldset —
          a Retry control must stay usable exactly when the preferences are
          unavailable. */}
      {settingsError && (
        <div
          className="error-card"
          role="alert"
          data-testid="main-agent-error"
          aria-live="polite"
        >
          <h2>Could not load preferences</h2>
          <p>
            {settings.loadError?.message ??
              "Settings are unavailable. Your preference is untouched."}
          </p>
          <button type="button" onClick={() => void settingsStore.load()}>
            Retry
          </button>
        </div>
      )}

      <fieldset
        className="settings-group"
        disabled={!settingsReady || settings.saving}
        aria-busy={settings.saving}
      >
        <legend>Preferred Main Agent</legend>
        {settings.phase === "loading" && (
          <p className="option-note" data-testid="main-agent-loading">
            Loading preferences…
          </p>
        )}
        {settings.actionError && (
          <p className="field-error" role="alert" data-testid="main-agent-action-error">
            {settings.actionError.message}
          </p>
        )}
        <p className="option-note">
          A saved preference for a later feature — selecting an agent starts
          nothing today.
        </p>
        <div className="radio-option">
          <input
            id="main-agent-none"
            type="radio"
            name="main-agent"
            checked={settings.data.mainAgentId === null}
            onChange={() => handleMainAgent(null)}
          />
          <label htmlFor="main-agent-none">
            None <span className="option-note">— no preferred Main Agent</span>
          </label>
        </div>
        {agents.records.map((record) => {
          const unavailable =
            record.discovery.status === "not_found" || record.discovery.status === "error";
          return (
            <div className="radio-option" key={record.id}>
              <input
                id={`main-agent-${record.id}`}
                type="radio"
                name="main-agent"
                checked={settings.data.mainAgentId === record.id}
                onChange={() => handleMainAgent(record.id)}
              />
              <label htmlFor={`main-agent-${record.id}`}>
                {record.displayName}
                {unavailable && settings.data.mainAgentId === record.id && (
                  <span className="option-note warn-note">
                    {" "}
                    — executable currently unavailable; preference preserved
                  </span>
                )}
              </label>
            </div>
          );
        })}
      </fieldset>
    </section>
  );
}

export default Agents;

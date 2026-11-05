import { useAgents, useAgentsStore } from "../../lib/agents/AgentsContext";
import { useSettings, useSettingsStore } from "../../lib/settings/SettingsContext";
import type { AgentRecord } from "../../types/agents";
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

function Agents() {
  const agents = useAgents();
  const agentsStore = useAgentsStore();
  const settings = useSettings();
  const settingsStore = useSettingsStore();

  const ready = agents.phase === "ready";
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

      <fieldset className="settings-group" disabled={!ready || settings.saving}>
        <legend>Preferred Main Agent</legend>
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

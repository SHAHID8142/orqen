/**
 * Agents surface.
 *
 * Agent integrations are NOT installed yet. This surface explains the
 * plan only — it lists no agents and reports no statuses, because none
 * exist (see docs/ROADMAP.md Phase 3).
 */

const PLANNED_CAPABILITIES: readonly string[] = [
  "Agent registry with capability metadata",
  "Provider-independent adapters (e.g. Codex, Claude Code) running as isolated processes",
  "Appointing a Main Orchestrator from supported agents",
  "Task assignment, proposals, discussion, voting and verdict recording",
  "Performance evaluation and failure tracking",
];

function Agents() {
  return (
    <section aria-labelledby="agents-heading" className="agents">
      <h1 id="agents-heading" tabIndex={-1}>
        Agents
      </h1>
      <div className="notice-card" data-testid="agents-not-installed">
        <h2>Agent integrations are not installed yet</h2>
        <p>
          Orqen is provider-independent by design, but no agent is connected in
          this build. There is nothing to configure or appoint here yet — this
          surface will become active in Phase 3.
        </p>
      </div>

      <div className="panel">
        <h2>Planned capabilities</h2>
        <ul>
          {PLANNED_CAPABILITIES.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        <p className="panel-note">
          Scope and acceptance criteria: docs/ROADMAP.md · Governance rules:
          docs/GOVERNANCE.md
        </p>
      </div>
    </section>
  );
}

export default Agents;

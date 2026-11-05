import "./Workflows.css";

/**
 * Workflows surface.
 *
 * The workflow engine is NOT implemented yet (docs/ROADMAP.md Phase 7).
 * This surface documents the planned pipeline — it runs nothing and
 * reports no execution state.
 */

const PIPELINE_STAGES: readonly string[] = [
  "Task Intake",
  "Research",
  "Discussion",
  "Planning",
  "Chunking",
  "Agent Assignment",
  "Execution",
  "Audit",
  "Verification",
  "Try to Break",
  "Fix",
  "Secure",
  "Shippable",
  "Approval",
  "Next Chunk",
];

function Workflows() {
  return (
    <section aria-labelledby="workflows-heading" className="workflows">
      <h1 id="workflows-heading" tabIndex={-1}>
        Workflows
      </h1>
      <div className="notice-card" data-testid="workflows-not-implemented">
        <h2>Workflow execution is not implemented yet</h2>
        <p>
          The Universal Agent Workflow below is the planned pipeline for every
          future feature. No stage is executable in this build — this surface
          documents the model only.
        </p>
      </div>

      <div className="panel">
        <h2>Planned pipeline (docs/WORKFLOW.md)</h2>
        <ol className="pipeline">
          {PIPELINE_STAGES.map((stage, index) => (
            <li key={stage}>
              <span className="stage" aria-hidden="true">
                {String(index + 1).padStart(2, "0")}
              </span>
              <span>{stage}</span>
            </li>
          ))}
        </ol>
        <p className="panel-note">
          Stage rules and overrides: docs/WORKFLOW.md · Execution controls:
          docs/GOVERNANCE.md
        </p>
      </div>
    </section>
  );
}

export default Workflows;

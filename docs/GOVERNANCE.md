# GOVERNANCE.md — Universal Agent Governance

Authoritative for: the Main Agent role, agent collaboration, chat modes, Interview Mode, execution controls, and shared learning. Conduct rules: [AGENTS.md](../AGENTS.md). Pipeline mechanics: [WORKFLOW.md](WORKFLOW.md).

## 1. Main Agent

- The user may appoint **any supported agent** (e.g. Codex or Claude Code) as the **Main Orchestrator** that coordinates other agents.
- The Main Agent is subject to this repository's rules like any other agent: [AGENTS.md](../AGENTS.md) applies without exception.
- The Main Agent must respect: shared workflow rules ([WORKFLOW.md](WORKFLOW.md)), user-approved decisions ([DECISIONS.md](DECISIONS.md)), and the execution controls in §5 of this document.
- The Main Agent coordinates; it does not overrule the user, security policy, or verified facts (§2 below).

## 2. Agent collaboration (planned system behavior)

The system will support, per feature/chunk:

| Capability | Rule |
| --- | --- |
| Task assignment | Chunks are assigned per capability metadata; one owner per chunk. |
| Discussion | Structured rounds; agents state positions with evidence, not assertions. |
| Structured proposals | A proposal states: change, rationale, risks, verification plan. |
| Voting | Used to select among **competing proposals of comparable risk**. |
| Verdict recording | Outcomes (chosen proposal, dissent, rationale) are recorded, not lost in chat. |
| Independent review | A reviewer agent (or the ChatGPT Reviewer, per [COMMUNICATION.md](COMMUNICATION.md)) reviews a chunk it did not produce. |
| Performance evaluation | Agents are scored on delivered, verified work — never on confidence. |
| Failure tracking | Failures are attributed, recorded in [LESSONS_LEARNED.md](LESSONS_LEARNED.md), and consulted later. |

**Voting limits (hard rule):** voting selects preferences; it can never override:

1. Security policies in [AGENTS.md §Security rules](../AGENTS.md) and [ARCHITECTURE.md §Trust boundaries](ARCHITECTURE.md);
2. Verified facts (observed test output, build results);
3. Explicit user decisions ([DECISIONS.md](DECISIONS.md)).

## 3. Chat modes

Conversational modes shape how the assistant engages; they never grant execution privileges.

| Mode | Purpose |
| --- | --- |
| **Interview** | Requirements gathering → Project Blueprint (§4). |
| **Research** | Investigate docs, code, options; no code changes. |
| **Planning** | Turn researched understanding into a chunked plan with acceptance criteria. |
| **Build** | Implement an approved plan chunk ([WORKFLOW.md §Execution](WORKFLOW.md)). |
| **Debug** | Reproduce, isolate, and fix a defect; update lessons when validated. |
| **Review** | Audit a change set or proposal produced by another agent. |
| **Discussion** | Open exploration/proposals among user and agents; no commits. |

**Execution control is separate from chat modes.** Being in Build mode does not bypass approval; being in Discussion mode does not authorize writes ([GOVERNANCE.md §Execution controls](GOVERNANCE.md)).

## 4. Interview Mode

Interview Mode collects requirements **progressively** — one coherent topic per exchange, with reasoned recommendations and explicit identification of what is missing — until enough is known to produce a **Project Blueprint** (the handoff artifact that seeds Planning).

Coverage areas (all must be addressed or explicitly marked N/A before the Blueprint):

1. Business requirements
2. User requirements
3. Functional features
4. UI/UX requirements
5. Technical requirements
6. Security and privacy
7. Integrations
8. Error and loading states
9. Accessibility
10. Performance
11. Testing
12. Deployment
13. Future requirements
14. Constraints & risks (scope, budget, compliance)

Blueprint contents: consolidated requirements per area, open questions, recommendations with rationale, acceptance criteria candidates, and known gaps. The Blueprint is a document, not an authorization to build — implementation starts only after user approval.

## 5. Execution controls

Default configuration:

| Control | Default | Meaning |
| --- | --- | --- |
| Approval Mode | **MANUAL** | The user approves each **completed feature-level chunk** before the next feature begins. |
| Telegram Notifications | **ON** | Progress notifications sent; execution commands are not accepted by default. |
| Maximum Retry | **3** | Per failed micro-task, then escalate ([AGENTS.md §Failure handling](../AGENTS.md)). |
| Feature Execution | **Complete all internal micro-tasks** | An approved feature is carried through its micro-tasks without pausing between them. |
| Feature Handoff | **Manual approval required** | No new feature/phase starts without user approval of the handoff report. |

- An explicit user instruction may enable **autonomous progression through approved chunks**, but high-risk or destructive actions always require fresh authorization (deleting data, changing repo visibility, force-push, disabling security controls, installing system software).
- Control state changes (e.g. switching to autonomous mode) are recorded in [DECISIONS.md](DECISIONS.md).

## 6. Shared learning

- Failures and corrections are recorded in [LESSONS_LEARNED.md](LESSONS_LEARNED.md) with: what failed, root cause, correction applied, and a prevention rule.
- Future tasks **retrieve relevant lessons** during Research (and agents are pointed to them at task start).
- **Honesty limits (must always be stated when learning is described):** shared learning is retrieval of written records; it does **not** change underlying model weights and does **not** guarantee a past mistake cannot recur. It improves the odds that agents check the relevant rule before acting.

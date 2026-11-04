# WORKFLOW.md — Universal Agent Workflow

Authoritative for: how every feature moves from idea to approved, shippable work. Conduct rules: [AGENTS.md](../AGENTS.md). Mode/control separation: [GOVERNANCE.md](GOVERNANCE.md).

## Pipeline

```
Task Intake → Research → Discussion → Planning → Chunking → Agent Assignment
     → Execution → Audit → Verification → Try to Break → Fix
     → Secure → Shippable → Approval → Next Chunk
```

## Stage definitions

| Stage | What happens | Output | Gate to proceed |
| --- | --- | --- | --- |
| **Task Intake** | A feature/task is captured with intent, scope boundaries, and links to prior decisions. | Intake note in STATUS.md / task record. | User or approved plan confirms scope. |
| **Research** | Consult official docs; check tool compatibility; identify reusable decisions and lessons. | Research summary; dependency compatibility notes. | Unknowns either resolved or converted into explicit questions. |
| **Discussion** | (Multi-agent mode) involved agents raise concerns and structured proposals; recording per [GOVERNANCE.md §Agent collaboration](GOVERNANCE.md). | Proposals + recorded verdicts. | Concerns resolved or explicitly deferred by the user. |
| **Planning** | Define exact scope, dependencies, risks; break the feature into small micro-tasks; write **acceptance criteria before implementation**. | Micro-task list with criteria. | User approves the plan (or pre-approved plan scope applies). |
| **Chunking** | Split into independently verifiable chunks, each small enough for one focused change set. | Ordered chunk list. | Each chunk has criteria and no circular dependencies. |
| **Agent Assignment** | Chunks assigned to agents (single-agent mode: self). | Assignment record. | Assignment respects capability metadata. |
| **Execution** | Implement one micro-task/chunk at a time; focused changes; no speculative code. | Working code. | No unrelated files touched. |
| **Audit** | Review the change set: correctness, standards fit ([AGENTS.md §Coding standards](../AGENTS.md)), scope check. | Audit findings. | Findings addressed or consciously waived. |
| **Verification** | Run the gates for real: `npm run verify` + rust gates (+ tests). Record actual results. | QA evidence in [QA_REPORT.md](QA_REPORT.md). | All gates pass on observed output. |
| **Try to Break** | Adversarial pass against the acceptance criteria: invalid inputs, failure paths, integration seams. | Break-attempts list + outcomes. | Every discovered defect is either fixed or filed with severity. |
| **Fix** | Correct defects found by audit/verification/break pass; re-run the full gate loop. | Updated code + evidence. | Maximum 3 retries per micro-task, then escalate ([AGENTS.md §Failure handling](../AGENTS.md)). |
| **Secure** | Security review: secrets scan, least-privilege check, dependency audit, diff inspection before commit. | Security notes. | `git diff --cached` shows no secrets or stray files. |
| **Shippable** | Chunk is declared shippable: docs updated (STATUS, QA_REPORT, affected docs), commit prepared. | Commit + pushed state. | CI green when applicable. |
| **Approval** | The user reviews the handoff report and approves. | Recorded approval. | [GOVERNANCE.md §Execution controls](GOVERNANCE.md) defaults to MANUAL. |
| **Next Chunk** | Proceed to the next chunk (manual mode: after approval; autonomous mode: per explicit instruction, destructive actions still gated). | — | — |

## Rules that override the pipeline

1. **Voting does not override security policies, verified facts, or explicit user decisions** ([GOVERNANCE.md §Agent collaboration](GOVERNANCE.md)).
2. A failed stage loops back to its own stage — never skips forward.
3. High-risk or destructive actions (deleting data, changing visibility, force-push, disabling security controls) require explicit user authorization regardless of mode.
4. Never claim a stage complete without its observed output artifact.

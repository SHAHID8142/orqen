# COMMUNICATION.md — Communication Protocol

Authoritative for: who communicates with whom, in what structure, and where outcomes are recorded. Participants: **User**, **Zcode Agent** (this implementation agent), **Main Orchestrator** (future appointed agent), **Sub-Agents** (future), **ChatGPT Reviewer** (external review), **GitHub** (remote + CI).

## 1. General rules

- Every structured message has: subject, scope, and an explicit expected response (`decision required` / `informational`).
- Records beat memory: outcomes go into the docs named in §5, never only into chat.
- Honesty rule: a report states only what was observed, with the evidence attached.

## 2. Message types

### 2.1 Structured Question (agent → user)

Used when a decision or missing information blocks progress.

```
Q-ID: Q-<n> | Phase: <phase> | Scope: <feature/chunk>
Question: <one sentence>
Options:
  A) <option> — <implication>
  B) <option> — <implication>
Recommendation: <option> — <one-sentence rationale>
Response needed: <decision | information>
```

### 2.2 Progress Report (agent → user)

Issued during a feature at meaningful checkpoints (not between every micro-task).

```
Feature: <name> | Phase: <phase>
Completed: <micro-tasks done>
In progress: <current>
Next: <upcoming>
Blockers: <none | list>
Status updated: docs/STATUS.md
```

### 2.3 Completion Report (agent → user, end of feature/phase)

Mandatory format (used verbatim at the end of a feature):

```
PHASE: <phase>
STATUS: PASS | PARTIAL | BLOCKED
REPOSITORY: <url>
BRANCH: <branch>
COMMIT: <sha>
COMPLETED: <micro-tasks completed>
VERIFICATION: <commands executed + real results>
SECURITY: <checks performed + findings>
BLOCKERS: <none | list>
NEXT ACTION: <request for manual review or next approved step>
```

### 2.4 Decision Record (any participant → DECISIONS.md)

```
D-<n> | Date | Status: APPROVED | SUPERSEDED by D-<m>
Context: <why a choice was needed>
Decision: <what was decided>
Consequences: <trade-offs accepted>
```

### 2.5 Review Handoff (agent → ChatGPT Reviewer / reviewer agent)

```
Handoff: <feature/chunk>
Commit/branch: <ref>
Scope of review: <files or behavior>
Acceptance criteria: <list>
Verification evidence: <QA_REPORT.md section>
Questions for reviewer: <optional>
```

Reviewer responses classify findings as `[BLOCKER] [MAJOR] [MINOR] [NOTE]` and are answered in writing; blockers must be fixed and re-verified before approval.

### 2.6 Agent Proposal / Verdict (future, multi-agent)

Structured per [GOVERNANCE.md §Agent collaboration](GOVERNANCE.md): proposal (change, rationale, risks, verification plan) → votes → recorded verdict. Verdicts are recorded even when the user overrules the vote.

## 3. Interaction map

```
User ⇄ Zcode Agent            questions, reports, approvals (this file §2.1–2.3)
User ⇄ Main Orchestrator      task intake, decisions, handoffs (future)
Main Orchestrator ⇄ Sub-Agents assignments, proposals, verdicts (future)
Zcode Agent → ChatGPT Reviewer review handoffs (§2.5)
Zcode Agent ⇄ GitHub          commits, pushes, CI status (verified on GitHub)
All agents → docs/            records (DECISIONS, LESSONS_LEARNED, STATUS, QA_REPORT)
```

## 4. Escalation rules

1. Blocking ambiguity → Structured Question (§2.1), stop the affected line of work.
2. Repeated failure (3 retries exhausted) → blocker report in [STATUS.md](STATUS.md), no false success claims.
3. Security-relevant discovery (secret exposure, capability misuse, suspicious dependency) → stop, report immediately, propose containment before continuing.
4. Destructive/high-risk action → explicit user authorization first, always.

## 5. Where outcomes live

| Outcome | Recorded in |
| --- | --- |
| Approvals of choices | [DECISIONS.md](DECISIONS.md) |
| Validated failures + prevention | [LESSONS_LEARNED.md](LESSONS_LEARNED.md) |
| Phase/feature progress | [STATUS.md](STATUS.md) |
| Verification evidence | [QA_REPORT.md](QA_REPORT.md) |
| Reviewer findings | QA_REPORT.md entry + commit fixing blockers |
| CI status | GitHub Actions runs (checked, never assumed) |

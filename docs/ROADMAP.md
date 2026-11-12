# ROADMAP.md — Phased Development Roadmap

Priorities: **P0** = must exist for the product to function; **P1** = core value; **P2** = important; **P3** = polish. Each phase lists dependencies and acceptance criteria. Status column is updated in [STATUS.md](STATUS.md) as work completes — this file records the plan, not progress claims.

## Phase 0 — Project Foundation & Bootstrap
**Status: delivered** | Priority P0 | Dependencies: none

Private GitHub repository; runnable Tauri 2 + React + TypeScript desktop shell; modular structure; full documentation set; quality gates (ESLint, TS strict, Vitest, cargo fmt/clippy); CI; verified foundation screen with real IPC.

**Acceptance criteria:**
- [x] Repository `SHAHID8142/orqen` (created private; made public 2026-11-05 by explicit user instruction — D-010/D-011) with clean history on `main`.
- [x] App launches on macOS; React interface renders; no simulated statuses.
- [x] `npm run verify` (lint, typecheck, tests, build) passes; rust gates pass.
- [x] Documentation set complete and internally consistent.
- [~] CI configured and validated (`actionlint` clean; GitHub lists it active; jobs instantiate) — **execution BLOCKED — EXTERNAL** by an account-level GitHub billing restriction; CI has never reported a successful run. Local gates cover the same checks (see [QA_REPORT.md](QA_REPORT.md)).
- [x] No secrets committed.

## Phase 1 — Workspace Shell & Navigation
**Status: delivered (Feature 001, 2026-11-05)** | Priority P0 | Dependencies: Phase 0

Multi-module shell (state-based navigation per [ARCHITECTURE.md §Module boundaries](ARCHITECTURE.md), D-013), in-memory workspace/project concept, accessibility baseline (keyboard navigation, ARIA labels, focus states), settings page storing values in memory.

**Acceptance criteria:**
- [x] Navigate between the 5 module surfaces without full reloads (state-based; asserted by tests).
- [x] No lint/type regressions (all gates pass — docs/QA_REPORT.md Phase 1 section).
- [x] Keyboard traversal verified (buttons keyboard-activatable, Escape closes overlay sidebar, focus moves to view heading, skip link; automated tests).
- [x] In-memory project creation/selection/deletion with validation and explicit delete confirmation; empty states.
- [x] Settings controls functional in memory with truthful "preference only" labeling.
- [~] Per-project isolated context: project record + selection exist; context isolation arrives with persistence (Phase 2).
- [x] Navigation state survives app restart **not** required (in-memory by design, documented).

## Phase 2 — Local Persistence (SQLite) & Settings
**Status: delivered (Feature 001, 2026-11-05)** | Priority P0 | Dependencies: Phase 1

SQLite store in the Rust core ([ARCHITECTURE.md §Persistence architecture](ARCHITECTURE.md), D-014), transactional migrations, settings persistence, project/selection records, failure-safe initialization.

**Acceptance criteria:**
- [x] Data survives app restart (projects, selection, settings — Rust reopen tests + real-launch restart probe with an externally seeded row).
- [x] DB file lives in the app data dir, gitignored; never in the repository.
- [x] All access flows through validated Rust commands (parameterized SQL; WebView never touches the DB).
- [x] Schema migrations: versioned (`PRAGMA user_version`), transactional, idempotent, rollback tested.
- [x] Failure handling: corrupt/unreadable databases surface errors with recovery guidance and are never auto-reset; failed init keeps a retry path (no in-memory fallback).

## Phase 3 — Agent Registry & Adapters
Priority P1 | Dependencies: Phase 2

Agent registry with capability metadata; adapter interface (provider-independent); first two adapters (e.g. Codex, Claude Code) running agents as isolated least-privilege child processes; credential storage via OS keychain; agent health/status view backed by real probes only.

**Acceptance criteria:** run a trivial prompt through two different agents without core code changes; credentials never stored in repo/DB plaintext; process sandboxing documented; adapter conformance tests.

## Phase 4 — Chat Infrastructure & Modes
**Status: delivered (Feature 001, 2026-11-13)** | Priority P1 | Dependencies: Phase 3

Mode-aware chat surface (Interview, Research, Planning, Build, Debug, Review, Discussion per [GOVERNANCE.md §Chat modes](GOVERNANCE.md)); conversation persistence per project; message export.

**Acceptance criteria:**
- [x] Mode switch changes interaction framing only — never execution permissions (modes are a backend catalog with GOVERNANCE §3 purposes; switching updates the conversation record, nothing else).
- [x] History survives restart (conversations + messages in SQLite, schema v3, migration + cascade tests).
- [x] Export matches [COMMUNICATION.md](COMMUNICATION.md) conventions (structured content preserved verbatim; header metadata + honesty footer; D-026).

## Phase 5 — Interview Mode & Project Blueprint
Priority P1 | Dependencies: Phase 4

Progressive requirements interview covering the 14 areas in [GOVERNANCE.md §Interview mode](GOVERNANCE.md); missing-requirements detection; reasoned recommendations; Blueprint generation and storage.

**Acceptance criteria:** a sample project interview covers all areas or marks N/A; produced Blueprint contains requirements, open questions, recommendations, gaps; user approval gates the handoff.

## Phase 6 — Orchestration & Multi-Agent Coordination
Priority P1 | Dependencies: Phase 3, Phase 4

Main Orchestrator appointment; agent task assignment; structured proposals; discussion rounds; voting with the hard limits in [GOVERNANCE.md §Agent collaboration](GOVERNANCE.md); verdict recording; independent review; performance evaluation and failure tracking.

**Acceptance criteria:** orchestration of ≥2 agents on one chunk recorded end-to-end; votes never override security policy/verified facts/user decisions (enforced + tested); verdicts retrievable per chunk.

## Phase 7 — Workflow Execution & Task Chunking
Priority P1 | Dependencies: Phase 6

Execution engine implementing the [WORKFLOW.md](WORKFLOW.md) pipeline stages with recorded stage outputs; chunk dependencies; execution controls state machine (MANUAL default, retry cap 3, escalation) per [GOVERNANCE.md §Execution controls](GOVERNANCE.md).

**Acceptance criteria:** every chunk has recorded audit/verification/break-pass outcomes; MANUAL approval blocks the next chunk until approved; autonomous mode only via explicit user instruction; destructive actions always gated.

## Phase 8 — Quality Verification & Shared Learning
Priority P2 | Dependencies: Phase 7

Automated gate runner per chunk; adversarial "try to break" harness; lessons-learned store with retrieval during Research ([GOVERNANCE.md §Shared learning](GOVERNANCE.md)); failure analytics.

**Acceptance criteria:** a deliberately injected defect is caught by the break pass; recorded lessons are surfaced in a subsequent task's research step; honesty caveat about learning limits displayed where learning is described.

## Phase 9 — Skills & MCP Integration
Priority P2 | Dependencies: Phase 3

Skill/MCP server integration, added only by explicit user action, least-privilege scoped, revocable.

**Acceptance criteria:** adding an MCP server requires explicit consent; scopes visible in UI; removal revokes access; unauthorized capability requests are rejected and logged.

## Phase 10 — Telegram Remote Interface
Priority P2 | Dependencies: Phase 7

Notifications (default ON) and approval round-trips via Telegram bot; command execution OFF by default.

**Acceptance criteria:** remote approval maps onto the same execution-control state machine; high-risk actions still require in-app/explicit authorization; secrets (bot token) never enter the repo.

## Phase 11 — Hardening, Packaging & Release
Priority P2 | Dependencies: Phase 8 (Phases 9–10 may land in parallel)

Windows CI job; packaging/installers for macOS + Windows; performance pass; security audit; documentation freeze; beta → release.

**Acceptance criteria:** signed/notarized builds or documented limitations; CI green on macOS + Windows + Linux gates; release notes generated from QA reports; no open P0/P1 issues.

## Dependency graph (summary)

```
P0 ─ P1(shell) ─ P2(sqlite) ─ P3(agents) ─ P4(chat) ─ P5(interview)
                              └─ P6(orchestration) ─ P7(workflow) ─ P8(quality) ─ P11(release)
                              └─ P9(mcp) ────────────────────┘
                                            P7 ─ P10(telegram) ─┘
```

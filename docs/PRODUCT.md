# PRODUCT.md — Product Vision & Functional Requirements

Status legend used throughout: **[P0 IMPLEMENTED]** = shipped and verified in Phase 0. **[P2 IMPLEMENTED]** = shipped and verified in Phase 2. **[PLANNED]** = documented intent, not implemented. Nothing may be presented in the UI as functional while marked [PLANNED].

## 1. Vision

Orqen is a **local-first desktop workspace for managing multiple AI coding agents**. The user stays in control: they appoint any supported agent as the Main Orchestrator, converse in purpose-specific chat modes, and approve what gets built. Orqen coordinates the agents, breaks approved features into small verifiable chunks, runs quality loops (audit → verify → try to break → fix → secure), and learns from failures across tasks.

Design principles:

1. **Provider-independent** — no hard dependency on one AI model, vendor, or coding agent. Agents are pluggable adapters behind a registry.
2. **Local-first** — the workspace, project data, and lessons live on the user's machine (SQLite in a later phase). Remote access (Telegram) is an explicit, opt-in extension.
3. **Human in command** — execution control is separate from conversation. Agents propose; the user (or configured policy) approves; high-risk and destructive actions always require explicit authorization.
4. **Truthful by construction** — the system never presents simulated status as real. Governance: [GOVERNANCE.md](GOVERNANCE.md).

## 2. Target users and platforms

- Primary user: a developer managing AI coding agents for real software projects.
- Platforms: macOS and Windows desktop application; Telegram as the future remote interface.

## 3. Functional requirements by module

### 3.1 Application shell & navigation [PLANNED]
- Workspace-level navigation between modules (Chat, Interview, Agents, Workflows, Tasks, Verification, Knowledge, Settings).
- Per-project workspaces with isolated context and data.
- Responsive light-themed UI; accessibility: keyboard navigable, screen-reader labeled, focus visible.

### 3.2 Chat & interview [PLANNED]
- Mode-aware chat surface (see [GOVERNANCE.md §Chat modes](GOVERNANCE.md)): Interview, Research, Planning, Build, Debug, Review, Discussion.
- **Interview Mode** progressively gathers requirements across the fourteen coverage areas defined in [GOVERNANCE.md §Interview mode](GOVERNANCE.md), offers reasoned recommendations, flags missing requirements, and produces a **Project Blueprint** document that seeds planning.
- Conversation history retained per project; exportable as Markdown.

### 3.3 Agent registry & adapters [PLANNED]
- Registry of supported agents (e.g. Codex, Claude Code, and others) with capability metadata.
- Adapter interface so new providers/agents require no core changes. Adapters run agents as isolated, least-privilege processes.
- Agent identity, credentials, and model selection are user-managed in Settings; credentials never enter the repository.

### 3.3a Diagnostic execution [P3 IMPLEMENTED — registry slice]
- A controlled diagnostic test run per eligible agent: fixed noninteractive prompt, isolated working directory, explicit user confirmation, truthful result rendering (status, duration, exit code, output), cancellation (including while the run is still starting), and timeout.
- Authentication is the CLI's own; the run may contact the provider's service and consume the user's quota — communicated in the UI before confirmation.
- Availability is per adapter and honest: Codex runs with its documented read-only sandbox; Claude Code diagnostic execution is currently **disabled** in the app because its isolation cannot be verified across CLI versions (see [ARCHITECTURE.md §Trust boundaries](ARCHITECTURE.md)).
- This is the only executed agent interaction so far: no conversations, no coding tasks, no delegation.

### 3.4 Orchestration [PLANNED]
- The user appoints a supported agent as **Main Orchestrator**. The orchestrator coordinates sub-agents under the shared rules in [GOVERNANCE.md §Main agent](GOVERNANCE.md).
- Agent task assignment, structured proposals, discussion rounds, voting, verdict recording, and independent review — with the voting limits in [GOVERNANCE.md §Agent collaboration](GOVERNANCE.md).
- Performance evaluation and failure tracking per agent.

### 3.5 Workflow execution [PLANNED]
- The Universal Agent Workflow ([WORKFLOW.md](WORKFLOW.md)) drives every feature: Task Intake → Research → Discussion → Planning → Chunking → Agent Assignment → Execution → Audit → Verification → Try to Break → Fix → Secure → Shippable → Approval → Next Chunk.
- Features are divided into small chunks with acceptance criteria; each chunk runs through execution controls ([GOVERNANCE.md §Execution controls](GOVERNANCE.md)).

### 3.6 Task & chunk management [PLANNED]
- Persistent task/chunk records: status, assignment, acceptance criteria, audit results, verdicts.
- Dependency tracking between chunks; blocked states surfaced in the UI.

### 3.7 Quality verification [PLANNED]
- Automated gate runner (lint/typecheck/tests/build) per chunk, mirroring the repository's own gates.
- Independent review step (agent or ChatGPT Reviewer) before a chunk is marked shippable.
- "Try to break" pass: adversarial checks against the chunk's acceptance criteria.

### 3.8 Shared knowledge & memory [PLANNED]
- Lessons-learned store (validated failures, root causes, prevention rules) consulted during future tasks — mechanism and honesty caveats: [GOVERNANCE.md §Shared learning](GOVERNANCE.md).
- Reusable decisions record, searchable in chat modes.

### 3.9 Skills & MCP integration [PLANNED]
- Extensible skills and MCP (Model Context Protocol) tool servers, added by explicit user action with least-privilege scoping.

### 3.10 Remote communication [PLANNED]
- Telegram bot for notifications and approval round-trips; default ON for notifications, OFF for command execution unless enabled (see [GOVERNANCE.md §Execution controls](GOVERNANCE.md)).

### 3.11 Settings & configuration [P2 IMPLEMENTED — local persistence]
- Local settings (approval mode, retry limits, notification channels, agent credentials).
- Defaults ship safe: MANUAL approval, retry cap 3.
- **Implemented (Phase 2):** approval mode, maximum retry, and Telegram notification preference persist in SQLite and survive restarts; "Restore defaults" updates persistent storage. These remain saved preferences — autonomous execution and Telegram delivery are not operational.
- Agent credentials (Phase 3+) will use the OS keychain, never plaintext SQLite.

## 4. Non-goals

- Orqen is not a cloud service; there is no hosted backend in the product.
- Orqen does not silently execute destructive operations; destructive actions always need explicit authorization.
- Orqen does not guarantee that an agent never repeats a past mistake — shared learning reduces recurrence, it does not change model weights ([GOVERNANCE.md §Shared learning](GOVERNANCE.md)).

## 5. Success criteria for the product

1. A user can run two or more different agents on one project without code changes to Orqen.
2. Every executed feature chunk has a recorded audit, verification result, and approval decision.
3. A validated failure is retrievable by future tasks of any agent in the workspace.
4. All executed work is reproducible from the local database (Phase 2 onward).

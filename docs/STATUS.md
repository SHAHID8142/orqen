# STATUS.md — Current Status

Single source of truth for "where is the project right now". Updated at every meaningful checkpoint. Verification evidence: [QA_REPORT.md](QA_REPORT.md).

## Current position

| Field | Value |
| --- | --- |
| **Phase** | 0 — Project Foundation & Bootstrap (final remediation complete) |
| **Feature** | Phase 0 bootstrap (repository, app shell, docs, gates, CI) |
| **Progress** | All Phase 0 micro-tasks implemented; all local gates pass (17 frontend tests + 10 Rust tests); **CI = BLOCKED — EXTERNAL** (GitHub account-level billing restriction; workflow itself valid) |
| **Blockers** | **CI blocked externally:** GitHub refuses to start Actions jobs — *"The job was not started because your account is locked due to a billing issue"* (runs 41933672348, 41933868579). External to the repository; no repository change can resolve it. Rerun path documented in [QA_REPORT.md](QA_REPORT.md) (`workflow_dispatch` or next push, once GitHub permits execution). Per D-011 the repository stays PUBLIC permanently and no Actions-minute purchases are required development steps. |
| **Next review checkpoint** | Manual review of the Phase 0 remediation (incl. ChatGPT GitHub review per [COMMUNICATION.md](COMMUNICATION.md)) |
| **Next phase (after approval)** | Phase 1 — Workspace Shell & Navigation ([ROADMAP.md](ROADMAP.md)) |

## Phase 0 progress detail

| Micro-task | Status |
| --- | --- |
| A — GitHub repository | Done (created PRIVATE; PUBLIC since 2026-11-05 by user direction — permanent per D-011) |
| B — Runnable desktop foundation | Done (Tauri 2 + React + TS; launch verified locally) |
| C — Modular project structure | Done (boundaries in [ARCHITECTURE.md §Module boundaries](ARCHITECTURE.md)) |
| D — Project documentation | Done (11 docs, cross-referenced, one authoritative source per rule) |
| E — Universal agent governance | Done ([GOVERNANCE.md](GOVERNANCE.md)) |
| F — Code quality & security | Done (gates pass; least-privilege capabilities; CSP; Unicode length policy D-012) |
| G — GitHub Actions CI | Workflow authored and validated (`actionlint` clean, GitHub-listed active, jobs instantiate, `workflow_dispatch` enabled) — **BLOCKED — EXTERNAL** (account-level billing restriction) |
| H — Final verification | Done locally; CI confirmation BLOCKED — EXTERNAL (see [QA_REPORT.md](QA_REPORT.md)) |

## Open items carried forward

1. **Windows verification** — CI runs Rust checks on Linux and frontend gates on Linux; Windows packaging is not machine-verified in CI yet (planned Phase 11, [ROADMAP.md](ROADMAP.md)). Windows behavior is documented, not verified.
2. **ESLint 10 dev-only advisories** — accepted consciously (D-009), reassess next phase.
3. **Style CSP hardening** — `'unsafe-inline'` retained for styles (D-005); revisit in hardening.

## Recent history

- **2026-11-05** — Phase 0 implemented: repository created, foundation app built, docs + governance written, quality gates configured, CI workflow authored, all local gates passing.
- **2026-11-05** — CI runs investigated: `startup_failure` ×2 (private repo), then after user-directed change to public (D-010), jobs created but refused — GitHub account billing restriction identified as the external blocker (QA_REPORT row 14).
- **2026-11-05** — Phase 0 remediation: consistent Unicode code-point length validation across the IPC boundary with regression tests (D-012); `workflow_dispatch` added to CI; documentation aligned with the permanent PUBLIC visibility (D-011); duplicate/false documentation claims corrected.

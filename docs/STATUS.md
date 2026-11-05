# STATUS.md — Current Status

Single source of truth for "where is the project right now". Updated at every meaningful checkpoint. Verification evidence: [QA_REPORT.md](QA_REPORT.md).

## Current position

| Field | Value |
| --- | --- |
| **Phase** | 1 — Workspace Shell & Navigation |
| **Feature** | 001 — Workspace Shell & Navigation (review remediation complete) |
| **Progress** | Review fixes delivered: overlay-sidebar focus management, delete-confirmation focus flow, retry input editing UX. All local gates pass (62 frontend tests, 10 Rust tests). **CI = BLOCKED — EXTERNAL** (unchanged; GitHub account-level billing restriction) |
| **Blockers** | **CI blocked externally** (see [QA_REPORT.md](QA_REPORT.md) CI verification) — external to the repository; rerun path documented there (`workflow_dispatch` or next push, once GitHub permits execution). |
| **Next review checkpoint** | Manual review of the Feature 001 remediation |
| **Next feature (after approval)** | Per [ROADMAP.md](ROADMAP.md): remaining Phase 1 polish or Phase 2 — Local Persistence (SQLite) & Settings |

## Phase 1 · Feature 001 progress detail

| Micro-task | Status |
| --- | --- |
| Workspace store (types, validation, observable store) | Done — 12 unit tests |
| Settings store (approval mode, retry cap, Telegram preference) | Done — 6 unit tests |
| App shell (header, collapsible sidebar, main, project selector) | Done — shell tests incl. Escape/focus/aria states |
| Overview surface (Phase 0 runtime diagnostics preserved) | Done — 3 tests |
| Projects surface (create/select/delete + confirmation + empty states) | Done — 6 tests |
| Settings surface (3 working preferences, truthful labels) | Done — 4 tests |
| Agents + Workflows surfaces (honest planned content) | Done — covered by shell navigation tests |
| Responsive behavior | Done — CSS breakpoints at 900px/640px; narrow-mode sidebar logic tested |
| Quality gates + launch | Done — see QA_REPORT.md Phase 1 section |
| Documentation | Done — D-013; ARCHITECTURE/ROADMAP/README/QA_REPORT updated |

## Open items carried forward

1. **Windows verification** — CI runs Rust checks on Linux and frontend gates on Linux; Windows packaging is not machine-verified in CI yet (planned Phase 11, [ROADMAP.md](ROADMAP.md)).
2. **ESLint 10 dev-only advisories** — accepted consciously (D-009), reassess next phase.
3. **Style CSP hardening** — `'unsafe-inline'` retained for styles (D-005); revisit in hardening.
4. **Overlay focus containment** — narrow-mode overlay moves focus into the sidebar, makes main `inert`, and restores focus to the toggle on every close path; a full focus-trap review remains part of the Phase 11 accessibility hardening.

## Recent history

- **2026-11-05** — Phase 0 implemented: repository created, foundation app built, docs + governance written, quality gates configured, CI workflow authored, all local gates passing.
- **2026-11-05** — CI runs investigated: `startup_failure` ×2 (private repo), then after user-directed change to public (D-010), jobs created but refused — GitHub account billing restriction identified as the external blocker (QA_REPORT row 14).
- **2026-11-05** — Phase 0 remediation: consistent Unicode code-point length validation across the IPC boundary with regression tests (D-012); `workflow_dispatch` added to CI; documentation aligned with the permanent PUBLIC visibility (D-011); duplicate/false documentation claims corrected.
- **2026-11-05** — Phase 1 · Feature 001 delivered: workspace shell with five navigation surfaces, in-memory project management (validation, selection, confirmed deletion), in-memory settings with truthful preference labeling, accessibility baseline (skip link, focus management, aria states, inert overlay background). D-013 recorded.
- **2026-11-05** — Feature 001 review remediation: overlay sidebar focus management (always-mounted `hidden` sidebar with valid `aria-controls`, focus into sidebar on open, focus restored to toggle on Escape/backdrop/toggle), project delete-confirmation focus flow (Cancel focused on appear, Delete restored on cancel, surviving project or name input after delete), retry preference draft editing (free typing, commit on blur/Enter, Escape reverts, clamp on commit). 62 frontend tests passing; L-012/L-013 lessons recorded.

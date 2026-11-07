# STATUS.md — Current Status

Single source of truth for "where is the project right now". Updated at every meaningful checkpoint. Verification evidence: [QA_REPORT.md](QA_REPORT.md).

## Current position

| Field | Value |
| --- | --- |
| **Phase** | 3 — Agent Registry & Adapters |
| **Feature** | 002 — Real Agent Execution Adapters & Controlled Test Runs |
| **Progress** | Feature delivered: registry for Codex + Claude Code, PATH-based cross-platform discovery, explicit user-initiated `--version` verification (fixed noninteractive argument, timeout + whole-tree kill, bounded output; shell-invocation scoped per platform — see D-017), Main Agent preference persisted in SQLite (migration v2, real Phase 2 DB upgraded on launch). **Remediation delivered: atomic run admission (admit-before-register, RAII release), Claude Code diagnostics DISABLED (isolation not verifiable), truthful machine-readable provider parsing, starting-state cancellation, enforced reader-slot reuse, bounded finished-run retention, backend integration tests. 116 frontend + 93 Rust tests pass (83 lib + 6 execution-integration + 4 persistence-integration; plus 1 timeout-path test under the test-only `test-timeouts` feature); all local gates pass. CI = BLOCKED — EXTERNAL (unchanged)** |
| **Blockers** | **CI blocked externally** (see [QA_REPORT.md](QA_REPORT.md) CI verification) — external to the repository; rerun path documented there. |
| **Next review checkpoint** | Manual review of Phase 3 Feature 002 (execution adapters, diagnostic run) |
| **Next feature (after approval)** | Per [ROADMAP.md](ROADMAP.md): Phase 4 — Chat Infrastructure & Modes |

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

## Phase 3 · Feature 002 progress detail

| Micro-task | Status |
| --- | --- |
| Adapter contract + Codex/Claude Code adapters (documented noninteractive invocations) | Done — 5 Rust tests |
| Controlled diagnostic run (isolated dir, fixed prompt, timeout, cancel, bounded output) | Done — runner tests + full-pool rejection sentinel test |
| Run registry (in-memory, session-scoped; single active run enforced) | Done |
| Tauri commands (start/get/cancel diagnostic run; async) | Done |
| Frontend run state machine (idle → awaiting confirmation → starting → running → terminal) | Done — 5 store tests |
| Agents UI "Agent Test Run" section (confirmation, running, result, cancel) | Done — component tests |
| Real-provider verification | PARTIAL — Claude Code 2.1.316 present; the real run fails at the SDK level due to the user's local CLI configuration (`unrecognized_model`); Codex absent. Recorded truthfully; the UI surfaces exactly this failure output. |

## Phase 3 · Feature 001 progress detail

| Micro-task | Status |
| --- | --- |
| Registry definitions (Codex, Claude Code; capability metadata) | Done — 3 Rust tests |
| CLI discovery (PATH search, platform extensions, failure handling) | Done — 7 Rust tests |
| Controlled CLI verification (fixed `--version` argument, timeout + whole-tree kill, bounded output) | Done — 13 Rust tests |
| Migration v2 (`settings.main_agent_id`) + persistence | Done — upgrade/rollback/restart tests incl. real v1→v2 integration |
| Agents IPC (`list_agents`, `discover_agent`, `verify_agent`, `set_main_agent`) | Done — registry id validation at the boundary |
| Frontend agents store + Main Agent preference in settings store | Done — 10 agents store tests |
| Agents UI (status chips, verify, Main Agent radios, honesty note) | Done — 11 component tests |
| Real-machine evidence | Done — Claude Code 2.1.316 installed and version-probed; Codex genuinely absent (real negative case) |

## Phase 2 · Feature 001 progress detail

| Micro-task | Status |
| --- | --- |
| A — Database foundation (location, connection strategy, failure policy) | Done — 4 Rust tests (incl. corruption never deleted) |
| B — Schema & transactional migrations (v1) | Done — 7 Rust tests (idempotence, rollback, constraints, FK) |
| C — Persistent project operations | Done — atomic create/select/delete (transactional, in-transaction snapshots); regression tests incl. auto-select contract + reopen survival |
| D — Settings persistence | Done — 5 Rust tests (defaults, updates, restore, invalid rejected, reopen survival) |
| E — React state integration (loading/ready/error, retry, serial mutation queue, snapshot convergence) | Done — 24 store tests incl. deferred-reorder races |
| F — Error handling & data safety | Done (storage error kind, recovery guidance, no auto-reset; verified in tests) |
| G — Automated testing | Done — 33 Rust + 80 frontend tests |
| H — Restart verification | DB level verified (real-launch probe); interactive GUI restart → manual checklist |

## Open items carried forward

1. **Windows verification** — CI runs Rust checks on Linux and frontend gates on Linux; Windows packaging is not machine-verified in CI yet (planned Phase 11, [ROADMAP.md](ROADMAP.md)). Cross-compilation from this macOS host is blocked at the bundled-SQLite C build (no MSVC/mingw toolchain installed); the Unix-only API audit is instead verified statically (every `std::os::unix`/`libc` use is cfg-gated). Windows diagnostic-run runtime behavior (direct-child kill only) remains unverified until a Windows machine or CI runner exists.
2. **Codex diagnostics are macOS-only; real-provider acceptance still PARTIAL** — the confinement mechanism is officially BETA, so safety is re-proven at every diagnostic start by the per-run enforcement preflight (D-024) on the actual installed binary; the automated sandbox-harness suite (12 tests, FAILS under `ORQEN_REQUIRE_CODEX_ENFORCEMENT=1` without a supported binary) is harness-level evidence, and end-to-end `codex exec` enforcement awaits one user-authorized real-provider run. Linux/Windows fail closed. Claude Code remains disabled (inherited global configuration, version-dependent restrictions, no read-only enforcement in print mode; the live `unrecognized_model` failure is unresolved) ([ARCHITECTURE.md §Trust boundaries](ARCHITECTURE.md), [QA_REPORT.md](QA_REPORT.md)).
3. **ESLint 10 dev-only advisories** — accepted consciously (D-009), reassess next phase.
4. **Style CSP hardening** — `'unsafe-inline'` retained for styles (D-005); revisit in hardening.
5. **Visual confirmation of focus indicators** — the overlay interaction model is verified by automated tests and a real-browser keyboard matrix (QA_REPORT accessibility-fix section); final on-screen confirmation of focus ring appearance by the user remains part of manual review.
6. **Interactive restart verification** — persistence verified by automated reopen tests and a real-launch DB probe; the visual restart walkthrough (create → relaunch → see data) awaits the user's manual pass (checklist in the Phase 2 final report).
7. **Credentials at rest** — no secrets are stored today; when agent credentials arrive (Phase 3+), they go to the OS keychain, never plaintext SQLite ([ARCHITECTURE.md §Persistence](ARCHITECTURE.md)).

## Recent history

- **2026-11-05** — Phase 0 implemented: repository created, foundation app built, docs + governance written, quality gates configured, CI workflow authored, all local gates passing.
- **2026-11-05** — CI runs investigated: `startup_failure` ×2 (private repo), then after user-directed change to public (D-010), jobs created but refused — GitHub account billing restriction identified as the external blocker (QA_REPORT row 14).
- **2026-11-05** — Phase 0 remediation: consistent Unicode code-point length validation across the IPC boundary with regression tests (D-012); `workflow_dispatch` added to CI; documentation aligned with the permanent PUBLIC visibility (D-011); duplicate/false documentation claims corrected.
- **2026-11-05** — Phase 1 · Feature 001 delivered: workspace shell with five navigation surfaces, in-memory project management (validation, selection, confirmed deletion), in-memory settings with truthful preference labeling, accessibility baseline (skip link, focus management, aria states, inert overlay background). D-013 recorded.
- **2026-11-05** — Feature 001 review remediation: overlay sidebar focus management (always-mounted `hidden` sidebar with valid `aria-controls`, focus into sidebar on open, focus restored to toggle on Escape/backdrop/toggle), project delete-confirmation focus flow (Cancel focused on appear, Delete restored on cancel, surviving project or name input after delete), retry preference draft editing (free typing, commit on blur/Enter, Escape reverts, clamp on commit). 62 frontend tests passing; L-012/L-013 lessons recorded.
- **2026-11-05** — Feature 001 final accessibility fix: narrow overlay is now a fully contained modal drawer (skip link + header + main `inert` while open, `role="dialog"`/`aria-modal`, in-drawer close button, effect-based focus restoration to the toggle). Real-browser keyboard verification found and fixed a Chromium wrap-around defect across `inert` regions (explicit boundary wrap added, overlay mode only). 70 frontend tests passing; L-014 recorded.
- **2026-11-05** — Phase 2 · Feature 001 delivered: SQLite persistence via rusqlite (WAL, foreign keys, busy timeout), transactional migrations (v1), projects + selection + settings persisted across restarts, snapshot-convergent frontend stores with loading/error/retry states. D-014 recorded; 80 frontend + 33 Rust tests; real-launch restart probe verified (data preserved, no wipe).
- **2026-11-06** — Phase 3 · Feature 002 delivered: execution adapters (Codex `exec --sandbox read-only`, Claude `-p` with denied mutating tools), controlled diagnostic run in an isolated directory (fixed prompt, 120s timeout, whole-tree cancel), run registry (session-scoped, single active run), Agents UI test-run section with confirmation + truthful result rendering. Real-provider acceptance: attempted against the installed Claude Code 2.1.316 — the run fails at the SDK level due to the user's local CLI configuration (recorded, not simulated); Codex absent.
- **2026-11-06** — Phase 3 · Feature 001 final remediation: settings error/Retry moved outside the disabled preference fieldset (verified with real browser clicks + keyboard), CLI probes spawn in their own process group with whole-tree termination on timeout (no more orphaned pipe holders), reader abandonment bounded and counter-clean. 109 frontend + 70 Rust tests; L-020 recorded.
- **2026-11-06** — Phase 3 · Feature 001 review remediation: verification moved off the Tauri main thread (async + spawn_blocking), end-to-end probe deadline with descendant-pipe grace period, Windows cmd.exe semantics scoped honestly, unix discovery requires the execute bit, Main Agent UX surfaces settings errors with retry and queues updates (no silent drops). 108 frontend + 67 Rust tests; L-018 recorded.
- **2026-11-06** — Phase 3 · Feature 001 delivered: Agent Registry (Codex + Claude Code), PATH-based discovery, explicit user-initiated `--version` verification (fixed noninteractive argument, timeout + whole-tree kill, bounded output; shell-invocation scoped per platform — see D-017), Main Agent preference persisted via migration v2 — real Phase 2 database upgraded on launch with all data intact. D-016 recorded; 104 frontend + 64 Rust tests. Real-machine evidence: Claude Code 2.1.316 verified; Codex absent.
- **2026-11-06** — Phase 2 · Feature 001 review remediation: creation auto-selects atomically (D-015), serial mutation queue in the frontend store (no stale snapshots, queued-not-discarded requests), future schema versions rejected untouched, sanitized storage errors with local diagnostics; real integration test suite added (lifecycle + concurrent serialization). 83 frontend + 39 Rust tests; L-015/L-016 recorded.

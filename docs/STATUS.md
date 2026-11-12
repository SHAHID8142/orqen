# STATUS.md — Current Status

Single source of truth for "where is the project right now". Updated at every meaningful checkpoint. Verification evidence: [QA_REPORT.md](QA_REPORT.md).

## Current position

| Field | Value |
| --- | --- |
| **Phase** | 4 — Chat Infrastructure & Modes |
| **Feature** | 001 — Chat Infrastructure & Modes (Phase 3 Feature 002 accepted with a real-provider run) |
| **Progress** | **Phase 4 delivered: mode-aware per-project chat (7 GOVERNANCE §3 modes), SQLite persistence (schema v3 — conversations + messages with cascading deletes), per-message mode annotation, Markdown transcript export (D-026), Chat surface added as the 6th navigation module, Overview capability lists made truthful (Phases 0–4 implemented). Chat is records-only — no agent invocation; modes never grant execution permissions. Phase 3 remains fully delivered and accepted (user-authorized real-provider diagnostic SUCCEEDED, 2026-11-07). 121 frontend + 131 Rust tests pass per suite run; all local gates pass. CI = BLOCKED — EXTERNAL (unchanged)** |
| **Blockers** | **CI blocked externally** (see [QA_REPORT.md](QA_REPORT.md) CI verification) — external to the repository; rerun path documented there. |
| **Next review checkpoint** | Manual review of Phase 3 Feature 002 — **final manual acceptance COMPLETE: a user-authorized real-provider diagnostic SUCCEEDED through the app UI (codex, 8.6 s, exit 0, real response "I'm available and ready to help."; cleanup verified); see QA_REPORT ** |
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

## Phase 4 · Feature 001 progress detail

| Micro-task | Status |
| --- | --- |
| Migration v3 (conversations + messages; mode/role CHECK constraints; cascading deletes) | Done — 6 migration/schema tests |
| Chat command module (CRUD, rename, mode switch, append, validation — D-012 code points, allowlisted modes/roles) | Done — 12 chat tests |
| Markdown transcript export (D-026: verbatim content, header metadata, honesty footer, safe slug file names) | Done — 2 export tests |
| Chat surface (6th nav module): conversation list, thread, composer, mode selector, export overlay (modal a11y: focus, Escape, aria-modal) | Done — 3 shell-level tests + full verify |
| Store convergence (serial queue, per-project scoping, project-switch reset) | Done — covered by shell tests |
| Overview truthfulness fix (implemented/planned lists reflect Phases 0–4) | Done |
| Quality gates | Done — see QA_REPORT.md Phase 4 section |
| Documentation | Done — D-026; ARCHITECTURE/PRODUCT/ROADMAP/README updated |

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

- **2026-11-13** — Phase 4 delivery hardening: the npm npx cache evicted the previously authorized codex 0.164.0 binary (enforcement gate correctly FAILED without a usable binary — fail-closed verified live). The ChatGPT desktop app's own codex-cli 0.167.0-alpha.3.2 exposed a real defect: pre-release version suffixes broke the semver patch parse, rejecting a legitimate newer CLI as "older than the minimum" — FIXED (base-triple comparison; pre-release unit tests added). The full 12-test enforcement suite then PASSED against the 0.167.0-alpha binary — cross-version sandbox-policy confirmation. No downloads or installs.
- **2026-11-13** — Phase 4 · Feature 001 delivered: chat infrastructure & modes — schema v3 (per-project conversations + messages, mode/role constraints, cascading deletes), backend-authoritative mode catalog (GOVERNANCE §3, D-026), per-message mode annotation (mid-conversation switches stay honest in history), Markdown transcript export with verbatim content, Chat surface as the 6th navigation module with modal export a11y, Overview implemented/planned lists corrected to reflect Phases 0–4. Chat is records-only: no agent invocation, modes never grant execution permissions. 121 frontend + 131 Rust tests; all gates pass.
- **2026-11-07** — Phase 3 · Feature 002 final manual acceptance: the owner authorized ONE controlled diagnostic; it was run through the real app UI and **SUCCEEDED against the real provider** (codex-cli 0.164.0 via the previously authorized cached binary on the app's process PATH — no install, no config change; authenticated as ChatGPT login per `codex login status`, credentials never exposed). Real response: "I'm available and ready to help." (8.6 s, exit 0). Post-run cleanup verified (isolated work dir removed; no lingering ORQEN-spawned processes). Real-provider acceptance is now PASS; the sandbox-harness vs live-run distinction in earlier docs is resolved by this run.
- **2026-11-07** — Phase 3 · Feature 002 last review corrections: (1) `run_probe` no longer performs unbounded reader joins — completed readers are joined, incomplete ones are detached with their enforced reader-slot reservations accounted until the reader actually terminates; a NEW fixture proves a setsid-escaped pipe-holding descendant cannot extend the probe past its deadline (the test cleans up its escaped descendant explicitly); probe tests and verification pool tests now share ONE serialization lock. (2) The network test's HTTP server uses a nonblocking listener with a bounded accept-loop wakeup and a shutdown flag, joined deterministically via an RAII guard on every path; the scenario runs twice proving no listener thread or port is left behind. (3) Windows test compilation restored: the tests module's Unix-only imports, fixture helper, and helpers are `#[cfg(unix)]`-gated (platform-independent adapter tests remain runnable everywhere); the cross-target `cargo check --target x86_64-pc-windows-msvc` still stops at the `libsqlite3-sys` C build (`cc` cannot find MSVC headers on this macOS host — before any Orqen code), so Windows runtime verification remains unclaimed. L-024 recorded. 118 frontend + 131 Rust tests; 12/12 enforcement tests executed under `ORQEN_REQUIRE_CODEX_ENFORCEMENT=1`.
- **2026-11-07** — Phase 3 · Feature 002 preflight safety & test-integrity fix: the preflight probe helper's deadline now covers the WHOLE lifecycle (spawn → execution → concurrent bounded output drain → reader grace → reaping) — a descendant holding the inherited pipes can no longer hang a diagnostic start beyond the deadline (the post-exit unbounded `read_to_string` defect); probes run in their own process group with RAII termination/reaping and enforced reader-slot reservations (structured `ReaderSlotsExhausted`/`TimedOut`/`Spawn`/`Poll` errors). The sandbox network test now serves a real HTTP 200 sentinel and requires the exact unsandboxed curl as a positive control before accepting the sandboxed connect-denial evidence (the old accept-and-close listener made nonzero curl exits meaningless). Preflight negative probes hardened with positive-control discipline (unsandboxed protected-read control, permitted inside-write control, OS denial signatures, configuration-error text; ambiguous results fail closed). Per-run preflight scope stated precisely: filesystem confinement re-verified per run; network restriction is suite-verified policy (UI wording narrowed). D-025, L-022, L-023 recorded. 118 frontend + 130 Rust tests; 12/12 enforcement tests pass under `ORQEN_REQUIRE_CODEX_ENFORCEMENT=1` against the authorized cached codex-cli 0.164.0.
- **2026-11-07** — Phase 3 · Feature 002 final acceptance hardening: per-run sandbox enforcement preflight introduced (D-024 — the permissions mechanism is documented beta, so the installed binary must PROVE enforcement at every start); `--strict-config` added (unknown config fields hard-error); enforcement suite expanded (protected reads in `$HOME`, `/tmp`, and the per-user temp dir; network denial with positive control; denial-signature assertions; suite FAILS without a supported binary under the gate flag). Docs corrected to distinguish sandbox-harness proofs from `codex exec` execution.
- **2026-11-07** — Phase 3 · Feature 002 safe Codex adapter completion: per-run Codex permissions implemented WITHOUT touching the user's global configuration — `-c` overrides (top TOML precedence layer) define an app-owned read-deny-by-default profile with a per-run random profile name, `--ignore-user-config`/`--ignore-rules`/`--ephemeral` (nothing inherited; auth still from `CODEX_HOME`), credential-shaped env vars excluded, and NO legacy `--sandbox` flag (the silent-fallback trap). Codex diagnostics ENABLED on macOS for CLI ≥ 0.164.0 (version recorded in the eligibility ledger); entry-point audit confirms no bypassing internal caller. D-023 recorded.
- **2026-11-07** — Phase 3 · Feature 002 final security acceptance: Codex diagnostic execution DISABLED after the official-documentation review (D-022 — later corrected by D-023's research); backend-authoritative eligibility ledger (fresh backend verification + executable identity fingerprint re-checked by the runner before spawn; direct-IPC bypass tests); process-group termination extended to normal exits with a surviving-descendant fixture test; duplicate terminal-result recording removed; cancellation honored before spawn. No live provider run performed (reported PARTIAL).
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

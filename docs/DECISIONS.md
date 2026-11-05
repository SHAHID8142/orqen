# DECISIONS.md — Decision Log

Format per [COMMUNICATION.md §2.4](COMMUNICATION.md). New entries append at the end. A decision is recorded only when approved (by the user, or by this repository's rules for implementation-detail choices).

---

## D-001 | 2026-11-05 | APPROVED — Technology foundation
**Context:** Phase 0 required a stable, provider-independent desktop foundation.
**Decision:** Tauri 2 + React 19 + TypeScript (strict) + Vite + npm; Rust core; Vitest for tests; ESLint 10 + cargo fmt/clippy for quality; GitHub Actions for CI. SQLite deferred to Phase 2.
**Consequences:** macOS/Windows capable; Linux reserved for CI rust checks. No state layer until Phase 2 — acceptable for foundation scope.

## D-002 | 2026-11-05 | APPROVED — Bundle identifier
**Context:** `create-tauri-app` generated `com.maryamahad.orqen` from the local machine user, which does not match repository ownership.
**Decision:** identifier `io.github.shahid8142.orqen` (GitHub-hosted reverse-domain convention); product name `Orqen`.
**Consequences:** changing the identifier later would orphan app-data paths; treat as stable from Phase 2 onward.

## D-003 | 2026-11-05 | APPROVED — TypeScript 6.0.x instead of 7.x
**Context:** latest stable TypeScript was 7.0.2, but `typescript-eslint 8.72.1` declares peer range `>=4.8.4 <6.1.0`; installing TS 7 would break the lint toolchain.
**Decision:** pin `typescript@~6.0.3`. Re-evaluate when typescript-eslint supports TS 7.
**Consequences:** slightly older compiler; full lint compatibility. Upgrade is a recorded decision, not a routine bump.

## D-004 | 2026-11-05 | APPROVED — Least-privilege capabilities; opener plugin removed
**Context:** the scaffold shipped `tauri-plugin-opener` and `opener:default` capability; Phase 0 has no feature that needs opening external URLs.
**Decision:** capabilities contain `core:default` only; the opener plugin dependency was removed. Plugins are added per feature, deliberately, with scoped permissions and a decision entry.
**Consequences:** smaller attack surface; external-link opening must be re-added explicitly when a feature needs it.

## D-005 | 2026-11-05 | APPROVED — Content Security Policy enabled
**Context:** scaffold shipped `csp: null` (no CSP).
**Decision:** set CSP `default-src 'self' ipc: http://ipc.localhost; img-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline'; font-src 'self'` in `tauri.conf.json`.
**Consequences:** `'unsafe-inline'` retained for styles to keep Vite-style injection working; tightening styles further is a future hardening item.

## D-006 | 2026-11-05 | APPROVED — Test stack: Vitest + Testing Library + jest-dom
**Context:** meaningful automated smoke test required for the React foundation.
**Decision:** Vitest 5 (jsdom environment, config inside `vite.config.ts`), `@testing-library/react`, `@testing-library/jest-dom` for typed DOM assertions; per-file `cleanup()` registration because Vitest does not enable global hooks (see [LESSONS_LEARNED.md](LESSONS_LEARNED.md) L-001).
**Consequences:** two small test-only dependencies; frontend tests can render real components against a mocked IPC layer.

## D-007 | 2026-11-05 | APPROVED — Clippy denies warnings
**Context:** quality gates must fail on Rust lint drift.
**Decision:** `cargo clippy --all-targets -- -D warnings` is a required gate (`npm run rust:clippy`), same in CI.
**Consequences:** speculative/dead code fails the build — enforced by the `CommandError::Internal` removal incident (L-002).

## D-008 | 2026-11-05 | APPROVED — Execution controls defaults
**Context:** Phase 0 governance needed concrete defaults before any orchestration exists.
**Decision:** Approval Mode MANUAL; Telegram notifications ON (future); Maximum Retry 3; approved features run all internal micro-tasks; feature handoff requires manual approval. Source of truth: [GOVERNANCE.md §Execution controls](GOVERNANCE.md).
**Consequences:** every later phase implements against these defaults; changes are new decision entries.

## D-009 | 2026-11-05 | APPROVED — npm audit policy
**Context:** `npm audit` reported 10 moderate advisories in dev-only ESLint tooling (minimatch via `@eslint/config-array`, GHSA-g3hv-75wg-m249); the offered fix downgrades ESLint to 9.x.
**Decision:** keep ESLint 10 (current stable). Production (`--omit=dev`) dependencies are clean and `npm run audit` gates production deps. Dev-only advisories are recorded in [QA_REPORT.md](QA_REPORT.md) and reassessed each phase.
**Consequences:** a known, bounded dev-tooling advisory is accepted consciously; revisit when eslint 10.x ships a patched dependency chain.

## D-010 | 2026-11-05 | APPROVED (user-directed) — Repository made public
**Context:** Phase 0 originally required a private repository; two CI runs on the private repo ended in `startup_failure` with zero jobs and no annotations (runs 41932865723, 41933399059). The user then explicitly instructed that the repository be made public.
**Decision:** `SHAHID8142/orqen` is now **public** by direct user instruction, superseding the Phase 0 private requirement. No secrets existed in the repository at the time of the change (pattern scan + staged-diff review passed before the first push).
**Consequences:** source and documentation are publicly visible; CI runs on public repositories are free and unrestricted, which may also resolve the Actions startup failures if the cause was private-repo Actions restrictions. Security posture is unchanged: never commit credentials, tokens, or local data.

## D-011 | 2026-11-05 | APPROVED (user-directed) — PUBLIC visibility is permanent
**Context:** private-repo GitHub Actions minutes are exhausted and the user cannot purchase additional minutes; the user has explicitly confirmed the repository must remain PUBLIC.
**Decision:** `SHAHID8142/orqen` stays public permanently. Never change the visibility to private; never suggest purchasing GitHub Actions minutes as a required development step. Public repositories receive free Actions minutes, but an account-level billing restriction can still prevent jobs from executing — CI status is therefore recorded independently of visibility.
**Consequences:** secret hygiene is a standing requirement (scan before every push; see [AGENTS.md §Security rules](../AGENTS.md)); CI progress is tracked as `BLOCKED — EXTERNAL` while GitHub cannot execute jobs (see [QA_REPORT.md](QA_REPORT.md)).

## D-012 | 2026-11-05 | APPROVED — String length policy: Unicode scalar values
**Context:** `echo_ping` validation used JavaScript `.length` (UTF-16 code units) on the frontend and Rust `.len()` (UTF-8 bytes) on the backend — inconsistent for Bangla text, emoji, and other multi-unit scripts (e.g. 150 "🙂" = 300 UTF-16 units / 600 UTF-8 bytes / 150 code points).
**Decision:** all cross-boundary string length limits count **Unicode scalar values (code points)**: Rust `chars().count()`, JavaScript spread iteration `[...value].length`. Composed emoji such as "👍🏽" (base + skin-tone modifier) count as two code points. Grapheme-cluster counting was considered and deferred: it would require `unicode-segmentation` + `Intl.Segmenter`, whose edge-case agreement is platform-dependent, for an arbitrary limit where consistency — not linguistic precision — is the requirement.
**Consequences:** both sides now accept or reject identically for any input; enforced by Unicode regression tests in `src-tauri/src/commands/app_info.rs` and `src/lib/ipc/appInfo.test.ts` (English, Bangla, emoji, whitespace, empty, exact-200, 201-over). Future string limits must reuse this policy.

## D-013 | 2026-11-05 | APPROVED — Phase 1 shell: state-based navigation, dependency-free stores
**Context:** Phase 1 required navigation across five fixed surfaces and in-memory workspace/settings state without new dependencies.
**Decision:** (1) View switching is state-based (`ViewId` in `src/app/views.ts`, rendered by `AppShell`) — no router dependency; deep links are not a Phase 1 requirement. Revisit via a new decision if URL-addressable views become necessary. (2) Domain state lives in small framework-agnostic observable stores (`src/lib/workspace`, `src/lib/settings`) bound to React via `useSyncExternalStore` + context providers; stores are injectable into `App` for tests. (3) All workspace/settings state is in-memory session state only — persistence is Phase 2 scope and the UI says so.
**Consequences:** zero new runtime dependencies; navigation semantics tested via `aria-current` and heading focus; restart persistence is explicitly not claimed anywhere in the UI.

## D-014 | 2026-11-05 | APPROVED — SQLite persistence stack (Phase 2)
**Context:** Phase 2 required reliable local-first persistence for projects, the selection and settings, owned by Rust, with minimal dependencies.
**Decision:**
- `rusqlite` with the `bundled` feature (SQLite compiled into the binary — no system library dependency, identical behavior on macOS and Windows).
- One connection behind a managed mutex (`DbState`); `journal_mode=WAL` + `synchronous=NORMAL` + `foreign_keys=ON` + `busy_timeout` 5 s.
- Hand-rolled transactional migrations over `PRAGMA user_version` (per-migration transaction, rollback on failure, re-run safe) instead of the `rusqlite::migration` helper — full control for rollback testing.
- Selection state lives in `settings.selected_project_id` with `REFERENCES projects(id) ON DELETE SET NULL`.
- `uuid` v4 for project IDs, `time` (RFC 3339) for timestamps.
- New dependencies: `rusqlite`, `uuid`, `time` (Rust side only; no new npm dependencies).
**Consequences:** database files live in the OS app-data directory and are gitignored; corrupted databases fail closed with recovery guidance (never auto-reset); the frontend receives full snapshots from every mutation, keeping UI and database converged.

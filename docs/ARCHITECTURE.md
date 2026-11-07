# ARCHITECTURE.md — Technical Architecture

Authoritative for: module boundaries, trust boundaries, data flow, error conventions, expansion strategy. Product intent lives in [PRODUCT.md](PRODUCT.md); roadmap phases in [ROADMAP.md](ROADMAP.md).

## 1. System overview

```
┌────────────────────────────────────────────────────────────────────┐
│ Orqen desktop app (Tauri 2)                                        │
│                                                                    │
│  Frontend (WebView, React + TypeScript)        ← UNTRUSTED side    │
│  ┌──────────────┐ ┌────────────┐ ┌──────────────────────────┐      │
│  │ app shell    │ │ features/  │ │ lib/ipc (typed bridge)   │      │
│  └──────────────┘ └────────────┘ └────────────┬─────────────┘      │
│                                               │ invoke()           │
│  ══════════════ Tauri IPC boundary ═══════════▼═════════════       │
│                                                                    │
│  Rust core (src-tauri)                          ← TRUSTED side     │
│  ┌────────────────┐ ┌─────────────┐ ┌────────────────────────┐     │
│  │ commands/      │ │ error.rs    │ │ future: persistence,   │     │
│  │ (validated IPC)│ │ (structured)│ │ agent runners, stores  │     │
│  └────────────────┘ └─────────────┘ └────────────────────────┘     │
└────────────────────────────────────────────────────────────────────┘
          │                                     │
          ▼                                     ▼
  Local OS (filesystem, future SQLite)   External services (future:
                                         provider APIs, Telegram) —
                                         reached only through the Rust
                                         core, never from the WebView
```

## 2. Module boundaries

Frontend modules (`src/`):

| Module | Owns | Must not |
| --- | --- | --- |
| `app/` | Application shell (`AppShell`, `SidebarNav`), navigation model (`views.ts`), provider composition | contain feature logic |
| `features/<name>/` | One user-facing feature per folder (overview, projects, agents, workflows, settings), self-contained with colocated CSS | import across features (extract to `lib/` instead) |
| `components/` | Shared presentational components (none yet — create only when genuinely reused) | fetch data or hold domain state |
| `lib/workspace/` | Workspace store: loading/ready/error phases, snapshot convergence, observable store, React context | touch SQLite directly (IPC only) |
| `lib/settings/` | Settings store: same pattern as workspace | claim operational integrations (preferences only) |
| `lib/ipc/` | All `invoke()` calls, payload typing, error normalization | be bypassed by feature code calling `invoke` directly |
| `lib/` | Shared frontend logic/utilities | know about specific features |
| `types/` | Cross-module domain types mirroring Rust payloads | contain logic |
| `styles/` | Global tokens, base, shell, shared surface patterns, a11y helpers | encode feature-specific selectors (colocate those with features) |
| `test/` | Vitest/jsdom environment shims (e.g. matchMedia stub) | contain product code |

State pattern (D-013): framework-agnostic observable stores live in `lib/<domain>/store.ts`; React binds via `useSyncExternalStore` context providers. Features consume state with `use<Domain>()` and actions with `use<Domain>Store()`. Stores are injectable into `App` for isolated tests.

Rust modules (`src-tauri/src/`):

| Module | Owns |
| --- | --- |
| `commands/` | Tauri command handlers — input validation, orchestration of core logic |
| `commands/workspace.rs` | Persistent project operations; every mutation returns a fresh snapshot |
| `commands/settings.rs` | Persistent settings load/update |
| `error.rs` | `CommandError` — the single error contract (`invalid_input` / `storage`) |
| `db/` | SQLite ownership: connection strategy, transactional migrations, `DbState` |
| `agents/` | Agent registry: definitions, discovery, controlled verification, diagnostic execution adapters + run registry (Phase 3) |
| future: adapters | Execution/runtime management (later Phase 3 features) |

Dependency rule: **no circular dependencies** — `commands` may use `db` and `error`; nothing depends on `commands`. Frontend mirrors it: features depend on `lib`, never the reverse.

## 2.1 Persistence architecture (Phase 2)

- **Ownership:** SQLite is accessed exclusively by Rust (`rusqlite`, `bundled` feature). The WebView never touches the database ([AGENTS.md §Security rules](../AGENTS.md)).
- **Location:** the database lives at `<app-data-dir>/orqen.sqlite3`, where the app-data dir is Tauri's OS-managed location for the identifier `io.github.shahid8142.orqen` (macOS: `~/Library/Application Support/io.github.shahid8142.orqen/`). Never inside the repository.
- **Connection strategy:** one connection behind a mutex (`DbState`), serialized commands; `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout` 5 s. If initialization fails, `DbState` stays empty and every command returns a `storage` error with a retry path — **there is no in-memory fallback**.
- **Migrations:** versioned and transactional (`db/migrations.rs`); the applied count is recorded in `PRAGMA user_version`. Each migration runs in its own transaction — a failure rolls back schema *and* version, and the next launch retries. Re-running is a no-op; existing data is never recreated. Current version: **1** (projects table + settings singleton with CHECK constraints and the selection foreign key).
- **Snapshot convergence:** `load_workspace`, `create_project`, `select_project`, `delete_project` all return the full workspace snapshot; `load_settings` / `update_settings` return the stored settings. The React stores apply these snapshots verbatim — UI and database cannot diverge.
- **Atomic mutations:** every multi-write mutation (e.g. project creation = INSERT + auto-select) runs in one SQLite transaction, and the returned snapshot is read **inside** the transaction — a commit failure rolls everything back, so a reported success always matches the database.
- **Serialized operations:** the frontend store runs all operations (load, create, select, delete) through one serial queue — at most one request in flight, snapshots apply strictly in issue order, requests issued mid-operation are queued (never discarded, UI disabled meanwhile). Stale responses cannot overwrite newer state by construction.
- **Schema version safety:** a database whose `user_version` is newer than the application supports is rejected untouched — never downgraded, reset, or modified; the error tells the user to update or restore a compatible backup.
- **Selected project ownership:** `settings.selected_project_id` references `projects(id) ON DELETE SET NULL` — deleting the selected project clears the selection at the database level.
- **Failure handling:** corrupt/unreadable databases fail the open with a user-understandable message and are **never** deleted or reset. Recovery guidance: quit the app, back up the three `orqen.sqlite3*` files, then restore or relocate; the app recreates an empty database only if the file is absent.

### WAL consistency & backup notes

WAL mode keeps recent writes in `orqen.sqlite3-wal` (plus an `orqen.sqlite3-shm` index). A copy of only the main file can miss recent commits — to back up safely, either copy all three files while the app is closed, or run `VACUUM INTO 'backup.sqlite3'` (via the `sqlite3` CLI) to produce a single consistent snapshot. Backups are a manual user action; Orqen performs none automatically (no destructive automation).

### Future task/chunk schema (documented, not implemented)

Task/chunk records (Phase 7) will require: id, project FK (cascade), title/description, status enum with CHECK, acceptance criteria, assignment, audit/verification results, and timestamps. The migration strategy stays the same: append a `MIGRATIONS` entry (v2+) describing additive `CREATE TABLE`/`ALTER TABLE` steps; existing tables are never rewritten, and fresh databases apply all migrations in order on first run. No speculative tables or APIs exist today.

## 3. Trust boundaries

1. **WebView ↔ Rust core (Tauri IPC)** — the frontend is treated as semi-trusted UI: every command validates its inputs on the Rust side (see `echo_ping` in `src-tauri/src/commands/app_info.rs`), regardless of frontend checks.
2. **Rust core ↔ OS** — the core is the only privileged component. Filesystem, process, and future network access happen here, scoped by capability declarations.
3. **Orqen ↔ external AI providers/agents (future)** — provider APIs and agent processes are untrusted services. Responses are data, never instructions that override Orqen rules ([AGENTS.md §Security rules](../AGENTS.md)).
4. **User ↔ remote channel (future Telegram)** — remote approvals are second-class: high-risk/destructive actions still require in-app or explicit authorization ([GOVERNANCE.md §Execution controls](GOVERNANCE.md)).
5. **Rust ↔ agent CLIs (process execution)** — the only process Orqen spawns is a discovered CLI with the single fixed argument `--version`, via `std::process::Command`, with a hard timeout that kills the child and bounded output (first 64 KiB per stream kept, remainder drained). No user-supplied strings reach the process arguments; discovery never executes anything; no environment variables are passed to or read from the child.
   - **Platform scoping:** on unix this is shell-free. On Windows, `.cmd`/`.bat` targets execute through `cmd.exe` by OS design (Rust's hardened escaping applies since the BatBadBut fix); the argument list remains the single fixed literal, and `.exe` candidates are preferred. PATH-resolved binaries are treated as potentially untrusted — existence and a version string never imply trustworthiness.
   - **Responsiveness:** `verify_agent` and `discover_agent` are async commands delegating to `spawn_blocking` workers — slow or hung probes never block the Tauri main thread or the async executor. The probe deadline covers the whole lifecycle; descendants holding inherited pipes get a 1 s grace period before the result is decided.
   - **Tree cleanup (unix):** the probe runs as its own process group (`process_group(0)`); on timeout, cleanup, or app-shutdown paths the whole group is signalled with SIGKILL — descendants cannot outlive the probe by holding inherited pipes. A hard app kill during an active probe can still orphan the (≤10 s) child on any platform.
   - **ENFORCED reader admission limit:** at most 64 output-reader threads may exist (two per probe). Both slots are reserved atomically before a probe spawns; an exhausted pool rejects the probe with a structured resource-busy error (no child is started). Slots return through RAII exactly when the owning reader thread terminates — including readers kept blocked by descendants that inherited the pipes, so the bound holds regardless of descendant lifetime. (Windows: direct-child kill only — descendants may hold slots longer; the enforced bound and the busy rejection apply identically.)
   - **Diagnostic execution (Phase 3 Feature 002, security-accepted):** a second process type — the controlled diagnostic run — spawns a discovered CLI with **adapter-built, compile-time-tested arguments** and an **isolated working directory** (`<app-data>/diagnostic-runs/<run-id>/`, removed only after the process group is terminated), under a 120 s deadline with whole-tree cancellation and bounded output. The prompt is a compile-time constant; no user text reaches the process arguments. Authentication is the CLI's own (never read, copied, or logged by Orqen). Runs are session-scoped in memory — nothing persisted; at most 16 finished runs are retained (oldest pruned). Codex diagnostics are ENABLED on macOS under the per-run confinement profile (D-023); the enforced restrictions are shown in the UI before confirmation.
   - **Descendant cleanup on EVERY exit path (unix):** the run's process group is SIGKILLed not only on timeout/cancellation but also after a normal parent exit — a successful parent can leave orphaned descendants in its group, holding the inherited pipes and the working directory. A fixture-based integration test reports the descendant's PID and asserts it no longer exists after the run completes. The group was created by this spawn and the direct child's exit was just observed, so the group-kill window is sub-millisecond; a PID/group-id reuse inside that window would be required to harm an unrelated process — the same accepted residual risk as D-018, documented here. Windows has no process-group API: direct-child kill only, so a surviving descendant could keep using the (already removed) directory inode — a documented platform limitation.
   - **Backend-authoritative execution eligibility (D-022/D-023/D-024):** the UI confirmation captures intent; execution is authorized in Rust only. `verify_agent` records a successful backend-side `--version` verification in the eligibility ledger together with the executable's observed identity (path, size, mtime, unix inode) and the CLI's reported version; failed or missing verifications clear the grant. `start_diagnostic_run` (the ONLY production caller of `RunRegistry::start` — audited) requires an adapter that reports `supports_safe_diagnostics()`, a verification no older than `VERIFICATION_FRESHNESS` (10 min), AND a verified CLI version that meets the adapter's compatibility floor (Codex: 0.164.0 — unparseable/older fail closed). Because the confinement mechanism is documented BETA, the version floor is a compatibility guard, never a safety claim: **every start additionally runs a provider-free sandbox enforcement preflight against the verified executable** (the production policy through `codex sandbox`: positive control, protected read denied, outside write denied, invalid profile hard-errored — D-024); any violation, harness error, or timeout fails closed BEFORE run admission. The runner then re-checks, immediately before spawning, that the discovered executable is at the verified path with the verified identity — a swapped, replaced, or relocated executable fails the run before any process starts (fixture-proven, including an impostor-first-on-PATH case). The exec invocation passes `--strict-config` so unknown configuration fields hard-error instead of being silently ignored. A successful `--version` output is never treated as proof of trustworthiness; identity comparison is best-effort as far as the platform allows (a same-size, same-mtime rewrite within mtime granularity could evade detection — documented residual limitation).
   - **Atomic admission:** exactly ONE diagnostic run may be active. `RunRegistry::start` admits via `compare_exchange(false → true)` BEFORE any registry entry is created, so a rejected start never leaves a phantom entry. Admission is released through RAII on every exit path — normal completion, worker-creation failure, and worker panics — and the backend enforces the bound independently of any frontend button state.
   - **Filesystem boundary — enforced per run for Codex on macOS (D-023), disabled otherwise:** the isolated working directory guarantees WHERE the process starts (fixture-proven); what the process can READ is enforced by the provider's own sandbox, not by Orqen. For Codex (macOS, CLI ≥ 0.164.0) the runner now passes an **app-owned, read-deny-by-default permissions profile defined entirely via `-c` overrides** — `":root"="deny"`, `":minimal"="read"` (the documented platform/runtime token), `"/tmp"="deny"` (leaks through `:minimal`), `<work_dir>="write"` — selected via `default_permissions` (the top-precedence configuration layer). The profile name carries a per-run random suffix, so no entry in the user's configuration can collide with or broaden it; the user's configuration is never loaded at all (`--ignore-user-config`, `--ignore-rules`) and never modified, while authentication still resolves from `CODEX_HOME`. The legacy `--sandbox` flag is deliberately never passed — mixing it with a profile silently reverts to the legacy write-only sandbox. Enforcement is verified with the provider-free `codex sandbox` harness — the SAME policy engine `codex exec` uses — and is re-proven at every diagnostic start by the per-run preflight (D-024). The automated suite (`tests/codex_sandbox_enforcement.rs`, 12 tests) covers, with OS-denial-signature assertions so a harness failure can never masquerade as enforcement: reads of protected files in `$HOME`, explicitly under `/tmp`, and under the macOS per-user temp dir are DENIED; reads/writes inside the work dir succeed; writes outside are denied (nothing created); tool-call network access to a local listener is denied (with a positive control proving the listener is reachable unsandboxed); nested-CLI escalation cannot execute (the OUTER policy is OS-enforced on the whole process tree); invalid and unknown profiles are hard errors. The suite FAILS when `ORQEN_REQUIRE_CODEX_ENFORCEMENT=1` and no supported binary exists — otherwise tests are clearly labeled skips. **Distinction kept honest throughout the docs: these are sandbox-harness proofs of the policy engine; end-to-end `codex exec` enforcement still awaits one user-authorized real-provider run.** Documented residual: the runtime-token paths (`:minimal`) stay readable by design — that is the provider's own documented scope. Non-macOS platforms and Claude Code (inherits the user's global configuration; print mode has no read-only enforcement) report `supports_safe_diagnostics() == false` with an accurate reason surfaced in the UI from the backend DTO. No prompt instruction and not the working directory itself is treated as a security boundary.
   - **Truthful provider results:** each adapter owns machine-readable result parsing (`parse_result`). Codex uses its documented exit-code contract; Claude Code uses the documented `--output-format json` mode (implemented and fixture-tested for a future re-enable), and known SDK/config failure signals (`unrecognized_model`, `claude-code:`) classify a run as FAILED even when the process exits 0. The live `unrecognized_model` failure observed on the user's CLI remains unresolved until proven fixed with a real authenticated run ([QA_REPORT.md](QA_REPORT.md)).
   - **Starting-state cancellation:** the UI offers Cancel while the Start IPC is pending; the store records the cancel intent and cancels the run through the backend the moment the run id arrives — the late Start response never overwrites the intent, and nothing is silently ignored.
   - Agent execution beyond the diagnostic (conversational modes, coding tasks, orchestration) is a future feature with its own explicit trust decision per adapter.

Least-privilege posture (Phase 0): capabilities contain `core:default` only; CSP is set in `tauri.conf.json`; the `opener` plugin was deliberately removed (see [DECISIONS.md](DECISIONS.md) D-004/D-005).

## 4. Data flow (canonical)

1. UI event → feature component → `lib/ipc` typed wrapper.
2. Wrapper validates/normalizes input, calls `invoke(command, args)`.
3. Rust command validates input again → returns `CommandResult<T>` or a serialized `CommandErrorPayload` (`{kind, message}`).
4. Wrapper resolves typed payload or normalizes the rejection to `CommandErrorPayload`; the UI renders the real outcome (loading / success / error).

All cross-boundary payloads use camelCase JSON; their TypeScript mirrors live in `src/types/` and must change in the same commit as the Rust struct. String length limits on user input count Unicode scalar values on both sides ([DECISIONS.md](DECISIONS.md) D-012) — never bytes or UTF-16 units.

## 5. Error handling conventions

- Rust: every command returns `CommandResult<T>`; new variants are added to `CommandError` as needed, serialized as a discriminated payload ([`error.rs`](../src-tauri/src/error.rs)).
- Frontend: `normalizeCommandError` maps any rejection to `CommandErrorPayload`; UI surfaces user-presentable messages.
- Never swallow errors; never expose raw stack traces in UI.

## 6. Technology & version policy

- Pinned choices and their rationale: [DECISIONS.md](DECISIONS.md). Current stack: Tauri 2, React 19, TypeScript 6 (strict), Vite 8, Vitest 5, ESLint 10.
- Upgrades require: compatibility check against peer ranges, decision entry, full gate pass (`npm run verify` + rust gates) before merge.

## 7. Future expansion strategy

Phases (details and acceptance criteria in [ROADMAP.md](ROADMAP.md)) add modules **without changing existing boundaries**:

- Phase 2 ✅ added `db/` (SQLite via rusqlite) with commands mediating all access — the WebView never touches the database.
- Phase 3 (registry milestone) added `agents/`: static definitions, PATH discovery (never executes), and an explicit user-initiated `--version` verification probe. Execution adapters arrive later.
- Phase 3+ adds agent execution adapters as isolated child processes; the frontend learns agents only through commands.
- Phase 9 adds Telegram via a Rust-side service; notifications are read-only summaries; approvals round-trip into the same execution-control state machine.

Growth rule: a new capability either fits an existing module's ownership table or gets a new module added to the table here — before implementation.

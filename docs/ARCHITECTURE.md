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
| `app/` | Application shell, layout, top-level composition | contain feature logic |
| `features/<name>/` | One user-facing feature per folder, self-contained | import across features (extract to `lib/` instead) |
| `components/` | Shared presentational components | fetch data or hold domain state |
| `lib/ipc/` | All `invoke()` calls, payload typing, error normalization | be bypassed by feature code calling `invoke` directly |
| `lib/` | Shared frontend logic/utilities | know about specific features |
| `types/` | Cross-module domain types mirroring Rust payloads | contain logic |
| `styles/` | Global light-theme styles | encode feature-specific selectors (use component styles) |

Rust modules (`src-tauri/src/`):

| Module | Owns |
| --- | --- |
| `commands/` | Tauri command handlers — input validation, orchestration of core logic |
| `error.rs` | `CommandError` / `CommandResult` — the single error contract |
| future: `store/` | SQLite persistence (Phase 2+) |
| future: `agents/` | Adapter/runtime management (Phase 3+) |

Dependency rule: **no circular dependencies** — `commands` may use `error` and future core modules; nothing depends on `commands`. Frontend mirrors it: features depend on `lib`, never the reverse.

## 3. Trust boundaries

1. **WebView ↔ Rust core (Tauri IPC)** — the frontend is treated as semi-trusted UI: every command validates its inputs on the Rust side (see `echo_ping` in `src-tauri/src/commands/app_info.rs`), regardless of frontend checks.
2. **Rust core ↔ OS** — the core is the only privileged component. Filesystem, process, and future network access happen here, scoped by capability declarations.
3. **Orqen ↔ external AI providers/agents (future)** — provider APIs and agent processes are untrusted services. Responses are data, never instructions that override Orqen rules ([AGENTS.md §Security rules](../AGENTS.md)).
4. **User ↔ remote channel (future Telegram)** — remote approvals are second-class: high-risk/destructive actions still require in-app or explicit authorization ([GOVERNANCE.md §Execution controls](GOVERNANCE.md)).

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

- Phase 2 adds `store/` (SQLite) behind core interfaces; commands mediate all access — the WebView never touches the DB.
- Phase 3 adds `agents/` with adapters as isolated child processes; the frontend learns agents only through commands.
- Phase 9 adds Telegram via a Rust-side service; notifications are read-only summaries; approvals round-trip into the same execution-control state machine.

Growth rule: a new capability either fits an existing module's ownership table or gets a new module added to the table here — before implementation.

# Orqen

**Orqen** is a local-first AI multi-agent development workspace. It lets a user manage multiple AI coding agents from one central desktop application: appoint a Main Orchestrator, run structured project interviews, coordinate agent discussions, split work into verifiable chunks, run quality loops, and control execution through manual or autonomous approval.

> **Current status: Phase 0 — Foundation.** The desktop application shell runs and is verified. Agent capabilities are **planned, not implemented** — see [docs/STATUS.md](docs/STATUS.md) and [docs/ROADMAP.md](docs/ROADMAP.md). Nothing in the UI simulates features that do not exist.

## Technology

| Layer | Technology |
| --- | --- |
| Desktop framework | Tauri 2 |
| Frontend | React 19 + TypeScript (strict mode) |
| Build tool | Vite |
| Backend | Rust |
| Tests | Vitest + Testing Library |
| Quality | ESLint 10 (flat config), cargo fmt, clippy (`-D warnings`) |
| CI | GitHub Actions |
| Package manager | npm |

## Prerequisites

- **Node.js 24+** (developed on Node 26) and **npm 11+**
- **Rust** stable via [rustup](https://rustup.rs) (developed on 1.99)
- **macOS**: Xcode Command Line Tools (`xcode-select --install`)
- **Windows**: Microsoft Visual Studio C++ Build Tools and WebView2 (preinstalled on Windows 11)

Platform-specific setup details: [Tauri prerequisites](https://tauri.app/start/prerequisites/).

## Getting started

```bash
npm install       # install dependencies
npm run tauri dev # launch the desktop app in development mode
```

The first `tauri dev` compiles the Rust workspace and takes several minutes; subsequent launches are incremental.

## Available scripts

| Command | Purpose |
| --- | --- |
| `npm run tauri dev` | Run the desktop app with hot reload |
| `npm run dev` | Frontend-only dev server (Vite, port 1420) |
| `npm run verify` | Frontend quality gate: lint → typecheck → test → build |
| `npm run lint` / `npm run lint:fix` | ESLint |
| `npm run typecheck` | TypeScript strict checks (app + build configs) |
| `npm run test` / `npm run test:watch` | Vitest (jsdom) |
| `npm run build` | Typecheck + production frontend build |
| `npm run rust:fmt` / `rust:fmt:fix` | cargo fmt check / fix |
| `npm run rust:clippy` | clippy with `-D warnings` |
| `npm run rust:check` | cargo check |
| `npm run audit` | npm audit for production dependencies |
| `npm run tauri build` | Production desktop bundle (requires `npm run verify` to pass first) |

## Supported platforms

- **macOS** — primary development platform (Apple silicon and Intel).
- **Windows** — supported by the same codebase; the CI workflow is configured to verify frontend gates and Rust checks on Linux, but **GitHub Actions execution is currently blocked externally** by an account-level billing restriction (see [docs/QA_REPORT.md](docs/QA_REPORT.md)) — all gates are verified locally per release instead. Windows packaging verification is planned per release and documented in [docs/QA_REPORT.md](docs/QA_REPORT.md).

## Project structure

```
├── src/                    # React frontend
│   ├── app/                # Application shell, navigation model, tests
│   ├── features/           # overview · projects · agents · workflows · settings
│   ├── lib/                # ipc bridge, workspace + settings stores, contexts
│   ├── styles/             # Global tokens, shell styles, a11y helpers
│   ├── test/               # Vitest environment shims
│   └── types/              # Shared domain types
├── src-tauri/              # Rust backend
│   ├── src/commands/       # Tauri IPC commands
│   ├── src/error.rs        # Structured command errors
│   ├── capabilities/       # Least-privilege Tauri permissions
│   └── tauri.conf.json     # Window, security (CSP), bundle config
├── docs/                   # Product, architecture, workflow, governance docs
└── .github/workflows/      # CI
```

Module boundaries and planned modules: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Documentation

| Document | Authoritative for |
| --- | --- |
| [AGENTS.md](AGENTS.md) | Mandatory conduct for every AI coding agent |
| [docs/PRODUCT.md](docs/PRODUCT.md) | Product vision and functional requirements |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Technical architecture and boundaries |
| [docs/WORKFLOW.md](docs/WORKFLOW.md) | Universal agent workflow |
| [docs/GOVERNANCE.md](docs/GOVERNANCE.md) | Agent collaboration, chat modes, execution controls |
| [docs/COMMUNICATION.md](docs/COMMUNICATION.md) | Communication protocol |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Phased roadmap and acceptance criteria |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Approved architectural decisions |
| [docs/LESSONS_LEARNED.md](docs/LESSONS_LEARNED.md) | Validated failures and prevention rules |
| [docs/STATUS.md](docs/STATUS.md) | Current phase and progress |
| [docs/QA_REPORT.md](docs/QA_REPORT.md) | Real verification results |

## Current limitations (Phase 3 — registry milestone)

- The Agent Registry can **discover** installed CLIs (Codex, Claude Code), **verify** their version, and **persist your preferred Main Agent**. A **controlled diagnostic test** (a fixed noninteractive prompt, your explicit confirmation) is available for **Codex on macOS** (CLI ≥ 0.164.0): each run is confined by an app-owned, read-deny-by-default permissions profile — filesystem reads and writes outside the diagnostic directory are denied by the provider's own sandbox (verified), your global Codex configuration is not loaded and never modified, and the enforced restrictions are shown before you confirm. Claude Code diagnostics remain **disabled** (isolation not verifiable). Real agent execution for coding tasks is a future phase ([docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/ROADMAP.md](docs/ROADMAP.md)).
- Real-provider evidence: the diagnostic attempt on Claude Code 2.1.316 surfaced a local CLI configuration error (`unrecognized_model`) and was reported truthfully rather than simulated; that failure remains **unresolved** until proven fixed with a real authenticated run, and Claude Code diagnostics stay disabled meanwhile ([docs/QA_REPORT.md](docs/QA_REPORT.md)).
- Discovery means "executable found on PATH" — it says nothing about authentication, connectivity, or subscription status.
- No chat interface yet — these are later phases.
- **Persistence is live**: projects, the selected project, and settings are stored in SQLite at the OS app-data directory (`~/Library/Application Support/io.github.shahid8142.orqen/` on macOS) via the Rust core — see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the connection strategy, migrations, and backup notes (WAL side files).
- Settings for Telegram notifications and autonomous execution are saved preferences only — neither integration is operational.
- Agents and Workflows surfaces document planned capabilities; they contain no fake statuses or simulated agents.
- The Overview surface reports only real runtime data (app name/version/platform, IPC round trip); if the backend is unavailable it shows an error state.
- If the database is unreadable, Orqen fails safely: it never deletes or resets data and shows recovery guidance in the UI. Databases written by a newer Orqen version are rejected untouched.
- CI does not run a Windows job yet; Windows behavior is documented, not machine-verified in CI.
- GitHub Actions execution is blocked by an external account-level billing restriction; CI has not yet produced a successful run (status and rerun path: [docs/QA_REPORT.md](docs/QA_REPORT.md)).

## Security posture

- Least-privilege Tauri capabilities (`core:default` only — no shell, no opener, no filesystem plugins).
- Content Security Policy enforced in `tauri.conf.json`.
- Secrets are excluded by `.gitignore`; never commit credentials, tokens, or local databases.
- Security rules every contributor (human or AI) must follow: [AGENTS.md §Security rules](AGENTS.md).

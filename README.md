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
- **Windows** — supported by the same codebase; CI currently verifies Rust checks on Linux and frontend gates on Linux. Windows packaging is verified locally and documented per release (see [docs/QA_REPORT.md](docs/QA_REPORT.md)).

## Project structure

```
├── src/                    # React frontend
│   ├── app/                # Application shell
│   ├── components/         # Shared UI components
│   ├── features/           # Feature modules (foundation screen today)
│   ├── lib/                # IPC bridge, shared logic
│   ├── styles/             # Global styles (light theme)
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

## Current limitations (Phase 0)

- No agent integration, chat interface, orchestration, or persistence — these are later phases ([docs/ROADMAP.md](docs/ROADMAP.md)).
- The foundation screen reports only real runtime data (app name/version/platform, IPC round trip); if the backend is unavailable it shows an error state.
- CI does not run a Windows job yet; Windows behavior is documented, not machine-verified in CI.

## Security posture

- Least-privilege Tauri capabilities (`core:default` only — no shell, no opener, no filesystem plugins).
- Content Security Policy enforced in `tauri.conf.json`.
- Secrets are excluded by `.gitignore`; never commit credentials, tokens, or local databases.
- Security rules every contributor (human or AI) must follow: [AGENTS.md §Security rules](AGENTS.md).

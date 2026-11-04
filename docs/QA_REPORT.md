# QA_REPORT.md — Real Verification Results

Only observed results are recorded here — every entry lists the actual command and its actual outcome. Nothing in this file is projected or assumed. Last updated: 2026-11-05 (Phase 0).

## Phase 0 verification (local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Toolchain present | `git --version`, `node --version`, `npm --version`, `rustc --version`, `cargo --version` | PASS — git 2.54.0, Node 26.10.0, npm 11.19.1, Rust 1.99.1 |
| 2 | Dependencies install | `npm install` | PASS — installed cleanly |
| 3 | ESLint | `npm run lint` | PASS — 0 errors, 0 warnings |
| 4 | TypeScript strict | `npm run typecheck` (`tsc -p tsconfig.json && tsc -p tsconfig.node.json`) | PASS |
| 5 | Automated tests | `npm run test` | PASS — 11/11 tests, 2 files (App smoke ×3, IPC bridge ×8) |
| 6 | Frontend production build | `npm run build` | PASS — dist emitted (~224 KB JS, ~3.4 KB CSS) |
| 7 | Rust formatting | `npm run rust:fmt` (cargo fmt --check) | PASS |
| 8 | Rust lints | `npm run rust:clippy` (`clippy --all-targets -- -D warnings`) | PASS (after L-002 fix) |
| 9 | Rust compile | `npm run rust:check` (cargo check) | PASS |
| 10 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — binary compiled (341 crates), `target/debug/orqen` ran for the full observation window without crashing, Vite dev server answered HTTP 200. On-screen visual confirmation was not possible in this headless session (macOS screen-recording permission); the user should confirm the foundation window on first manual launch. UI rendering and IPC behavior are covered by automated tests (row 5). |
| 11 | Repository visibility | `gh repo view SHAHID8142/orqen --json visibility` | Initially PASS — `PRIVATE`; changed to `PUBLIC` on 2026-11-05 by explicit user instruction (D-010), after a secret scan confirmed no credentials in the repository |
| 12 | Secret scan before push | grep-based scan of tracked files + `git diff --cached` review | PASS — no credentials/tokens/keys found |
| 13 | Production dependency audit | `npm run audit` (`npm audit --omit=dev`) | PASS — 0 production vulnerabilities |
| 14 | CI run on GitHub | GitHub Actions `ci.yml` | **BLOCKED — account billing lock.** Observed chain: runs on the private repo (41932865723, 41933399059) ended `startup_failure` at 0s with zero jobs and no annotations. After the user directed the repository to public (D-010), run 41933672348 created both jobs but refused to start them with the annotation: *"The job was not started because your account is locked due to a billing issue."* The workflow file itself is valid: `actionlint` exit 0, GitHub lists it `active`, and jobs were instantiated. CI cannot be confirmed until the account's billing issue is resolved (github.com/settings/billing), after which any push re-triggers the workflow. |

## CI verification (GitHub Actions, ubuntu-latest)

To be filled with the observed first-run results after push (frontend job: lint → typecheck → test → build; rust job: fmt → clippy → check with Tauri Linux dependencies).

## Known accepted issues

| Issue | Severity | Assessment | Reference |
| --- | --- | --- | --- |
| 10 npm audit advisories (moderate) in dev-only ESLint tooling chain (`minimatch` ≤11 via `@eslint/config-array`, GHSA-g3hv-75wg-m249) | Moderate, **dev-only** | Accepted consciously: zero production-dependency impact; the npm-proposed fix downgrades ESLint to 9.x. Reassess each phase; revisit when the eslint 10 chain ships a patched minimatch. | [DECISIONS.md](DECISIONS.md) D-009 |
| No Windows CI job yet | Info | Documented limitation; Windows packaging verification planned Phase 11 ([ROADMAP.md](ROADMAP.md)). | [STATUS.md](STATUS.md) |
| CSP keeps `'unsafe-inline'` for styles | Info | Needed for Vite style injection; tightening deferred to hardening phase. | [DECISIONS.md](DECISIONS.md) D-005 |

## Outstanding issues

1. **CI BLOCKED (user action required):** the GitHub account is locked due to a billing issue, which prevents all Actions runs. Resolution: settle the billing matter at [github.com/settings/billing](https://github.com/settings/billing); any subsequent push re-triggers CI. Everything CI would verify (lint, typecheck, tests, build, fmt, clippy, check) has passed locally on macOS arm64 — rows 1–10 above.

## Known accepted issues

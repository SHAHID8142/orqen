# QA_REPORT.md — Real Verification Results

Only observed results are recorded here — every entry lists the actual command and its actual outcome. Nothing in this file is projected or assumed. Last updated: 2026-11-05 (Phase 0).

## Phase 0 verification (local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Toolchain present | `git --version`, `node --version`, `npm --version`, `rustc --version`, `cargo --version` | PASS — git 2.54.0, Node 26.10.0, npm 11.19.1, Rust 1.99.1 |
| 2 | Dependencies install | `npm install` | PASS — installed cleanly |
| 3 | ESLint | `npm run lint` | PASS — 0 errors, 0 warnings |
| 4 | TypeScript strict | `npm run typecheck` (`tsc -p tsconfig.json && tsc -p tsconfig.node.json`) | PASS |
| 5 | Automated tests | `npm run test` | PASS — 17/17 tests, 2 files (App smoke ×3, IPC bridge ×14 incl. Unicode regressions) |
| 6 | Frontend production build | `npm run build` | PASS — dist emitted (~224 KB JS, ~3.4 KB CSS) |
| 7 | Rust formatting | `npm run rust:fmt` (cargo fmt --check) | PASS |
| 8 | Rust lints | `npm run rust:clippy` (`clippy --all-targets -- -D warnings`) | PASS (after L-002 fix) |
| 9 | Rust compile | `npm run rust:check` (cargo check) | PASS |
| 10 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — binary compiled (341 crates), `target/debug/orqen` ran for the full observation window without crashing, Vite dev server answered HTTP 200. On-screen visual confirmation was not possible in this headless session (macOS screen-recording permission); the user should confirm the foundation window on first manual launch. UI rendering and IPC behavior are covered by automated tests (row 5). |
| 11 | Repository visibility | `gh repo view SHAHID8142/orqen --json visibility` | Initially PASS — `PRIVATE`; changed to `PUBLIC` on 2026-11-05 by explicit user instruction (D-010), after a secret scan confirmed no credentials in the repository |
| 12 | Secret scan before push | grep-based scan of tracked files + `git diff --cached` review | PASS — no credentials/tokens/keys found |
| 13 | Production dependency audit | `npm run audit` (`npm audit --omit=dev`) | PASS — 0 production vulnerabilities |
| 14 | CI run on GitHub | GitHub Actions `ci.yml` | **BLOCKED — EXTERNAL** (account-level billing restriction; evidence table in "CI verification" below). Workflow itself is valid: `actionlint` exit 0, GitHub lists it `active`, jobs instantiate once the repo is public — they are refused at start with *"The job was not started because your account is locked due to a billing issue."* CI has never reported a successful run; this is not claimed anywhere. |

## Phase 0 remediation verification (2026-11-05, local, macOS 27.2 arm64)

All results below are observed outcomes; exit codes were captured without pipes after the L-003 lesson.

| # | Check | Command | Result |
| --- | --- | --- | --- |
| R1 | Clean install | `npm ci` (after removing `node_modules`) | PASS |
| R2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; 17/17 tests; dist built |
| R3 | Rust format | `npm run rust:fmt` | PASS — exit 0 (first run FAILED on new test lines exceeding rustfmt width; fixed via `rust:fmt:fix`, re-verified) |
| R4 | Rust lints | `npm run rust:clippy` | PASS — exit 0 |
| R5 | Rust compile | `npm run rust:check` | PASS — exit 0 |
| R6 | Rust unit tests | `cargo test --manifest-path src-tauri/Cargo.toml` | PASS — 10/10, incl. Unicode regressions (150 emoji = 600 UTF-8 bytes / 150 code points accepted; 201 emoji and 201 Bangla chars rejected) |
| R7 | Production audit | `npm run audit` (`npm audit --omit=dev`) | PASS — 0 vulnerabilities |
| R8 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — `target/debug/orqen` ran through the observation window without crashing, 0 error lines in build log, Vite dev server answered HTTP 200. On-screen visual confirmation remains for the user's first manual launch. |
| R9 | Security scan | pattern scan over tracked files; sensitive-file check; capabilities review; dependency justification | CLEAN — no secrets, no tracked `.env`/key/db files, `core:default`-only capabilities, 3 justified runtime dependencies. One regex hit on `src-tauri/tauri.conf.json` line 33 was the icon filename `128x128@2x.png` (benign false positive). |
| R10 | CI status | GitHub Actions runs inspected via `gh` | BLOCKED — EXTERNAL (see CI verification section above; no successful run ever observed; not claimed) |

## CI verification (GitHub Actions)

**Status: BLOCKED — EXTERNAL (GitHub account-level billing restriction).**

Observed evidence (read from GitHub, never assumed):

| Run | Trigger | Result | Detail |
| --- | --- | --- | --- |
| 41932865723 | push 926b3be | `startup_failure`, 0s, 0 jobs | private repo; no annotations |
| 41933399059 | push a441649 | `startup_failure`, 0s, 0 jobs | private repo; no annotations |
| 41933672348 | push 7421298 | `failure`, 3s, jobs created but not started | annotation: *"The job was not started because your account is locked due to a billing issue."* |
| 41933868579 | push 021e144 | `failure`, 3s, jobs created but not started | same billing-lock annotation |

Diagnosis: the workflow file is valid — `actionlint` exit 0, GitHub lists the workflow `active`, and jobs instantiate once the repository is public — so the blocker is the **GitHub account billing restriction**, not the workflow. Per D-011 the repository stays PUBLIC and no Actions-minute purchases are required development steps; CI is simply recorded as blocked until GitHub permits execution again.

**How to rerun CI when GitHub permits Actions execution** (no code changes needed):
1. Prefer the manual path: repository → **Actions → CI → Run workflow** (`workflow_dispatch` trigger is configured).
2. Alternatively, any push to `main` re-triggers the workflow automatically.
3. Record the observed run result in this file; do not mark CI as passed without a confirmed successful run on GitHub.

## Known accepted issues

| Issue | Severity | Assessment | Reference |
| --- | --- | --- | --- |
| 10 npm audit advisories (moderate) in dev-only ESLint tooling chain (`minimatch` ≤11 via `@eslint/config-array`, GHSA-g3hv-75wg-m249) | Moderate, **dev-only** | Accepted consciously: zero production-dependency impact; the npm-proposed fix downgrades ESLint to 9.x. Reassess each phase; revisit when the eslint 10 chain ships a patched minimatch. | [DECISIONS.md](DECISIONS.md) D-009 |
| No Windows CI job yet | Info | Documented limitation; Windows packaging verification planned Phase 11 ([ROADMAP.md](ROADMAP.md)). | [STATUS.md](STATUS.md) |
| CSP keeps `'unsafe-inline'` for styles | Info | Needed for Vite style injection; tightening deferred to hardening phase. | [DECISIONS.md](DECISIONS.md) D-005 |

## Outstanding issues

1. **CI BLOCKED — EXTERNAL:** an account-level GitHub billing restriction prevents all Actions jobs from starting (evidence and rerun path in the CI verification section above). This is external to the repository; no repository change can resolve it. Everything CI would verify (lint, typecheck, tests, build, fmt, clippy, check) has passed locally on macOS arm64 — rows 1–10 above. Per D-011 no Actions-minute purchases are required development steps; CI is re-run (manually via `workflow_dispatch` or by the next push) only once GitHub permits execution.

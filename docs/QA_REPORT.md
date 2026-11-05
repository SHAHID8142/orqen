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

## Phase 3 · Feature 001 verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; **104/104 tests** |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust tests | `cargo test` | PASS — **64/64** (60 lib incl. 24 new agent/migration tests + 4 integration incl. v1→v2 upgrade) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS — real app binary ran cleanly; **real user database upgraded v1→v2 on launch** with all Phase 2 data intact (`user_version` 2, settings preserved, `main_agent_id` NULL) |
| 7 | Real CLI evidence | `command -v` + `claude --version` | Claude Code **is installed** (`/Users/maryamahad/.local/bin/claude`) and reports `2.1.316 (Claude Code)` — the same noninteractive probe the Rust verification layer runs. Codex **is not installed** — the real negative case the UI must show truthfully. |
| 8 | Secret/DB scan | `git diff` + `git ls-files` | PASS — no secrets, no tracked databases, no new npm dependencies |
| 9 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

Test-type transparency: Rust unit tests use **controlled fixture executables** (temp-dir shell scripts) for discovery and verification — no real third-party CLI is invoked in automated tests. The real Claude Code 2.1.316 evidence above comes from running the documented `--version` probe against the actually installed binary on this machine. Interactive GUI walkthrough of the Agents screen remains a manual checklist item.

## Phase 2 · Feature 001 review remediation verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; **83/83 tests** |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust tests | `cargo test` | PASS — **39/39** (37 lib + 2 integration suites) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS — real app binary with the remediated code ran cleanly against the existing database (user_version 1, data and defaults intact, 0 error lines) |
| 7 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

**Confirmed defects fixed (with regression tests):**

1. **Creation contract (Task 1):** `create_project_conn` now performs INSERT + selection UPDATE in **one SQLite transaction** and reads the returned snapshot **inside** the transaction (read-your-writes) — a commit failure rolls everything back, so a reported success always matches the database. Restores the Phase 1 contract: a newly created project is selected immediately. Regression tests: creation selects the new project (unit), the selection survives reopening (unit), and the full integration lifecycle asserts it end-to-end. Frontend backend-simulation mocks already matched this behavior and were re-verified.
2. **Mutation races (Task 2):** all workspace operations (load, create, select, delete) now run through **one serial queue** — at most one request in flight, snapshots apply strictly in issue order, so no stale response can overwrite newer state. Requests issued during an in-flight operation are queued (not silently discarded); the UI disables controls while busy; the only pre-queue rejections are client-side validation and mutations attempted while the persisted state is unavailable — both surfaced as `actionError`. Regression tests use deferred promises, including an intentionally ordered two-create sequence and a reload-issued-during-create case.
3. **Migration & transaction safety (Task 3):** databases with `user_version` **newer** than the application supports are rejected untouched (never downgraded/reset — test asserts version and data intact and a non-destructive message); foreign-key enforcement is explicitly asserted on configured connections plus an engine-level FK violation test; mutations read their snapshot inside the transaction, eliminating ambiguous partial-success reporting.
4. **Error handling (Task 4):** new `describe_error` sanitizes storage errors — technical details go to local logs (`eprintln`), user messages name the failed operation with recovery guidance, mapped by SQLite error code (unreadable file / permissions / busy). Corruption guidance remains non-destructive (asserted: no deletion suggestions in user-facing text).

**Real integration verification (Task 5)** — `src-tauri/tests/persistence_integration.rs` against the real production functions and temporary file databases: create A (auto-selected) → create B (auto-selected) → reopen (both persist, B selected) → delete selected B (selection cleared) → select A + change settings → reopen (A selected, settings persisted) → duplicate creation rejected with no data loss → rejected invalid inputs touch nothing → concurrent mutations from 4 threads with separate connections serialize safely (all 5 projects persist). GUI automation is not available in this environment; the interactive on-screen walkthrough remains a manual checklist.

## Phase 2 · Feature 001 verification (2026-11-05, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; **80/80 tests**, 7 files |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust unit tests | `cargo test` | PASS — **33/33** (db init ×4, migrations ×7, workspace commands ×5, settings ×5, plus Phase 0 IPC/Unicode ×12) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — no crash, 0 error lines, Vite HTTP 200 |
| 7 | **Real database creation** | app launch + `sqlite3` inspection | PASS — `~/Library/Application Support/io.github.shahid8142.orqen/orqen.sqlite3` created with `-wal`/`-shm` (WAL active); `PRAGMA user_version` = 1; tables `projects` + `settings`; settings defaults row (`manual`, 3, on) |
| 8 | **Restart persistence probe** | external row seeded between two real launches | PASS — probe project + selection + changed setting survived the relaunch **and were not wiped** by the app (0 error lines); probe cleaned up afterwards |
| 9 | Secret/DB tracking scan | `git status` / `git ls-files` | PASS — no database files or secrets tracked; DB lives outside the repository |
| 10 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

Rust test coverage added for Phase 2: database initialization and configuration (WAL, foreign keys); migration application, idempotent re-run, and **failure rollback** (version + schema unchanged); corrupted-database open failure **without deletion**; project CRUD through the connection-level helpers; duplicate-name rejection (unique index + friendly error); unknown-id rejection; selected-project cleared via foreign key on delete; snapshot survival across reopen; settings defaults/updates/restore/invalid-value rejection; `DbState` retry path when initialization failed.

Frontend test coverage added for Phase 2: store loading/ready/error phases with retry; failed reloads keeping the last valid state; snapshot convergence after every mutation; duplicate-request guards (load/create/update in flight); failed writes preserving previous data; client-side validation without backend calls; component-level loading/error cards with retry; header selector disabled while loading or empty; deletion confirmation with focus management preserved; settings updates, clamped retry editing, backend failures keeping previous values; truthfulness labels unchanged.

## Phase 1 · Feature 001 accessibility fix verification (2026-11-05, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; **70/70 tests**, 7 files (19 shell tests incl. 8 new containment regressions) |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust unit tests | `cargo test` | PASS — 10/10 |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — no crash, 0 error lines, Vite HTTP 200 |
| 7 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

### Real-browser keyboard verification (in-app browser engine, viewport 500×800 narrow / 1280×800 wide, app served by `npm run dev`)

jsdom cannot emulate native Tab traversal or `inert` focus semantics, so the overlay interaction model was additionally verified against a real browser engine with trusted keyboard events:

| Step | Expected | Observed |
| --- | --- | --- |
| Open drawer via toggle (keyboard Enter) | focus → first nav item ("Overview") | PASS |
| Tab forward ×4 | Projects → Agents → Workflows → Settings, never leaving the drawer | PASS |
| Tab from Settings (last nav item) | wrap into drawer → close button (✕) | PASS (after fix; **initially failed** — see below) |
| Tab from ✕ | wrap → Overview | PASS |
| Shift+Tab from Overview | wrap backwards → ✕ | PASS |
| Programmatic `focus()` on inert header/skip link | rejected (focus stays in drawer) | PASS (engine-level containment proof) |
| Escape | drawer closes, `inert` removed everywhere, focus → toggle | PASS |
| Close-button click / backdrop click | drawer closes, focus → toggle | PASS / PASS |
| Navigation selection (Projects) | drawer closes, focus → view heading (not toggle) | PASS |
| Desktop 1280×800 | nothing inert, skip link reachable, docked sidebar, no dialog role | PASS |

**Defect found and fixed by this verification:** Chromium does not complete the Tab wrap-around across `inert` regions — Tab from the drawer's last element stranded focus on `<body>` instead of returning into the drawer. Fixed with an explicit Tab/Shift+Tab boundary wrap on the drawer, overlay mode only (jsdom cannot reproduce this; see [LESSONS_LEARNED.md](LESSONS_LEARNED.md) L-014). An earlier focus-restoration defect (synchronous `focus()` on the still-`inert` toggle — a no-op in real browsers, invisible in jsdom) was also fixed by restoring focus in an effect after the commit that removes `inert`.

## Phase 1 · Feature 001 review remediation verification (2026-11-05, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; **62/62 tests**, 7 files (was 52; +10 regression tests, −1 replaced) |
| 3 | Rust format / lints / compile | `npm run rust:fmt`, `rust:clippy`, `rust:check` | PASS — exit 0 all three |
| 4 | Rust unit tests | `cargo test --manifest-path src-tauri/Cargo.toml` | PASS — 10/10 |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — ran through the observation window without crashing, 0 error lines, Vite HTTP 200 |
| 7 | CI status | GitHub Actions | BLOCKED — EXTERNAL. Current observation: after push `37949ea` (2026-11-05) **no run is created at all** — the account restriction now prevents run creation, not just job start (latest recorded run remains 42056645579 with the billing-lock annotation). Workflow remains valid (`actionlint` clean, `active`, `workflow_dispatch` enabled) for when GitHub permits execution. |

Remediation regression tests added:

- **Sidebar overlay a11y (4)**: `aria-controls` target remains in the document while hidden; focus moves to the first nav item on narrow-overlay open; backdrop click closes and restores focus to the toggle; `inert` present on main only while the narrow overlay is open. (Escape-close + focus restore was already covered.)
- **Projects delete focus (4)**: confirmation appearing moves focus to Cancel; Cancel restores focus to the item's Delete button; confirmed deletion focuses the first surviving project's button; deleting the last project focuses the name input.
- **Settings retry editing (2, replacing 1 immediate-commit test)**: free typing without mid-edit clamping, blur commits clamped value (12→10); clearing the field is allowed and an empty draft reverts instead of committing; Enter commits, Escape reverts the draft.

## Phase 1 · Feature 001 verification (2026-11-05, local, macOS 27.2 arm64)

All results are observed outcomes with unpiped exit codes.

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS |
| 2 | Frontend gate | `npm run verify` (lint → typecheck → test → build) | PASS — exit 0; **52/52 tests**, 7 files |
| 3 | Rust format | `npm run rust:fmt` | PASS — exit 0 |
| 4 | Rust lints | `npm run rust:clippy` | PASS — exit 0 |
| 5 | Rust compile | `npm run rust:check` | PASS — exit 0 |
| 6 | Rust unit tests | `cargo test --manifest-path src-tauri/Cargo.toml` | PASS — 10/10 (Phase 0 Unicode regressions still green) |
| 7 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 8 | Desktop launch | `npm run tauri dev` (backgrounded, then stopped) | PASS (process level) — app compiled and ran through the observation window without crashing, 0 error lines, Vite HTTP 200. Visual on-screen confirmation remains for the user's first manual launch. |
| 9 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

Test coverage added for Feature 001: navigation between all five surfaces without reloads; `aria-current` active states; heading focus on navigation; skip link; header project reflection; sidebar toggle `aria-expanded`; Escape closes narrow overlay and restores focus; project creation/selection/deletion with inline confirmation; project name validation (empty, duplicate, over code-point limit, Unicode names); empty states; settings changes incl. retry clamping; Phase 0 runtime diagnostics preserved on Overview.

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
| 41935624408 | push 4fd7e40 | `failure`, 4s, jobs created but not started | same billing-lock annotation |
| 42056645579 | push 62862da | `failure`, 4s, jobs created but not started | same billing-lock annotation |
| — | push 37949ea | **no run created** | restriction is intermittent |
| 42061320009 | push 5a2d59e | `failure`, 10s, jobs created but not started | same billing-lock annotation — restriction confirmed intermittent; every observed run is refused |

Diagnosis: the workflow file is valid — `actionlint` exit 0, GitHub lists the workflow `active`, and jobs instantiated while runs were still being created — so the blocker is the **GitHub account billing restriction**, not the workflow. Per D-011 the repository stays PUBLIC and no Actions-minute purchases are required development steps; CI is simply recorded as blocked until GitHub permits execution again.

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

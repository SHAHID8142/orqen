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

## Phase 3 · Feature 002 final security acceptance verification (2026-11-07, local, macOS 27.2 arm64)

| # | Check | Method | Result |
| --- | --- | --- | --- |
| 1 | Frontend gates | `npm ci` → `npm run verify` (eslint + `tsc` strict + vitest + production build) | PASS — **117/117** (new: disabled-adapter reason rendered verbatim instead of a run button) |
| 2 | Rust gates | `npm run rust:fmt` + `npm run rust:clippy` + `npm run rust:check` | PASS — fmt clean, clippy `-D warnings` 0 errors, check clean |
| 3 | Rust tests | `cargo test` | PASS — **92 lib + 9 execution-integration + 4 persistence-integration** |
| 4 | Timeout path | `cargo test --features test-timeouts` | PASS — 1 additional test (2 s override deadline, full teardown) |
| 5 | Codex read confinement (research) | official Codex sandboxing + config-reference + permissions docs (2026-11-07) | `--sandbox read-only` documents WRITE boundaries only; no read allowlist exists. The `permissions.<name>.filesystem` profiles CAN deny reads / default-deny ungranted paths, but selection is via the top-level `default_permissions` config key with no documented per-invocation flag (mixing with `--sandbox` reverts to older settings) and is version-gated (~0.138+) ⇒ enforcement would require modifying the user's global Codex config ⇒ **not enforceable and verifiable per invocation** ⇒ Codex adapter DISABLED with an accurate reason (D-022) |
| 6 | Filesystem negative tests | adapter posture tests + command-gate tests + integration `swapped_executable_is_rejected_before_spawn` / `executable_at_a_different_path_is_rejected` | PASS — an enabled diagnostic cannot exist without a verified identity; a swapped or relocated executable fails the run BEFORE any spawn (fixture output asserted absent, impostor never executed). Unauthorized reads/writes by a spawned process cannot be prevented by Orqen itself (no OS sandbox is implemented here) — the prevention is that NO diagnostic process can start at all while every adapter is disabled; this is documented as the honest boundary, not presented as read isolation |
| 7 | Backend eligibility | `EligibilityLedger` unit tests (unverified/stale/clear/unobservable), command-gate tests (`the_admission_gate_rejects_without_backend_authorization`, `disabled_adapters_cannot_start_diagnostics_even_when_verified`), runner identity re-check | PASS — direct IPC invocations cannot start a diagnostic without a fresh (≤10 min) backend-side verification; the runner re-validates path + fingerprint (size/mtime/inode) immediately before spawning; `--version` output is never treated as trustworthiness proof |
| 8 | Normal-exit descendant cleanup | integration `descendants_are_terminated_after_a_normal_exit` — fixture parent exits successfully while a `sleep 60` descendant survives and reports its PID | PASS — the run still reports `succeeded` truthfully AND the reported descendant PID no longer exists after completion (polled `ps`); the work directory is removed |
| 9 | Process-safety regressions | cancellation/timeout/retention/reader-slot integration tests re-run | PASS — cancellation prompt (no exit code = killed), retention cap 16 (17-run proof), reader-slot exhaustion fails before spawn and recovers |
| 10 | Consistency review | runner audit: duplicate result recording REMOVED (terminal result now recorded exactly once, in the worker closure); stale "re-verifies the CLI" comment corrected; cancellation honored BEFORE spawn; lock ordering (runs → order / finished / pid) verified consistent — no new deadlock surface | PASS |
| 11 | Production audit | `npm run audit` (`npm audit --omit=dev`) | PASS — 0 vulnerabilities |
| 12 | Desktop launch probe | `cargo build` + direct `target/debug/orqen` launch, observed 12 s | PASS (process level) — alive the full window, 0 error lines |
| 13 | CI status | GitHub Actions | BLOCKED — EXTERNAL (billing restriction; see CI verification section — latest observed run 42345748478) |

**Real-provider status: PARTIAL (unchanged, honestly).** No live provider run was performed in this acceptance: Codex is not installed on the development machine (`which codex` → not found), and the installed Claude Code remains (a) disabled in Orqen and (b) broken at the SDK level by the user's local CLI configuration (`unrecognized_model`, unresolved) — its shell wrapper additionally forces `--dangerously-skip-permissions`, reinforcing that it must never be invoked by Orqen. No CLIs were installed, no authentication or global configuration touched, no quota consumed, and no provider run was simulated. **A successful authenticated provider run remains REQUIRED before declaring real-provider acceptance complete.**

**Windows status: unverified (unchanged).** Cross-compilation from this macOS host remains blocked at the bundled-SQLite C build (no MSVC/mingw toolchain; installing one requires user authorization). The Unix-only API audit remains the compensating evidence. Nothing in this acceptance changes Windows behavior claims.

## Phase 3 · Feature 002 review remediation verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Method | Result |
| --- | --- | --- | --- |
| 1 | Frontend gates | `npm ci` → `npm run verify` (eslint + `tsc` strict + vitest + production build) | PASS — **116/116** frontend tests (2 new starting-state cancellation tests), production build clean |
| 2 | Rust gates | `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` (also with `--features test-timeouts`) | PASS — 0 warnings in both feature configurations |
| 3 | Rust tests (plain) | `cargo test` | PASS — 83 lib + 6 execution integration + 4 persistence integration + 0 (timeout file compiled out without the feature) |
| 4 | Rust tests (timeout path) | `cargo test --features test-timeouts` | PASS — additionally 1 timeout integration test: a hung fixture hit the 2 s override deadline, was tree-killed, reported `timed_out` with no exit code, and its work dir was removed |
| 5 | Atomic admission (backend) | `registry_starts_exactly_one_run_atomically`, `concurrent_start_requests_admit_at_most_one` (6 threads + barrier), `rejected_start_leaves_no_registered_run` + integration `simultaneous_starts_admit_exactly_one` | PASS — exactly one of six simultaneous starts admitted; rejections explain the busy state; no phantom registry entries |
| 6 | Filesystem boundary (backend) | integration `diagnostic_process_runs_inside_the_isolated_directory` — the fixture prints its own working directory | PASS — the diagnostic process ran in `<data>/diagnostic-runs/<run-id>`, and that directory was removed after the run. Scope recorded honestly: this is start-directory isolation, NOT a read sandbox (see ARCHITECTURE.md) |
| 7 | Process lifecycle (backend) | integration `cancelled_run_terminates_promptly_and_reports_cancelled` + retention/exhaustion tests | PASS — cancellation terminated the sleeping fixture promptly (no exit code = killed), reported `cancelled`, removed the work dir; finished-run retention capped at 16 (oldest pruned, verified with 17 sequential runs); reader-slot exhaustion failed a run BEFORE any spawn and the pool recovered after release |
| 8 | Truthful provider parsing (unit) | adapter fixtures: `unrecognized_model` + `claude-code:` signals with exit 0, `is_error` JSON, malformed JSON, codex exit-code contract | PASS — SDK/config failure signals classify as FAILED even when the process exits 0 |
| 9 | Windows compilation | `cargo check --target x86_64-pc-windows-msvc` from this macOS host | **BLOCKED — toolchain, not code**: fails in the `libsqlite3-sys` build script (bundled SQLite C) because no MSVC/mingw C toolchain exists on this host; the check never reaches Orqen's own code. Installing mingw-w64 would be a global system change requiring user authorization (not done). Compensating evidence: static audit — every `std::os::unix`/`libc` usage is `cfg`-gated (runner.rs, verify.rs, discovery.rs; one ungated unix-symlink test also gated). Windows runtime behavior (direct-child kill only) remains unverified until a Windows machine/CI runner exists |
| 10 | Desktop launch probe | `cargo build` + direct `target/debug/orqen` launch, observed 12 s, then stopped | PASS (process level) — alive for the full window, no error output |
| 11 | Production audit | `npm run audit` (`npm audit --omit=dev`) | PASS — 0 vulnerabilities |
| 12 | CI status | GitHub Actions | BLOCKED — EXTERNAL (billing restriction, unchanged; see CI verification section) |

**Real-provider status (unchanged, unresolved):** the live diagnostic attempt against the installed Claude Code 2.1.316 still stands as evidence — the run failed at the SDK level (`unrecognized_model`) due to the user's local CLI configuration and was recorded truthfully. A fixture is NOT proof of a live fix: Claude Code diagnostic execution therefore stays DISABLED in the app (adapter `supports_safe_diagnostics() == false`, reason surfaced to the UI), and the `unrecognized_model` failure remains listed as unresolved until a real authenticated run succeeds. Codex remains absent on this machine, so no live Codex diagnostic was possible either — the Codex path is verified by controlled fixtures only (reported as PARTIAL, not passed).

**Scope honesty:** the isolated working directory is verified as the process's start directory; it is not a confidentiality boundary. Enforced OS-level confinement exists only via the provider's own documented sandbox (Codex `--sandbox read-only`); Claude Code is disabled because no verifiable equivalent exists. No prompt instruction is presented as a security boundary anywhere.

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

## Phase 3 · Feature 002 verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` | PASS — exit 0; **114/114 tests** |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust tests | `cargo test` | PASS — **75/75** (71 lib incl. 5 adapter-contract tests + runner, 4 integration) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` | PASS — schema v2, data intact, 0 error lines |
| 7 | Real-provider acceptance: Claude Code | `claude -p "<diagnostic prompt>" --disallowedTools "Bash,Edit,Write,NotebookEdit"` against the installed 2.1.316 | **FAILED — environment, not adapter.** The CLI exits 0 but emits `[claude-code:unrecognized_model]` (SDK-level) and warns about the user's `CLAUDE_CODE_AUTO_COMPACT_WINDOW` configuration. The run output is surfaced truthfully by the diagnostic UI; no output is simulated. |
| 8 | Real-provider acceptance: Codex | `command -v codex` | ABSENT — not installed on this machine; the adapter is fixture-tested but real-CLI-unverified. |
| 9 | No unintended file modifications | runner runs in the isolated directory; sentinel checks | PASS — the runner's cwd is the app-managed isolated directory; no project paths reachable |
| 10 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

Test-type transparency: adapter-contract tests use compile-time invocation builders; execution-runner tests use **controlled fixture executables** (temp-dir shell scripts); the real-provider attempt above was performed **manually via bash** against the installed CLI (not in automated tests).

## Phase 3 · Feature 001 review remediation verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` | PASS — exit 0; **108/108 tests** |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust tests | `cargo test` | PASS — **67/67** (63 lib + 4 integration; incl. 9 verify-lifecycle tests) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` | PASS — real app binary ran cleanly (schema v2, data intact, 0 error lines) |
| 7 | Non-blocking verification | async command + `spawn_blocking` + descendant test | PASS — verification of a script with a 30s pipe-holding descendant completed in <5s (the main-thread/freezing failure mode is structurally removed: the probe runs on a blocking worker, never the main thread); **on-screen responsiveness during a slow probe requires the manual checklist** |
| 8 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

**Confirmed defects fixed (with regression tests):**

1. **Blocking Tauri commands (Task 1):** `verify_agent` and `discover_agent` are now `async` and delegate the blocking work (process spawn/poll/drain, PATH walk) to `tauri::async_runtime::spawn_blocking` — the main thread and the async executor are never blocked. IPC input validation still happens synchronously before any work is queued. No database locks are held during verification (the probe never touches `DbState`).
2. **Process lifecycle (Task 2):** the deadline now covers the entire lifecycle — after the direct child exits, output readers get a 1 s grace period; **descendants holding inherited pipes cannot hang verification** (regression test: a script with a 30 s pipe-holding descendant verifies in <5 s). Readers drain shared bounded buffers (first 64 KiB per stream kept) and are abandoned deliberately if a descendant keeps the pipes open (they exit when the pipes close; memory stays bounded). Repeated probes verified independent (5×). Timed-out children are killed and reaped; `KillOnDrop` covers normal shutdown paths — a hard process kill during an active probe can still orphan the (≤10 s) child, documented in [ARCHITECTURE.md](ARCHITECTURE.md).
3. **Windows semantics (Task 3):** the "no shell" claim is now platform-scoped — on Windows, `.cmd`/`.bat` targets execute through `cmd.exe` by OS design (Rust's hardened escaping applies; the argument list remains the single fixed `--version` literal, so no free-form input ever reaches the batch interpreter). `.exe` candidates are preferred. `#[cfg(windows)]` tests added (batch-file verification, candidate order) — **these compile on Windows only and could not be executed on this macOS machine**; marked unverified in the report.
4. **Discovery truthfulness (Task 5):** on unix, discovery now requires the execute bit (permission bits) — a non-executable same-named file is skipped (regression test); symlinks to executables resolve and broken symlinks are skipped (regression tests). Windows continues to use the extension allowlist as its (documented, weaker) signal.
5. **Main Agent UX (Task 4):** Agents screen shows settings loading ("Loading preferences…"), an actionable settings-error card with Retry, and save failures via `role="alert"` while preserving the last valid preference; the radios are disabled until settings load successfully. The settings store no longer silently drops updates — operations issued while saving are queued in order (last write wins) and unavailable-state operations surface an error. Regression tests: delayed load, failed load + retry, failed save + preservation, rapid ordered selections.

## Phase 3 · Feature 001 final remediation verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` | PASS — exit 0; **109/109 tests** |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust tests | `cargo test` | PASS — **70/70** (66 lib incl. 13 verify-lifecycle tests, 4 integration) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` | PASS — schema v2, data intact, 0 error lines |
| 7 | **Real-browser Retry verification** | in-app browser against `npm run dev` (backend absent = genuine failure state) | PASS — the error card's Retry button is enabled, **real clicks** (×2) and a real Enter keypress each re-trigger the load (error correctly persists without a backend); no page freeze |
| 8 | **Process-tree cleanup** | orphan-process check after the timeout suite | PASS — **no lingering `sleep 30` orphans** after the process-group kill (previously each timeout probe orphaned one) |
| 9 | Reader cleanup timing | `active_detached_readers()` decay tests | PASS — abandoned readers release when descendants exit (deterministic with the process-group kill) |
| 10 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

**Confirmed defects fixed:**

1. **Disabled settings Retry (Task 1):** the settings error card + Retry moved OUTSIDE the disabled preference fieldset — the Retry control is enabled exactly when the preferences are unavailable, radios stay disabled until settings load, and the full failed-load → Retry → successful-load → selection flow is covered by regression tests (jsdom) and verified with real browser clicks + keyboard (Task 5).
2. **Process lifecycle (Task 2):** the probe spawns in its own **process group** (`process_group(0)`, unix) and timeout/cleanup terminates the **whole tree** via `killpg(SIGKILL)` — an orphaned descendant can no longer hold the inherited pipes open for 30s (regression: the timeout fixture previously left a live `sleep 30` orphan; now zero lingering processes after the suite). Reader abandonment keeps bounded memory (64 KiB/stream) and every reader exit path releases the global detached counter; the counter bound (64, logged) documents the platform limitation where tree-kill is unavailable (Windows — direct child only).
3. **Stale-binary measurement trap (Lesson L-020):** during diagnosis, cargo served a **stale test binary** after source edits (external-SSD mtime granularity) — counter readings that contradicted the instrumented code were from the old leaky build. Force-rebuild (`cargo clean -p orqen`) resolved the contradiction.

## Phase 3 · Feature 001 resource-safety correction verification (2026-11-06, local, macOS 27.2 arm64)

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | Clean install | `npm ci` | PASS — exit 0 |
| 2 | Frontend gate | `npm run verify` | PASS — exit 0; 109/109 tests |
| 3 | Rust gates | `rust:fmt` / `rust:clippy` / `rust:check` | PASS — exit 0 all three |
| 4 | Rust tests | `cargo test` | PASS — **74/74** (70 lib incl. 4 new pool-admission tests + updated slot assertions; 4 integration) |
| 5 | Production audit | `npm run audit` | PASS — 0 vulnerabilities |
| 6 | Desktop launch | `npm run tauri dev` | PASS — schema v2, data intact, 0 error lines |
| 7 | ENFORCED reader limit | pool unit tests + full-pool rejection test | PASS — acquire/release round trip; exact boundary (64th acquires, 65th rejected); pair atomicity (never a half-reservation); concurrent acquisition never exceeds the limit; full-pool `verify_cli` returns `ResourceBusy` **without spawning the fixture** (sentinel file absent) |
| 8 | CI status | GitHub Actions | BLOCKED — EXTERNAL (unchanged; see CI verification section) |

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
| 42345748478 | push 45de6c1 (Feature 002 remediation) | `failure`, 4s, both jobs refused | same billing-lock annotation; 0 steps executed — observed and recorded for this remediation, not a code failure |

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

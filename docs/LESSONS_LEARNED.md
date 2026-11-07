# LESSONS_LEARNED.md — Validated Failures & Prevention Rules

Only entries with a validated root cause and an applied correction belong here. A lesson is written when a failure was actually observed, diagnosed, and fixed — not for speculative risks. Retrieval rule: agents consult this file during Research ([WORKFLOW.md](WORKFLOW.md)); shared-learning honesty limits: [GOVERNANCE.md §Shared learning](GOVERNANCE.md).

---

## L-001 | 2026-11-05 | Testing Library cleanup does not auto-run under Vitest
**What failed:** `App.test.tsx` — 2 of 3 tests failed: `waitFor` timed out because `getByTestId` matched duplicate DOM from previous tests.
**Root cause:** `@testing-library/react` auto-cleanup registers via the **global** `afterEach`; Vitest runs without globals by default, so cleanup never ran and DOM accumulated across tests.
**Correction:** explicit `afterEach(() => cleanup())` in test files using Testing Library (now in `src/app/App.test.tsx`).
**Prevention rule:** any new test file that uses Testing Library registers `cleanup()` explicitly; do not assume framework-level globals.

## L-002 | 2026-11-05 | Clippy `-D warnings` rejects speculative code
**What failed:** `cargo clippy` failed the build: `CommandError::Internal` variant was never constructed (dead code).
**Root cause:** the error enum was designed ahead of need — a speculative abstraction, violating the "no speculative code" rule.
**Correction:** removed the unused variant; the frontend still defensively normalizes unknown errors to `internal`, and Rust adds variants when a command first needs them.
**Prevention rule:** define error/code variants only when a real call site exists today; extend on first use.

## L-003 | 2026-11-05 | Piped shell commands mask exit codes
**What failed:** `npm run tauri icon appicon.png 2>&1 | tail -6 && rm -f appicon.png` — `tauri icon` failed, but the chain continued and deleted the input file, because a pipeline's exit status is its **last** command.
**Root cause:** piping to `tail` for readable output discarded the real exit status of the build tool.
**Correction:** regenerated the icon; re-ran the tool from the correct working directory (`npx tauri icon` fails outside the project because the CLI is a local devDependency).
**Prevention rule:** when a command's success matters, run it unpiped (or use `set -o pipefail`) and only then post-process output; never chain cleanup/destructive steps onto piped invocations.

## L-004 | 2026-11-05 | Strict TS flags a missing DOM matcher set
**What failed:** `tsc` failed on `toBeInTheDocument`/`toHaveTextContent` — Vitest's own assertion types do not include jest-dom matchers.
**Root cause:** chose Testing Library without its matcher library, then used jest-dom-style assertions anyway.
**Correction:** added `@testing-library/jest-dom` and imported `@testing-library/jest-dom/vitest` in the test file (decision D-006).
**Prevention rule:** when picking a test stack, decide the **assertion matcher set** together with the renderer; both are dependencies with types.

## L-005 | 2026-11-05 | Scaffold identifiers derive from local machine state
**What failed:** `create-tauri-app` generated `com.maryamahad.orqen` — the identifier embedded the local OS username (`maryamahad`), not repository ownership (`SHAHID8142`).
**Root cause:** generators default identifiers from the machine environment; unreviewed defaults leak machine-specific values.
**Correction:** identifier set deliberately to `io.github.shahid8142.orqen` (D-002); window title/product name set to `Orqen`.
**Prevention rule:** audit every generated config value before first commit; treat generator defaults as untrusted input.

## L-006 | 2026-11-05 | Tauri window `theme` values are capitalized enums
**What failed:** `tauri dev` refused to start: config schema error `"light" is not valid under any of the schemas listed in the 'anyOf' keyword` on `app > windows > 0 > theme`.
**Root cause:** wrote the theme value in lowercase; the Tauri 2 config schema requires the enum spelling `"Light"` / `"Dark"`.
**Correction:** set `"theme": "Light"`; `tauri dev` then validated and launched.
**Prevention rule:** treat `tauri.conf.json` as schema-validated — let the editor schema (or `cargo tauri dev` validation) catch enum spellings; never hand-guess enum values.

## L-007 | 2026-11-05 | GitHub Actions first push can end in `startup_failure` that cannot be retried
**What failed:** the very first push to the brand-new repository produced CI run `startup_failure` after 0s, with zero jobs, empty logs and no annotations; `gh run rerun` was refused ("This workflow run cannot be retried").
**Root cause:** `startup_failure` means the run never created jobs (infrastructure/validation error before execution). The workflow file itself was verified clean (`actionlint` exit 0; GitHub listed the workflow as `active`), matching the documented first-push infrastructure failure mode; because no jobs exist, GitHub refuses retries — only a fresh run (new push) can re-trigger CI.
**Correction:** recorded the observed state honestly, then pushed a documentation commit to trigger a fresh run and recorded the new result.
**Prevention rule:** never treat a `startup_failure` as proof of a broken workflow without local validation (`actionlint`); trigger a fresh run by pushing, and only then record CI status. Always confirm CI results on GitHub — never assume.

## L-008 | 2026-11-05 | JavaScript `String.length` counts UTF-16 units, not characters
**What failed:** a Unicode regression test asserted `"🙂".repeat(150).length === 150` (comment claiming "UTF-16 units: 1 per 🙂"); the test failed with actual value **300** — "🙂" (U+1F642) is a surrogate pair, 2 UTF-16 units.
**Root cause:** conflated "character" with UTF-16 code units while writing the test for the code-point length policy (D-012); the same confusion was the root cause of the original frontend/backend validation inconsistency (`.length` UTF-16 vs `.len()` UTF-8 bytes).
**Correction:** test rewritten to assert both facts explicitly — `message.length === 300` (UTF-16 units) and `[...message].length === 150` (code points, the shared policy) — making the old-vs-new divergence visible in the regression suite.
**Prevention rule:** in this codebase, user-input length limits always use code-point counting (`[...value].length` in TS, `chars().count()` in Rust); never assert on `.length` for non-BMP strings without noting it counts UTF-16 units.

## L-009 | 2026-11-05 | Mocked IPC tests must not assume the echo payload
**What failed:** two Unicode tests asserted `echoPing("  বাংলা…  ")` resolves to the trimmed Bangla text, but they failed with `"ping"` — the shared `beforeEach` mock resolves every `invoke` with the literal `"ping"`.
**Root cause:** the test mocked the transport, then implicitly assumed the mock would mirror command semantics (echo) that only the real backend implements.
**Correction:** tests that assert echoed content set `mockResolvedValueOnce(<expected>)` explicitly; transport-only tests keep the shared default.
**Prevention rule:** a test may assert on a mock's response only for values that test itself arranged; every assertion must trace to either the system under test or an explicitly configured mock value.

## L-010 | 2026-11-05 | jsdom does not implement `window.matchMedia`
**What failed:** every shell test failed with `TypeError: window.matchMedia is not a function` once `AppShell` started reading the narrow-width breakpoint.
**Root cause:** jsdom does not ship `matchMedia`; the component's responsive behavior depends on it.
**Correction:** added a minimal always-non-matching stub in `src/test/setup.ts` (registered via vitest `setupFiles`); the narrow-window test overrides it with `vi.spyOn(window, "matchMedia")`.
**Prevention rule:** any DOM API the app depends on that jsdom lacks gets a stub in `src/test/setup.ts` — not per-test hacks — and a test override when a specific match state is needed.

## L-011 | 2026-11-05 | Implicit accessible names from flex spans are brittle
**What failed:** `getByRole("button", { name: /^A / })` could not find a project select button whose visible text was "A" + "Created 2026-11-05" — the spans sit in a flex row with no whitespace text nodes between them, so the computed accessible name was `"ACreated 2026-11-05"`.
**Root cause:** relying on textContent concatenation for the accessible name of a button composed of multiple inline/flex spans.
**Correction:** project select buttons now set an explicit `aria-label={project.name}`; tests query by that exact name and assert selection state via `aria-pressed`.
**Prevention rule:** interactive controls with composite visible content get explicit accessible names; do not let test queries depend on incidental text-join behavior.

## L-012 | 2026-11-05 | Author `display` rules override the HTML `hidden` attribute
**What failed:** during the overlay-sidebar remediation, the sidebar was switched to an always-mounted element with the `hidden` attribute — but the existing `.app-sidebar { display: flex }` rule would have overridden the user-agent `[hidden] { display: none }` stylesheet, silently rendering the "hidden" sidebar.
**Root cause:** the `hidden` attribute only maps to a UA-stylesheet rule, which loses to any author `display` declaration of equal or higher specificity.
**Correction:** added the guard rule `.app-sidebar[hidden] { display: none; }` in `global.css`.
**Prevention rule:** when an always-mounted element is toggled with `hidden`, any author CSS that sets `display` on it must include a matching `[hidden]` display-none guard; self-audit CSS/attribute interactions before relying on UA defaults.

## L-013 | 2026-11-05 | Negative DOM assertions need `queryBy*`, not `getBy*`
**What failed:** a focus-restoration regression test asserted `expect(getByRole("group", …)).not.toBeInTheDocument()`; `getByRole` throws when the element is absent, so the test failed before its assertion could evaluate.
**Root cause:** `getBy*` queries are "must exist" queries; using one for a not-expected-to-exist assertion inverts their contract.
**Correction:** switched the negative assertion to `queryByRole(...)`.
**Prevention rule:** absence assertions always use `queryBy*` (or `findBy*` negated with waitFor); reserve `getBy*` for elements that must exist at assertion time.

## L-014 | 2026-11-05 | Chromium does not wrap keyboard focus across `inert` regions — and jsdom cannot catch it
**What failed:** in the narrow-window modal drawer, Tab from the drawer's last focusable element moved focus to `<body>` instead of wrapping back into the drawer; the drawer's own close button was skipped by the traversal. Additionally, the original close handlers called `toggleRef.focus()` synchronously — a silent no-op in real browsers because the toggle was still `inert` at that instant (state update not yet committed).
**Root cause:** two layered issues: (1) Chromium's sequential focus navigation does not complete the wrap-around across `inert` boundaries — a real-engine behavior; (2) jsdom implements neither native Tab traversal nor `inert` focus semantics, so both defects passed the entire Vitest suite and were only caught by verifying with trusted keyboard events in a real browser engine.
**Correction:** (1) explicit Tab/Shift+Tab boundary wrap on the drawer, overlay mode only (`handleDrawerKeyDown` — Tab on the last drawer element focuses the first, Shift+Tab on the first focuses the last; interior presses left to the browser); (2) close paths set a `pendingFocusRestore` flag and an effect focuses the toggle **after** the commit that removes `inert`. Verified with the full keyboard matrix in a real browser (see [QA_REPORT.md](QA_REPORT.md) accessibility-fix section).
**Prevention rule:** any `inert`-based containment needs an explicit focus-wrap mechanism — do not rely on the engine to wrap across inert boundaries; and any keyboard-affecting UI change must be verified in a real browser (trusted events), not only in jsdom. When reading focus state, compare `activeElement` tag/class — `document.body.textContent` starts with the page's first text and makes a stranded `<body>` focus look like a focused element.

## L-015 | 2026-11-06 | A UI contract not pinned by authoritative-side tests silently regresses
**What failed:** Phase 1 established "creating a project selects it" (UI announcement + behavior), but the Phase 2 Rust rewrite of `create_project_conn` inserted the project without the selection UPDATE — the frontend kept announcing "created and selected" while the backend no longer did. The mismatch survived 113 passing tests because the frontend tests mocked the backend with the correct behavior, and no Rust test pinned the selection side of the contract.
**Root cause:** the behavioral contract lived in frontend test mocks (the non-authoritative side). When the authoritative implementation was rewritten, nothing failed.
**Correction:** `create_project_conn` now performs INSERT + selection UPDATE atomically in one transaction with an in-transaction snapshot read; Rust regression tests pin the contract (creation selects, selection survives reopen, integration lifecycle covers it end-to-end).
**Prevention rule:** every cross-boundary behavioral contract must have its pinning test on the **authoritative side of the boundary** (Rust here); frontend mocks must be kept in sync with pinned backend behavior, never used as the contract's proof.

## L-016 | 2026-11-06 | Type-stripped test runs hide stale state-property references
**What failed:** after the workspace store's state shape changed (`creating`/`selectingId`/`deletingId` → single `pendingAction`), two leftover `workspace.deletingId` references in `Projects.tsx` kept rendering disabled buttons — `undefined !== null` is always true, so Cancel/Delete were permanently dead at runtime while the Vitest suite (esbuild type-stripping, no typecheck) still executed the file.
**Root cause:** Vitest transforms TypeScript without typechecking; property renames that would fail `tsc` pass silently until the typecheck gate runs — and mid-remediation runs that stopped at the lint stage never reached it.
**Correction:** replaced the stale references with the single `pendingAction` flag; the final `npm run verify` (lint → typecheck → test) passed cleanly.
**Prevention rule:** never treat a green Vitest run as type safety on a TypeScript change — `npm run verify` runs the typecheck before tests precisely for this; when renaming store state fields, grep the whole `src/` tree for the old names in the same change.

## L-017 | 2026-11-06 | A bounded reader that stops early SIGPIPE-kills the writing child
**What failed:** the CLI verification probe read child output through a thread whose `take(64 KiB)` ended early — dropping the read end of the pipe. A test child writing far more than the cap died with SIGPIPE (`code: -1`) before printing its version line, so verification reported a confusing nonzero exit instead of a usable result.
**Root cause:** "bounded read" was implemented as "stop reading at the cap" — with pipes, closing the reader end converts the writer's next write into SIGPIPE.
**Correction:** the reader now drains to EOF while keeping only the first 64 KiB (remainder discarded) — the child always completes, memory stays bounded, and the caught test pinned the behavior (version printed before the flood is preserved).
**Prevention rule:** when bounding child-process output, drain the pipe to EOF and cap only what you retain; verify flooding, hanging, and normal children with dedicated tests.

## L-018 | 2026-11-06 | cfg-gated code is not compile-checked on other platforms — and synchronous Tauri commands block the main thread
**What failed (two related review findings):** (1) a `#[cfg(windows)]` test referenced a helper function that does not exist — the file compiled cleanly on macOS because cfg-gated code is never compiled here, and the break would only appear on a Windows build; (2) `verify_agent`/`discover_agent` were synchronous Tauri commands, so blocking process execution and PATH walks ran on the main thread and could freeze the UI.
**Root cause:** platform-gated code paths escape every local compile gate; and Tauri's command threading model (sync commands → main thread) was not accounted for when the commands were written.
**Correction:** the Windows test now calls the real `candidates_for_platform` (and the `.cmd` batch-file test documents the cmd.exe boundary per D-016/D-017); both commands are `async` and delegate all blocking work to `tauri::async_runtime::spawn_blocking` — the executor is never blocked.
**Prevention rule:** cfg-gated code must be reviewed line-by-line for references that only a foreign-platform compile would catch (or compiled in CI on that platform); any Tauri command doing filesystem walks, process execution, or network I/O must be `async` + `spawn_blocking`, not merely marked async.

## L-020 | 2026-11-06 | Stale test binaries on external volumes invalidate counter-measurement
**What failed:** during remediation debugging, `cargo test` repeatedly served a **stale test binary** after `verify.rs` source edits (log showed `Finished in 0.11s` with no recompile) — counter readings (stuck at 2) came from the old, leaky build and directly contradicted the freshly instrumented source, sending diagnosis in circles. The repository lives on an external volume (`/Volumes/SSD`), where mtime granularity can defeat cargo's change detection for rapid successive edits.
**Root cause:** cargo fingerprinting relies on mtimes; coarse-grained or delayed mtime updates on the external filesystem made edited sources appear unchanged.
**Correction:** `cargo clean -p orqen` forced a full rebuild; the fresh binary confirmed the already-applied `done`-check fix worked (counter returned to 0 after each probe).
**Prevention rule:** on this machine, when test behavior contradicts the current source, force `cargo clean -p orqen` before doubting the logic — and prefer verifying fixes with a fresh compile (check the log for `Compiling orqen`).

## L-021 | 2026-11-06 | Registry/execution tests resolved a REAL agent definition — a machine with the CLI installed would have executed it
**What failed:** the run-admission unit tests called `registry.start(find_definition("codex"), …)`. The worker thread re-runs PATH discovery before spawning; on the development machine `codex` is absent so the run failed harmlessly at discovery — but on any machine WITH `codex` installed, `cargo test` would have spawned the real provider CLI (contacting the provider, consuming quota) from an automated test.
**Root cause:** using the real registry entry was convenient for adapter resolution, but "the CLI happens to be absent here" is not a safety mechanism — the test's safety must not depend on the host's installed software.
**Correction:** admission tests use a static fixture definition with the same adapter id but an `executable_name` that exists nowhere (`orqen-absent-fixture-cli`), so discovery can never resolve; lifecycle integration tests use a controlled PATH-fixture shell script. D-021 records the rule.
**Prevention rule:** any test that can reach process spawning must reference fixtures that cannot resolve to a real installed CLI on ANY machine — never the production definitions of real agents.

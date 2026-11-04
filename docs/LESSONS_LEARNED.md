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

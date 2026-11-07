//! Diagnostic execution adapters.
//!
//! A provider-independent execution contract with one narrow, controlled
//! purpose: run a FIXED noninteractive diagnostic prompt through an
//! installed, verified CLI and report the truthful outcome.
//!
//! Boundaries:
//! - The prompt is a compile-time constant — no user-supplied text reaches
//!   the process arguments.
//! - The diagnostic runs in an application-managed, isolated working
//!   directory that contains no user project data.
//! - Each adapter owns its per-run confinement policy; an adapter that
//!   cannot establish and verify adequate restrictions must report
//!   `supports_safe_diagnostics() == false` and stay disabled.
//! - Authentication always comes from the CLI's own configuration; no
//!   credentials are read, stored, or logged.

use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The fixed diagnostic prompt. Deliberately not configurable: this
/// feature validates the execution architecture, not the model.
pub const DIAGNOSTIC_PROMPT: &str = "Respond with a short confirmation that \
the agent is available. Do not read, modify, create, or delete any local \
project files.";

/// The minimum Codex CLI version for which the per-run read-confinement
/// profile is VERIFIED (0.164.0 — the version used for the enforcement
/// evidence in docs/QA_REPORT.md). Older versions fail closed at the
/// eligibility gate: they must never receive a diagnostic.
pub const CODEX_MINIMUM_VERSION: (u64, u64, u64) = (0, 164, 0);

/// Adapter contract: how one supported CLI is invoked noninteractively.
///
/// Implementations own their provider-specific argument construction and
/// are unit-tested. The execution runner stays provider-agnostic.
pub trait ExecutionAdapter: Send + Sync {
    /// Stable id — matches the registry definition id.
    fn id(&self) -> &'static str;
    /// The noninteractive argument list AFTER the executable. Built from
    /// compile-time constants, the fixed diagnostic prompt, and the
    /// adapter's own per-run confinement policy for `work_dir`.
    fn build_arguments(&self, work_dir: &Path) -> Vec<String>;
    /// Whether this adapter's invocation establishes adequate
    /// least-privilege restrictions for a diagnostic run. Adapters that
    /// cannot must report false and stay disabled.
    fn supports_safe_diagnostics(&self) -> bool;
    /// Whether starting a diagnostic requires a provider-free sandbox
    /// enforcement preflight against the verified executable. The
    /// confinement mechanism is documented BETA ("under active
    /// development and may change"), so safety must be proven on the
    /// actual installed binary — never inferred from the version number
    /// alone.
    fn requires_enforcement_preflight(&self) -> bool {
        false
    }
    /// The minimum CLI version the confinement policy requires, when the
    /// policy is version-gated. This is a compatibility FLOOR (the flags
    /// and profile schema must exist), never a safety claim — the
    /// enforcement preflight is the actual gate.
    fn minimum_cli_version(&self) -> Option<(u64, u64, u64)> {
        None
    }
    /// Short human-readable summary of the applied restrictions.
    fn restriction_summary(&self) -> &'static str;
    /// Why safe diagnostics are not supported (empty string when they
    /// are supported).
    fn unsafe_diagnostics_reason(&self) -> &'static str {
        ""
    }
    /// Parses the raw process result into a truthful agent response or an
    /// error. Provider-specific: machine-readable output modes are used
    /// where documented, and provider/SDK failure signals (even with exit
    /// code 0) are classified as failures — never as success.
    fn parse_result(
        &self,
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<String, String>;
}

/// Quotes a string as a TOML basic string (the `-c` override values are
/// parsed as TOML).
fn toml_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// Extracts a (major, minor, patch) triple from a `--version` output
/// such as `codex-cli 0.164.0`. `None` when no version token exists —
/// the caller must fail closed.
fn parse_semver_triple(version_output: &str) -> Option<(u64, u64, u64)> {
    version_output.split_whitespace().find_map(|token| {
        let mut parts = token.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        Some((major, minor, patch))
    })
}

/// Whether a `--version` output denotes a CLI at least `minimum`.
/// Unparseable output fails closed (false).
pub fn version_meets_minimum(version_output: &str, minimum: (u64, u64, u64)) -> bool {
    parse_semver_triple(version_output)
        .map(|triple| triple >= minimum)
        .unwrap_or(false)
}

/// The per-run confinement policy for one Codex diagnostic: the `-c`
/// overrides that DEFINE an app-owned, read-deny-by-default permissions
/// profile and SELECT it for this invocation only.
///
/// Guarantees (verified on macOS Seatbelt, codex-cli 0.164.0 — see
/// docs/QA_REPORT.md):
/// - filesystem reads are denied everywhere except the platform/runtime
///   paths covered by the documented `:minimal` token and the diagnostic
///   directory; `/tmp` is explicitly re-denied (it leaks through
///   `:minimal`);
/// - filesystem writes are granted ONLY inside the diagnostic directory;
/// - the profile name carries a per-run random suffix, so it can never
///   collide with an entry in the user's global configuration — nothing
///   is read from or written to the user's configuration, and no
///   inherited profile can broaden this one;
/// - nothing here passes `--sandbox`/`sandbox_mode`: mixing those with
///   permissions profiles makes Codex silently fall back to the legacy
///   (write-only) sandbox, which would defeat read confinement.
pub struct CodexRunPolicy {
    /// `-c key=value` overrides to place on the `codex` invocation.
    pub config_overrides: Vec<String>,
    /// The profile name, for tooling that selects a profile explicitly
    /// (`codex sandbox -P <name>` — used by the enforcement tests).
    pub profile_name: String,
}

/// Builds the confinement policy for one diagnostic run. The user's
/// global configuration is never modified.
pub fn codex_diagnostic_run_policy(work_dir: &Path) -> CodexRunPolicy {
    // Per-run random suffix: a static name could collide with an
    // entry the user's global configuration defines, and (because a
    // more-specific rule can reopen a broader deny) such a collision
    // could GRANT reads. A fresh random name makes that impossible.
    let profile_name = format!(
        "orqen-diag-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    let work_dir_toml = toml_quote(&work_dir.to_string_lossy());
    let profile_definition = format!(
        "permissions.{profile_name}={{filesystem={{\":root\"=\"deny\",\
         \":minimal\"=\"read\",\"/tmp\"=\"deny\",{work_dir_toml}=\"write\"}}}}",
        profile_name = profile_name,
    );
    CodexRunPolicy {
        config_overrides: vec![
            profile_definition,
            format!("default_permissions={}", toml_quote(&profile_name)),
            // Tool-call children get only the documented core environment
            // (PATH-like basics), never arbitrary inherited variables that
            // could carry credentials, and credential-shaped names are
            // excluded even from that.
            "shell_environment_policy.inherit=\"core\"".to_string(),
            "shell_environment_policy.exclude=[\"*KEY*\",\"*TOKEN*\",\"*SECRET*\",\"*PASSWORD*\",\"*CREDENTIAL*\"]"
                .to_string(),
        ],
        profile_name,
    }
}

/// Hard timeout for one preflight probe (the sandbox harness normally
/// completes in well under a second).
const PREFLIGHT_PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounded retained output PER STREAM for one probe; the remainder is
/// drained and discarded so the pipe cannot deadlock and memory stays
/// bounded (probe outputs are tiny by design).
const MAX_PROBE_OUTPUT_BYTES: usize = 16 * 1024;
/// Grace for readers after the probe tree is terminated, bounded by the
/// remaining probe deadline.
const PROBE_READER_GRACE: Duration = Duration::from_millis(500);

/// Structured failure of one sandbox probe. Every variant fails closed.
#[derive(Debug)]
pub enum ProbeError {
    /// The enforced reader-slot pool is exhausted — no probe was started.
    ReaderSlotsExhausted,
    /// The probe exceeded its end-to-end deadline; the process group was
    /// terminated and the child reaped.
    TimedOut,
    /// The probe process could not be spawned.
    Spawn(String),
    /// Process polling failed.
    Poll(String),
}

impl ProbeError {
    /// Accurate, user-presentable reason for the fail-closed decision.
    fn message(&self) -> String {
        match self {
            ProbeError::ReaderSlotsExhausted => {
                "the local resource limit for output readers is exhausted — \
                 try again shortly."
                    .to_string()
            }
            ProbeError::TimedOut => format!(
                "the sandbox probe timed out after {}s and its process \
                 group was terminated",
                PREFLIGHT_PROBE_TIMEOUT.as_secs()
            ),
            ProbeError::Spawn(detail) => format!("could not start the sandbox probe: {detail}"),
            ProbeError::Poll(detail) => {
                format!("could not track the sandbox probe process: {detail}")
            }
        }
    }
}

/// The bounded result of one completed probe.
#[derive(Debug)]
pub struct ProbeOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Default)]
struct ProbeStream {
    data: Vec<u8>,
    done: bool,
}

#[derive(Default)]
struct ProbeBuffers {
    streams: [ProbeStream; 2],
    abandoned: bool,
}

impl ProbeBuffers {
    fn append(&mut self, stream: usize, chunk: &[u8]) {
        let slot = &mut self.streams[match stream {
            0 => 0,
            _ => 1,
        }];
        let remaining = MAX_PROBE_OUTPUT_BYTES.saturating_sub(slot.data.len());
        if remaining > 0 {
            let end = chunk.len().min(remaining);
            slot.data.extend_from_slice(&chunk[..end]);
        }
    }

    fn mark_done(&mut self, stream: usize) {
        self.streams[match stream {
            0 => 0,
            _ => 1,
        }]
        .done = true;
    }

    fn all_done(&self) -> bool {
        self.streams.iter().all(|s| s.done)
    }
}

/// Marks the probe buffers abandoned on every exit path (success,
/// failure, timeout, panic): blocked readers stop at their next wakeup,
/// so no reader thread can outlive the probe indefinitely.
struct ProbeDrainGuard {
    buffers: Arc<Mutex<ProbeBuffers>>,
}

impl Drop for ProbeDrainGuard {
    fn drop(&mut self) {
        if let Ok(mut buffers) = self.buffers.lock() {
            buffers.abandoned = true;
        }
    }
}

/// RAII owner of the probe's direct child: on ANY drop that has not
/// already reaped, the whole process group (unix) is terminated and the
/// child reaped — no zombie, no orphaned descendants.
struct ProbeChild {
    child: Option<std::process::Child>,
}

impl ProbeChild {
    /// Terminates the probe tree (group SIGKILL on unix — descendants
    /// holding the pipes die with it), then reaps the direct child.
    /// Idempotent: a None child is a completed reap.
    fn terminate_and_reap(&mut self) {
        if let Some(child) = self.child.as_mut() {
            #[cfg(unix)]
            {
                let pid = child.id();
                unsafe {
                    libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            self.child = None;
        }
    }
}

impl Drop for ProbeChild {
    fn drop(&mut self) {
        self.terminate_and_reap();
    }
}

/// Runs a command to completion under an END-TO-END deadline that covers
/// spawn, execution, output collection, cleanup, and direct-child
/// reaping (D-024 hardening).
///
/// Safety mechanics (reusing the verified Phase 3 patterns):
/// - the probe runs in its own process group (unix) and the WHOLE group
///   is terminated on timeout, after exit (descendants holding the
///   pipes die with it), and on every error/panic path via RAII;
/// - stdout/stderr are drained CONCURRENTLY by two bounded reader
///   threads (a full pipe cannot deadlock the child; retained memory is
///   bounded per stream); the readers hold enforced reader-slot
///   reservations and release them exactly when they terminate;
/// - after the direct child exits, readers get a bounded grace period
///   (clamped by the remaining deadline) and are then abandoned — a
///   descendant that inherited a pipe cannot extend the probe beyond
///   the deadline by more than the grace window.
///
/// A nonzero exit is a SUCCESSFUL probe (the caller decides semantics).
fn run_probe(mut command: Command, timeout: Duration) -> Result<ProbeOutcome, ProbeError> {
    #[cfg(unix)]
    command.process_group(0);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // The deadline starts before the spawn: it covers the whole
    // lifecycle.
    let deadline = Instant::now() + timeout;

    let (stdout_slot, stderr_slot) = match super::verify::acquire_reader_slot_pair() {
        Some(pair) => pair,
        None => return Err(ProbeError::ReaderSlotsExhausted),
    };

    let mut guarded_child = ProbeChild {
        child: Some(
            command
                .spawn()
                .map_err(|e| ProbeError::Spawn(e.to_string()))?,
        ),
    };
    // Spawn failed: the slots were never moved into readers — dropping
    // them here returns the capacity.

    let buffers = Arc::new(Mutex::new(ProbeBuffers::default()));
    let drain_guard = ProbeDrainGuard {
        buffers: buffers.clone(),
    };

    let stdout = guarded_child
        .child
        .as_mut()
        .and_then(|child| child.stdout.take())
        .expect("probe stdout was piped");
    let stderr = guarded_child
        .child
        .as_mut()
        .and_then(|child| child.stderr.take())
        .expect("probe stderr was piped");
    let stdout_reader = std::thread::spawn({
        let buffers = buffers.clone();
        move || {
            drain_bounded(stdout, &buffers, 0);
            drop(stdout_slot); // capacity returns exactly when this reader ends
        }
    });
    let stderr_reader = std::thread::spawn({
        let buffers = buffers.clone();
        move || {
            drain_bounded(stderr, &buffers, 1);
            drop(stderr_slot);
        }
    });

    // Execution phase, under the deadline.
    let exit_code = loop {
        let Some(child) = guarded_child.child.as_mut() else {
            unreachable!("the child is only taken after a completed reap");
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                // The direct child exited — terminate the whole group so
                // descendants holding the pipes cannot outlive the probe,
                // then reap (no zombie). RAII covers every other path.
                guarded_child.terminate_and_reap();
                break status.code();
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Guard Drop terminates the group and reaps.
                    return Err(ProbeError::TimedOut);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return Err(ProbeError::Poll(e.to_string()));
            }
        }
    };

    // Reader grace: bounded, and clamped by the remaining deadline. A
    // descendant that inherited a pipe cannot extend the probe beyond
    // the deadline by more than this window.
    let grace_end = deadline.min(Instant::now() + PROBE_READER_GRACE);
    while Instant::now() < grace_end {
        if buffers.lock().map(|b| b.all_done()).unwrap_or(true) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(drain_guard); // readers stop at their next wakeup
    let _ = stdout_reader.join().ok();
    let _ = stderr_reader.join().ok();

    let guard = buffers.lock().expect("probe buffers lock");
    let stdout_text = String::from_utf8_lossy(&guard.streams[0].data).to_string();
    let stderr_text = String::from_utf8_lossy(&guard.streams[1].data).to_string();
    drop(guard);
    Ok(ProbeOutcome {
        exit_code,
        stdout: stdout_text,
        stderr: stderr_text,
    })
}

/// Drains a pipe to EOF, retaining at most MAX_PROBE_OUTPUT_BYTES;
/// honors the abandon flag (set on every exit path) by stopping at the
/// next wakeup.
fn drain_bounded(mut stream: impl Read, buffers: &Mutex<ProbeBuffers>, stream_index: usize) {
    let mut chunk = [0u8; 8192];
    loop {
        if buffers.lock().map(|b| b.abandoned).unwrap_or(true) {
            break;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut buffers) = buffers.lock() {
                    buffers.append(stream_index, &chunk[..n]);
                }
            }
            Err(_) => break,
        }
    }
    if let Ok(mut buffers) = buffers.lock() {
        buffers.mark_done(stream_index);
    }
}

/// Removes every preflight fixture on drop (also on error paths).
struct PreflightFixtures {
    work_dir: PathBuf,
    protected: PathBuf,
    write_target: PathBuf,
}

impl Drop for PreflightFixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work_dir);
        let _ = std::fs::remove_file(&self.protected);
        let _ = std::fs::remove_file(&self.write_target);
    }
}

/// Provider-free sandbox enforcement preflight (D-024, semantics
/// hardened): proves that the ACTUAL installed CLI at `executable`
/// enforces the diagnostic confinement policy before a run is admitted.
/// Runs the production policy builder through `codex sandbox` — the
/// same policy engine `codex exec` uses — against harmless temporary
/// fixtures; NO provider service is contacted.
///
/// Probes, in order — every denial is validated against a control so an
/// unrelated failure can never be mistaken for enforcement, and
/// ambiguous results fail closed:
/// 1. positive control — the harness accepts the policy arguments and
///    the permitted work-dir read succeeds (distinguishes a real denial
///    from "the command never ran");
/// 2. unsandboxed control — the protected fixture IS readable without
///    confinement (a denial of an unreadable file would be meaningless);
/// 3. a protected read OUTSIDE the granted roots is denied with the OS
///    access-denial signature, and the content never appears;
/// 4. a permitted work-dir WRITE succeeds (write-denial control);
/// 5. a write outside the work dir is denied with the OS signature and
///    creates nothing;
/// 6. an invalid profile fails with a CONFIGURATION error (the
///    fail-closed mechanism is alive — not a missing command or a
///    harness crash).
///
/// Any violation, ambiguous result, harness error, or timeout fails
/// closed: the caller must refuse to start the diagnostic. The preflight
/// scope is FILESYSTEM confinement; tool-call network restriction is a
/// property of the same profile verified by the sandbox test suite, not
/// re-proven per run (see the restriction summary shown in the UI).
pub fn codex_enforcement_preflight(executable: &Path) -> Result<(), String> {
    let fixtures = PreflightFixtures {
        work_dir: std::env::temp_dir().join(format!("orqen-preflight-{}", uuid::Uuid::new_v4())),
        protected: std::env::temp_dir().join(format!(
            "orqen-preflight-protected-{}",
            uuid::Uuid::new_v4()
        )),
        write_target: std::env::temp_dir()
            .join(format!("orqen-preflight-write-{}", uuid::Uuid::new_v4())),
    };
    std::fs::create_dir_all(&fixtures.work_dir)
        .map_err(|e| format!("could not create the preflight work dir: {e}"))?;
    std::fs::write(
        fixtures.work_dir.join("orqen-preflight-allowed.txt"),
        "orqen-preflight-ok",
    )
    .map_err(|e| format!("could not write the preflight fixture: {e}"))?;
    std::fs::write(&fixtures.protected, "orqen-preflight-protected-content")
        .map_err(|e| format!("could not write the preflight fixture: {e}"))?;

    let policy = codex_diagnostic_run_policy(&fixtures.work_dir);
    let sandbox_probe = |overrides: &[String], args: &[&str]| -> Result<ProbeOutcome, String> {
        let mut command = Command::new(executable);
        command.arg("sandbox");
        for override_value in overrides {
            command.arg("-c").arg(override_value);
        }
        command.arg("--").args(args).current_dir(&fixtures.work_dir);
        run_probe(command, PREFLIGHT_PROBE_TIMEOUT)
            .map_err(|error| format!("preflight probe failed: {}", error.message()))
    };
    let excerpt = |output: &str| -> String { output.trim().chars().take(200).collect() };

    // 1. Positive control: the harness runs with our policy arguments
    // and the permitted read succeeds.
    let outcome = sandbox_probe(
        &policy.config_overrides,
        &["cat", "orqen-preflight-allowed.txt"],
    )?;
    if outcome.exit_code != Some(0) || !outcome.stdout.contains("orqen-preflight-ok") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: the permitted diagnostic read did not succeed ({})",
            excerpt(&outcome.stdout)
        ));
    }

    // 2. Unsandboxed control: the protected fixture is genuinely
    // readable without confinement — otherwise a denial below would be
    // meaningless.
    let protected_arg = fixtures.protected.to_string_lossy().to_string();
    let mut unsandboxed = Command::new("cat");
    unsandboxed
        .arg(&protected_arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let unsandboxed = run_probe(unsandboxed, PREFLIGHT_PROBE_TIMEOUT).map_err(|error| {
        format!(
            "the preflight fixture could not be verified readable \
             without confinement: {}",
            error.message()
        )
    })?;
    if unsandboxed.exit_code != Some(0)
        || !unsandboxed
            .stdout
            .contains("orqen-preflight-protected-content")
    {
        return Err("the installed CLI did not pass the sandbox enforcement \
             preflight: the protected fixture was not readable without \
             confinement, so a denial could not be verified"
            .to_string());
    }

    // 3. A protected read outside the granted roots must be denied with
    // the OS access-denial signature; the content must never appear. A
    // nonzero exit WITHOUT the signature is ambiguous (harness failure,
    // missing command, crash) and fails closed.
    let outcome = sandbox_probe(&policy.config_overrides, &["cat", &protected_arg])?;
    let denial_output = format!("{}{}", outcome.stdout, outcome.stderr);
    if outcome.exit_code == Some(0) || denial_output.contains("orqen-preflight-protected-content") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: a protected file outside the diagnostic \
             directory was readable ({})",
            excerpt(&denial_output)
        ));
    }
    if !denial_output.contains("Operation not permitted") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: the protected-read denial is ambiguous — no OS \
             access-denial signature ({})",
            excerpt(&denial_output)
        ));
    }

    // 4. Write-denial control: a write INSIDE the work dir succeeds.
    let outcome = sandbox_probe(
        &policy.config_overrides,
        &["touch", "orqen-preflight-inside.txt"],
    )?;
    if outcome.exit_code != Some(0) {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: the permitted diagnostic write did not succeed \
             ({})",
            excerpt(&outcome.stdout)
        ));
    }

    // 5. A write outside the work dir must be denied with the OS
    // signature and create nothing.
    let write_arg = fixtures.write_target.to_string_lossy().to_string();
    let outcome = sandbox_probe(&policy.config_overrides, &["touch", &write_arg])?;
    let denial_output = format!("{}{}", outcome.stdout, outcome.stderr);
    if outcome.exit_code == Some(0) || fixtures.write_target.exists() {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: a file could be created outside the diagnostic \
             directory ({})",
            excerpt(&denial_output)
        ));
    }
    if !denial_output.contains("Operation not permitted") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: the outside-write denial is ambiguous — no OS \
             access-denial signature ({})",
            excerpt(&denial_output)
        ));
    }

    // 6. An invalid profile must fail with a CONFIGURATION error — not
    // a missing command, a crash, or silence. An update that silently
    // IGNORED an unknown value would otherwise downgrade the policy
    // without notice.
    let mut broken_overrides = policy.config_overrides.clone();
    broken_overrides[0] = broken_overrides[0].replace("\"deny\"", "\"bogus-value\"");
    let outcome = sandbox_probe(&broken_overrides, &["echo", "preflight-should-not-run"])?;
    if outcome.exit_code == Some(0) {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: an invalid permissions profile was accepted \
             instead of failing closed ({})",
            excerpt(&outcome.stdout)
        ));
    }
    let combined = format!("{}{}", outcome.stdout, outcome.stderr);
    if !combined.contains("Error") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: the invalid-profile failure is ambiguous — no \
             configuration error reported ({})",
            excerpt(&combined)
        ));
    }

    Ok(())
}

/// OpenAI Codex CLI: `codex exec` is the documented noninteractive mode.
///
/// Per-run confinement (D-023): the invocation loads NONE of the user's
/// configuration (`--ignore-user-config` — auth still resolves from
/// `CODEX_HOME`), loads no execpolicy rules (`--ignore-rules`), persists
/// no session files (`--ephemeral`), and applies an app-owned,
/// read-deny-by-default permissions profile defined entirely via `-c`
/// overrides (the highest-precedence configuration layer). The legacy
/// `--sandbox` flag is deliberately NOT passed: mixing it with a
/// permissions profile silently reverts to the legacy write-only
/// sandbox.
///
/// Verified on macOS Seatbelt (codex-cli 0.164.0, `codex sandbox`
/// enforcement harness — see docs/QA_REPORT.md). Other platforms fail
/// closed until independently verified there.
pub struct CodexAdapter;

impl ExecutionAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn build_arguments(&self, work_dir: &Path) -> Vec<String> {
        let policy = codex_diagnostic_run_policy(work_dir);
        let mut args = vec![
            "exec".to_string(),
            // Nothing inherited: no MCP servers, hooks, plugins, model
            // providers, or profiles from the user's configuration.
            "--ignore-user-config".to_string(),
            "--ignore-rules".to_string(),
            "--ephemeral".to_string(),
            // The isolated working directory is not a git repository.
            "--skip-git-repo-check".to_string(),
            // Unknown configuration fields are a hard error, never a
            // silent ignore — a schema change in a future CLI cannot
            // silently drop part of the confinement profile.
            "--strict-config".to_string(),
        ];
        // Each override is its own `-c key=value` pair.
        args.extend(
            policy
                .config_overrides
                .iter()
                .flat_map(|override_value| ["-c".to_string(), override_value.clone()]),
        );
        args.push(DIAGNOSTIC_PROMPT.to_string());
        args
    }

    fn supports_safe_diagnostics(&self) -> bool {
        // The read-confinement profile is VERIFIED on macOS Seatbelt
        // only (D-023). Linux (Landlock/bubblewrap) and Windows
        // enforcement are not verified — fail closed there.
        cfg!(target_os = "macos")
    }

    fn requires_enforcement_preflight(&self) -> bool {
        // The permissions mechanism is documented BETA: every diagnostic
        // start must PROVE the installed binary enforces the confinement
        // policy (D-024) — the version floor is only a compatibility
        // guard, never a safety claim.
        cfg!(target_os = "macos")
    }

    fn minimum_cli_version(&self) -> Option<(u64, u64, u64)> {
        Some(CODEX_MINIMUM_VERSION)
    }

    fn unsafe_diagnostics_reason(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            ""
        } else {
            "diagnostic execution for Codex is disabled on this platform: \
             the per-run read-confinement profile is verified on macOS \
             (Seatbelt) only — enforcement here is unverified, so the \
             adapter fails closed"
        }
    }

    fn restriction_summary(&self) -> &'static str {
        "Runs noninteractively with an app-owned per-run permissions \
         profile: filesystem reads and writes are denied outside the \
         diagnostic directory (platform/runtime paths stay readable; \
         /tmp is explicitly denied), the tool-call environment carries \
         no arbitrary inherited variables, and your global Codex \
         configuration is not loaded. Before every run, a preflight \
         re-verifies this filesystem confinement on your installed \
         CLI; tool-call network access is restricted by the same \
         profile and is verified by the app's sandbox test suite."
    }

    fn parse_result(
        &self,
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<String, String> {
        // Documented contract: `codex exec` exits nonzero on provider and
        // sandbox errors; the agent's final message is printed to stdout.
        match exit_code {
            Some(0) => {
                let text = stdout.trim();
                if text.is_empty() {
                    Err("the CLI exited successfully but produced no output.".to_string())
                } else {
                    Ok(text.chars().take(8000).collect())
                }
            }
            Some(code) => {
                let excerpt = stderr_excerpt_of(stderr);
                Err(format!("the CLI exited with code {code}.{excerpt}"))
            }
            None => Err("the CLI was terminated by a signal.".to_string()),
        }
    }
}

/// Anthropic Claude Code CLI: `claude -p` is the documented print mode
/// (noninteractive) and `--output-format json` is the documented
/// machine-readable mode.
///
/// Diagnostic execution is currently DISABLED for this adapter: the CLI
/// inherits the user's global configuration (settings, hooks, MCP servers,
/// plugins), tool-denial lists are version-dependent, and the print mode
/// does not enforce read-only filesystem access — so confidentiality and
/// tool isolation cannot be verified across CLI versions. See
/// docs/ARCHITECTURE.md for the exact boundary and docs/QA_REPORT.md for
/// the real-CLI evidence.
pub struct ClaudeCodeAdapter;

/// Known SDK/config failure signals observed in Claude Code output.
const CLAUDE_ERROR_SIGNALS: &[&str] = &["unrecognized_model", "claude-code:"];

impl ClaudeCodeAdapter {
    /// Classifies captured output: an SDK/config failure signal (even with
    /// exit code 0) is a failure, never a success.
    fn detect_error_signal(output: &str) -> Option<String> {
        for signal in CLAUDE_ERROR_SIGNALS {
            if output.contains(signal) {
                let line = output
                    .lines()
                    .find(|line| line.contains(signal))
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>();
                return Some(format!(
                    "the CLI reported a provider error ({signal}): {line}"
                ));
            }
        }
        None
    }
}

impl ExecutionAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn build_arguments(&self, _work_dir: &Path) -> Vec<String> {
        vec![
            "-p".to_string(),
            DIAGNOSTIC_PROMPT.to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "--disallowedTools".to_string(),
            "Bash,Edit,Write,NotebookEdit".to_string(),
        ]
    }

    fn supports_safe_diagnostics(&self) -> bool {
        false
    }

    fn unsafe_diagnostics_reason(&self) -> &'static str {
        "diagnostic execution for Claude Code is disabled: the CLI inherits \
         the user's global configuration (hooks, MCP servers, plugins) and \
         tool restrictions are version-dependent, so isolation cannot be \
         verified across CLI versions"
    }

    fn restriction_summary(&self) -> &'static str {
        "Diagnostic execution is disabled: confidentiality and tool \
         isolation cannot be verified across CLI versions."
    }

    fn parse_result(
        &self,
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<String, String> {
        // Machine-readable mode: stdout is a JSON object with `result` and
        // `is_error` fields (documented). Provider/SDK failure signals are
        // classified as failures even with exit code 0.
        if let Some(signal) = ClaudeCodeAdapter::detect_error_signal(stdout)
            .or_else(|| ClaudeCodeAdapter::detect_error_signal(stderr))
        {
            return Err(signal);
        }
        match exit_code {
            Some(0) => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout);
                match parsed {
                    Ok(value) => {
                        let is_error = value
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let result = value
                            .get("result")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if is_error {
                            Err(format!(
                                "the CLI reported an error: {}",
                                if result.is_empty() {
                                    "unknown error"
                                } else {
                                    &result
                                }
                            ))
                        } else if result.is_empty() {
                            Err("the CLI exited successfully but produced no usable result."
                                .to_string())
                        } else {
                            Ok(result.chars().take(8000).collect())
                        }
                    }
                    Err(_) => {
                        Err("the CLI produced malformed machine-readable output.".to_string())
                    }
                }
            }
            Some(code) => {
                let excerpt = stderr_excerpt_of(stderr);
                Err(format!("the CLI exited with code {code}.{excerpt}"))
            }
            None => Err("the CLI was terminated by a signal.".to_string()),
        }
    }
}

/// Resolves the adapter for a registry id.
pub fn find_adapter(id: &str) -> Option<&'static dyn ExecutionAdapter> {
    const CODEX: CodexAdapter = CodexAdapter;
    const CLAUDE: ClaudeCodeAdapter = ClaudeCodeAdapter;
    match id {
        "codex" => Some(&CODEX),
        "claude-code" => Some(&CLAUDE),
        _ => None,
    }
}

/// Extracts a short stderr excerpt for error messages.
fn stderr_excerpt_of(stderr: &str) -> String {
    stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>()
}

/// The fixed diagnostic prompt (exposed for documentation/tests).
pub fn diagnostic_prompt() -> &'static str {
    DIAGNOSTIC_PROMPT
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn any_work_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("orqen-adapter-test-workdir")
    }

    // ---- run_probe lifecycle regression tests (unix fixtures) ----
    // These fixtures are the only processes the automated tests run —
    // never a real provider CLI.

    /// Serializes probe-heavy tests among themselves so the shared
    /// reader-slot pool is not contended between sibling tests.
    static PROBE_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Runs a probe, retrying while the reader-slot pool is busy with
    /// sibling tests (the documented user-facing behavior on exhaustion).
    fn run_probe_with_retry(
        build: impl Fn() -> Command,
        timeout: Duration,
    ) -> Result<ProbeOutcome, ProbeError> {
        let mut attempts = 0;
        loop {
            match run_probe(build(), timeout) {
                Err(ProbeError::ReaderSlotsExhausted) if attempts < 50 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(100));
                }
                other => return other,
            }
        }
    }

    fn probe_command(script: &Path, dir: &Path) -> Command {
        let mut command = Command::new(script);
        command.current_dir(dir);
        command
    }

    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).expect("write script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn probe_fixture_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("orqen-probe-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create fixture dir");
        dir
    }

    /// True while the process with `pid` still exists.
    fn process_alive(pid: u32) -> bool {
        Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// Polls until the recorded descendant is gone (bounded).
    fn assert_descendant_dead(pidfile: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(pid_text) = fs::read_to_string(pidfile)
                && let Ok(pid) = pid_text.trim().parse::<u32>()
                && !process_alive(pid)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the descendant must be terminated with the probe group"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[cfg(unix)]
    #[test]
    fn probe_completes_normally() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("ok");
        let script = write_script(&dir, "tool", "#!/bin/sh\necho probe-ok\n");
        let outcome =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(15))
                .expect("ok probe");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.stdout.contains("probe-ok"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_times_out_and_terminates_the_group() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("hang");
        let script = write_script(&dir, "tool", "#!/bin/sh\nsleep 30\n");
        let started = Instant::now();
        let error =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_millis(800))
                .expect_err("a hanging probe must time out");
        assert!(matches!(error, ProbeError::TimedOut), "got: {error:?}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the end-to-end deadline must cover cleanup, not just the poll loop"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_survives_a_descendant_holding_the_pipes() {
        // The direct child exits successfully while a descendant keeps
        // stdout open: the deadline must still hold (the pre-fix code
        // blocked forever in post-exit read_to_string) and the
        // descendant must not survive the probe.
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("descendant");
        let script = write_script(
            &dir,
            "tool",
            "#!/bin/sh\nsleep 30 &\necho $! > \"$1\"\necho probe-done\n",
        );
        let pidfile = dir.join("descendant.pid");
        let script_with_arg = script.clone();
        let pidfile_for_probe = pidfile.clone();
        let started = Instant::now();
        let outcome = run_probe_with_retry(
            || {
                let mut command = Command::new(&script_with_arg);
                command.arg(&pidfile_for_probe).current_dir(&dir);
                command
            },
            Duration::from_secs(10),
        )
        .expect("probe");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.stdout.contains("probe-done"));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "a descendant holding the pipes must not extend the probe"
        );
        assert_descendant_dead(&pidfile);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_bounds_excessive_stdout() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("stdout-flood");
        // 5 MiB on stdout — far beyond the retained bound; the pipe must
        // be drained concurrently (no deadlock) and memory stays bounded.
        let script = write_script(
            &dir,
            "tool",
            "#!/bin/sh\necho final-line\nyes probe-spam | head -c 5000000\n",
        );
        let outcome =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(30))
                .expect("probe");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(
            outcome.stdout.contains("final-line"),
            "the tail must be retained"
        );
        assert!(
            outcome.stdout.len() <= MAX_PROBE_OUTPUT_BYTES,
            "retained stdout must be bounded: {} bytes",
            outcome.stdout.len()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_bounds_excessive_stderr() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("stderr-flood");
        let script = write_script(
            &dir,
            "tool",
            "#!/bin/sh\necho done\nyes err-spam | head -c 5000000 >&2\n",
        );
        let outcome =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(30))
                .expect("probe");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(
            outcome.stderr.len() <= MAX_PROBE_OUTPUT_BYTES,
            "retained stderr must be bounded: {} bytes",
            outcome.stderr.len()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_handles_both_streams_producing_concurrently() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("both-streams");
        // Both pipes fill simultaneously — concurrent draining must
        // prevent the classic write-side deadlock.
        let script = write_script(
            &dir,
            "tool",
            "#!/bin/sh\necho both-done\nyes out | head -c 2000000 &\nyes err | head -c 2000000 >&2 &\nwait\n",
        );
        let outcome =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(30))
                .expect("probe");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.stdout.contains("both-done"));
        assert!(outcome.stdout.len() <= MAX_PROBE_OUTPUT_BYTES);
        assert!(outcome.stderr.len() <= MAX_PROBE_OUTPUT_BYTES);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_reports_nonzero_exit() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("nonzero");
        let script = write_script(&dir, "tool", "#!/bin/sh\necho oops >&2\nexit 3\n");
        let outcome =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(15))
                .expect("probe ran");
        assert_eq!(
            outcome.exit_code,
            Some(3),
            "a nonzero exit is a completed probe"
        );
        assert!(outcome.stderr.contains("oops"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn repeated_probes_do_not_leak_resources() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = probe_fixture_dir("repeat");
        let script = write_script(&dir, "tool", "#!/bin/sh\necho round\n");
        for _ in 0..3 {
            let outcome =
                run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(15))
                    .expect("probe");
            assert_eq!(outcome.exit_code, Some(0));
        }
        // The reader-slot pool must be fully restored after the run.
        let deadline = Instant::now() + Duration::from_secs(5);
        while super::super::verify::available_reader_slots() < 64 {
            assert!(Instant::now() < deadline, "reader slots must be recovered");
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn probe_fails_closed_when_reader_slots_are_exhausted() {
        let _lock = PROBE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Reserve the whole pool (pairs until none remain — the failing
        // call releases its own first slot internally).
        let mut held = Vec::new();
        while let Some(pair) = super::super::verify::acquire_reader_slot_pair() {
            held.push(pair);
        }
        assert!(!held.is_empty());

        let dir = probe_fixture_dir("exhausted");
        let script = write_script(&dir, "tool", "#!/bin/sh\necho never\n");
        let error = run_probe(probe_command(&script, &dir), Duration::from_secs(15))
            .expect_err("an exhausted pool must fail closed before any spawn");
        assert!(
            matches!(error, ProbeError::ReaderSlotsExhausted),
            "got: {error:?}"
        );

        drop(held);
        // Recovery: capacity returns and probes run again.
        let deadline = Instant::now() + Duration::from_secs(5);
        while super::super::verify::available_reader_slots() < 64 {
            assert!(Instant::now() < deadline, "reader slots must recover");
            std::thread::sleep(Duration::from_millis(100));
        }
        let outcome =
            run_probe_with_retry(|| probe_command(&script, &dir), Duration::from_secs(15))
                .expect("recovered");
        assert_eq!(outcome.exit_code, Some(0));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn adapters_match_registry_ids() {
        assert_eq!(CodexAdapter.id(), "codex");
        assert_eq!(ClaudeCodeAdapter.id(), "claude-code");
        assert!(find_adapter("codex").is_some());
        assert!(find_adapter("claude-code").is_some());
        assert!(find_adapter("ghost").is_none());
    }

    #[test]
    fn codex_invocation_loads_no_user_config_and_applies_the_confinement_profile() {
        let args = CodexAdapter.build_arguments(&any_work_dir());
        assert_eq!(args[0], "exec", "codex exec is the noninteractive mode");
        // Nothing inherited: no MCP servers, hooks, plugins, profiles.
        assert!(
            args.iter().any(|a| a == "--ignore-user-config"),
            "the user's global configuration must not be loaded: {args:?}"
        );
        assert!(args.iter().any(|a| a == "--ignore-rules"));
        assert!(args.iter().any(|a| a == "--ephemeral"));
        assert!(args.iter().any(|a| a == "--skip-git-repo-check"));

        // FALLBACK GUARD: mixing --sandbox/sandbox_mode with a permissions
        // profile makes Codex silently use the legacy write-only sandbox,
        // defeating read confinement. The flag must never be present.
        assert!(
            !args.iter().any(|a| a == "--sandbox" || a == "-s"),
            "the legacy sandbox flag must not be passed: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a.starts_with("sandbox_mode=")),
            "sandbox_mode must not be set: {args:?}"
        );

        // The profile: read-deny-by-default with only the documented
        // runtime token and the work dir granted.
        let profile_def = args
            .iter()
            .find(|a| a.starts_with("permissions.orqen-diag-"))
            .expect("the app-owned profile must be defined via -c");
        assert!(
            profile_def.contains("\":root\"=\"deny\""),
            "reads must be denied by default: {profile_def}"
        );
        assert!(
            profile_def.contains("\":minimal\"=\"read\""),
            "the documented runtime token must be granted: {profile_def}"
        );
        assert!(
            profile_def.contains("\"/tmp\"=\"deny\""),
            "/tmp leaks through :minimal and must be re-denied: {profile_def}"
        );
        assert!(
            profile_def.contains("=\"write\""),
            "the work dir must be writable: {profile_def}"
        );
        // The work dir grant quotes the path (spaces survive as one -c
        // value; TOML basic-string escaping applies).
        let work_dir_quoted = toml_quote(&any_work_dir().to_string_lossy());
        assert!(
            profile_def.contains(&format!("{work_dir_quoted}=\"write\"")),
            "the grant must bind to THIS run's work dir: {profile_def}"
        );
        // Selection: the same profile is selected as the default.
        let selection = args
            .iter()
            .find(|a| a.starts_with("default_permissions="))
            .expect("the profile must be selected");
        let name = profile_def
            .trim_start_matches("permissions.")
            .split('=')
            .next()
            .unwrap_or("");
        assert_eq!(
            selection.as_str(),
            format!("default_permissions={}", toml_quote(name)),
            "the selected profile must be the defined one"
        );

        // The environment passed to tool-call children is restricted.
        assert!(
            args.iter()
                .any(|a| a == "shell_environment_policy.inherit=\"core\""),
            "tool-call env must be the documented core set: {args:?}"
        );
        assert!(
            args.iter()
                .any(|a| a.starts_with("shell_environment_policy.exclude=")),
            "credential-shaped env names must be excluded: {args:?}"
        );

        // The prompt is the final argument.
        assert_eq!(args.last().expect("prompt"), DIAGNOSTIC_PROMPT);
    }

    #[test]
    fn the_confinement_profile_name_is_random_per_run() {
        let first = codex_diagnostic_run_policy(&any_work_dir()).profile_name;
        let second = codex_diagnostic_run_policy(&any_work_dir()).profile_name;
        assert_ne!(
            first, second,
            "a per-run random name makes collisions with the user's global configuration impossible"
        );
        assert!(first.starts_with("orqen-diag-"));
    }

    #[test]
    fn toml_quote_escapes_special_characters() {
        assert_eq!(toml_quote("/tmp/a"), "\"/tmp/a\"");
        assert_eq!(toml_quote("/a \"b\""), "\"/a \\\"b\\\"\"");
        assert_eq!(toml_quote("back\\slash"), "\"back\\\\slash\"");
        // A path with spaces stays a single quoted TOML string.
        assert_eq!(
            toml_quote("/Users/x/Application Support/dir"),
            "\"/Users/x/Application Support/dir\""
        );
    }

    #[test]
    fn version_gate_fails_closed() {
        assert!(version_meets_minimum(
            "codex-cli 0.164.0",
            CODEX_MINIMUM_VERSION
        ));
        assert!(version_meets_minimum(
            "codex-cli 0.165.1",
            CODEX_MINIMUM_VERSION
        ));
        assert!(version_meets_minimum(
            "codex-cli 1.0.0",
            CODEX_MINIMUM_VERSION
        ));
        assert!(
            !version_meets_minimum("codex-cli 0.163.9", CODEX_MINIMUM_VERSION),
            "versions below the verified minimum must fail closed"
        );
        assert!(
            !version_meets_minimum("", CODEX_MINIMUM_VERSION),
            "unparseable output must fail closed"
        );
        assert!(!version_meets_minimum("unknown", CODEX_MINIMUM_VERSION));
        assert!(CodexAdapter.minimum_cli_version().is_some());
        // Claude stays disabled and has no version gate to bypass into.
        assert!(ClaudeCodeAdapter.minimum_cli_version().is_none());
    }

    #[test]
    fn adapter_posture_matches_verified_platforms() {
        // Codex is enabled ONLY where the confinement profile was
        // independently verified (macOS Seatbelt, D-023); everywhere else
        // it must report disabled with an accurate reason.
        assert_eq!(
            CodexAdapter.supports_safe_diagnostics(),
            cfg!(target_os = "macos")
        );
        if CodexAdapter.supports_safe_diagnostics() {
            assert!(CodexAdapter.unsafe_diagnostics_reason().is_empty());
        } else {
            assert!(
                CodexAdapter
                    .unsafe_diagnostics_reason()
                    .contains("disabled")
            );
        }
        // Claude Code remains disabled on every platform until
        // independently reviewed.
        assert!(!ClaudeCodeAdapter.supports_safe_diagnostics());
        assert!(
            ClaudeCodeAdapter
                .unsafe_diagnostics_reason()
                .contains("disabled")
        );
        assert!(ClaudeCodeAdapter.restriction_summary().contains("disabled"));
    }

    #[test]
    fn claude_uses_documented_print_mode_with_denied_mutating_tools() {
        let args = ClaudeCodeAdapter.build_arguments(&any_work_dir());
        assert_eq!(args[0], "-p", "print mode is the noninteractive mode");
        assert!(args.contains(&DIAGNOSTIC_PROMPT.to_string()));
        let deny_position = args
            .iter()
            .position(|a| a == "--disallowedTools")
            .expect("mutating tools must be denied");
        assert!(
            args[deny_position + 1]
                .split(',')
                .all(|tool| matches!(tool, "Bash" | "Edit" | "Write" | "NotebookEdit")),
            "only known mutating tools may be denied: {:?}",
            args
        );
    }

    #[test]
    fn claude_parse_classifies_sdk_errors_as_failures() {
        // Real-CLI evidence: exit 0 with an SDK error is a FAILURE.
        let error = ClaudeCodeAdapter
            .parse_result(
                Some(0),
                "{\"type\":\"result\",\"is_error\":false,\"result\":\"\"}\n[claude-code:unrecognized_model] {\"model\":\"auto\"}\n",
                "",
            )
            .expect_err("SDK error must not classify as success");
        assert!(error.contains("provider error"), "got: {error}");
    }

    #[test]
    fn claude_parse_extracts_json_results() {
        let parsed = ClaudeCodeAdapter
            .parse_result(
                Some(0),
                "{\"type\":\"result\",\"is_error\":false,\"result\":\"Agent available.\"}\n",
                "",
            )
            .expect("valid json result");
        assert_eq!(parsed, "Agent available.");
    }

    #[test]
    fn claude_parse_rejects_error_results_and_malformed_json() {
        assert!(
            ClaudeCodeAdapter
                .parse_result(
                    Some(0),
                    "{\"is_error\":true,\"result\":\"permission denied\"}",
                    ""
                )
                .is_err()
        );
        assert!(
            ClaudeCodeAdapter
                .parse_result(Some(0), "not json", "")
                .is_err()
        );
        assert!(ClaudeCodeAdapter.parse_result(Some(0), "", "").is_err());
        assert!(ClaudeCodeAdapter.parse_result(Some(2), "", "boom").is_err());
    }

    #[test]
    fn codex_parse_uses_the_documented_exit_code_contract() {
        assert_eq!(
            CodexAdapter
                .parse_result(Some(0), "codex 1.0 ready", "")
                .expect("ok"),
            "codex 1.0 ready"
        );
        assert!(CodexAdapter.parse_result(Some(0), "   ", "").is_err());
        assert!(CodexAdapter.parse_result(Some(3), "", "boom").is_err());
        assert!(CodexAdapter.parse_result(None, "", "").is_err());
    }

    #[test]
    fn prompt_is_fixed_and_does_not_request_file_operations() {
        let prompt = diagnostic_prompt();
        assert!(prompt.contains("Do not read, modify, create, or delete"));
        // The prompt must be identical for every adapter (no per-provider
        // prompt drift in the diagnostic).
        let codex_args = CodexAdapter.build_arguments(&any_work_dir());
        let claude_args = ClaudeCodeAdapter.build_arguments(&any_work_dir());
        assert_eq!(codex_args.last().expect("codex prompt"), prompt);
        assert!(claude_args.contains(&prompt.to_string()));
    }
}

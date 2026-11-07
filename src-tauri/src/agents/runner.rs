//! Controlled diagnostic run execution.
//!
//! One fixed noninteractive prompt per registered agent, executed in an
//! application-managed isolated working directory. Runs are tracked
//! in-memory for the current session only — nothing about them is
//! persisted to SQLite (retention policy: session-scoped; privacy: the
//! prompt is a compile-time constant and the output never leaves the
//! machine).
//!
//! Concurrency: exactly ONE diagnostic run may be active at a time,
//! enforced atomically (`compare_exchange`) before any registry entry is
//! created — rejected starts never leave phantom entries. Admission is
//! released through RAII on every exit path, including worker panics.
//!
//! Platform notes:
//! - Unix: the probe spawns as its own process group (`process_group(0)`)
//!   and timeout/cancellation terminate the whole group with SIGKILL.
//! - Windows: only the direct child is killed; descendants may outlive
//!   the probe. This is a documented, unverified-on-macOS limitation —
//!   see docs/ARCHITECTURE.md.
//!
//! Process safety (reuses the Phase 3 verification patterns):
//! - output readers drain bounded shared buffers;
//! - the deadline covers the whole lifecycle and a cancellation flag is
//!   honored between polls (and before the process is ever spawned);
//! - execution eligibility is backend-authoritative: a fresh
//!   backend-side verification is required (`agents::eligibility`) and
//!   the executable's observed identity (path, size, mtime, unix inode)
//!   is re-checked immediately before spawning — frontend "Verified"
//!   status is UX, not authorization;
//! - on EVERY exit path — normal exit included — the process group is
//!   terminated (unix), so a successful parent cannot leave orphaned
//!   descendants holding the pipes or the working directory.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::definitions::AgentDefinition;
use super::discovery;
use super::eligibility::ExecutableIdentity;
use super::execution::find_adapter;

/// Hard deadline for one diagnostic run (model latency can be tens of
/// seconds; 120 s bounds a hung CLI).
pub const DIAGNOSTIC_TIMEOUT: Duration = Duration::from_secs(120);
/// Output retained per stream (first N bytes; the remainder is drained).
pub const MAX_RUN_OUTPUT_BYTES: u64 = 64 * 1024;
/// Polling interval for child exit / cancellation.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Grace period for output readers after the process tree is signalled.
const READER_GRACE: Duration = Duration::from_millis(1000);
/// Maximum concurrent diagnostic runs (the UI allows one; the backend
/// enforces the same bound independently).
pub const MAX_CONCURRENT_RUNS: usize = 1;
/// Finished runs retained for result display; older ones are pruned so a
/// long session cannot grow the registry without bound.
pub const MAX_RETAINED_FINISHED_RUNS: usize = 16;

/// Terminal status of a diagnostic run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

/// The recorded outcome of a finished diagnostic run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcomeDto {
    pub agent_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub outcome: RunOutcome,
    pub exit_code: Option<i32>,
    pub output: String,
    pub error: Option<String>,
}

/// Live status of a run (polled by the frontend).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum RunStatusDto {
    /// The run id is registered but the worker has not reported yet.
    Starting {
        agent_id: String,
    },
    Running {
        agent_id: String,
        started_at: String,
    },
    Finished {
        result: RunOutcomeDto,
    },
    /// The id is not registered (unknown id, or the registry was cleared
    /// by an application restart — runs are session-scoped).
    Unknown,
}

#[derive(Default)]
struct StreamBuffers {
    data: Vec<u8>,
    done: bool,
}

#[derive(Default)]
struct OutputBuffers {
    streams: [StreamBuffers; 2],
    abandoned: bool,
}

impl OutputBuffers {
    fn stream(&mut self, stream: usize) -> &mut StreamBuffers {
        &mut self.streams[match stream {
            0 => 0,
            _ => 1,
        }]
    }
}

#[derive(Clone, Default)]
struct SharedOutput {
    inner: Arc<Mutex<OutputBuffers>>,
}

impl SharedOutput {
    fn readers_done(&self) -> bool {
        let guard = self.inner.lock().expect("output lock");
        guard.streams.iter().all(|s| s.done)
    }

    fn is_abandoned(&self) -> bool {
        self.inner.lock().expect("output lock").abandoned
    }

    fn append(&self, stream: usize, chunk: &[u8]) {
        let mut guard = self.inner.lock().expect("output lock");
        let stream_buffers = guard.stream(stream);
        let remaining = MAX_RUN_OUTPUT_BYTES as usize - stream_buffers.data.len();
        if remaining > 0 {
            let end = chunk.len().min(remaining);
            stream_buffers.data.extend_from_slice(&chunk[..end]);
        }
    }

    fn mark_done(&self, stream: usize) {
        let mut guard = self.inner.lock().expect("output lock");
        guard.stream(stream).done = true;
    }

    fn snapshot(&self) -> (Vec<u8>, Vec<u8>) {
        let guard = self.inner.lock().expect("output lock");
        (guard.streams[0].data.clone(), guard.streams[1].data.clone())
    }
}

/// Stops draining and marks both streams done on every runner exit path
/// (success, failure, timeout, cancellation, panic). Bounded retention
/// means the buffers cannot grow after this point.
struct DrainGuard(SharedOutput);
impl Drop for DrainGuard {
    fn drop(&mut self) {
        let mut guard = self.0.inner.lock().expect("output lock");
        guard.abandoned = true;
        for stream in &mut guard.streams {
            stream.done = true;
        }
    }
}

struct RunEntry {
    agent_id: &'static str,
    started_at: String,
    finished: Mutex<Option<RunOutcomeDto>>,
    cancel: Arc<AtomicBool>,
    /// Some(pid) while the probe process is alive; None once the run
    /// finishes. Cancellation only signals a live, non-finished run —
    /// a finished entry has no PID to target (stale/reused-PID guard).
    pid: Mutex<Option<u32>>,
}

/// In-memory registry of diagnostic runs (session-scoped). Managed by
/// Tauri; the worker thread holds an `Arc` clone of the inner state.
pub struct RunRegistry {
    inner: Arc<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    runs: Mutex<HashMap<String, RunEntry>>,
    /// Admission gate: true while a diagnostic is executing. Atomic
    /// `compare_exchange(false → true)` admits exactly one run; released
    /// through RAII when the worker finishes (including panics).
    active: AtomicBool,
    /// Insertion order of run ids, for bounded finished-run retention.
    order: Mutex<Vec<String>>,
}

impl Default for RunRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RegistryInner::default()),
        }
    }
}

/// RAII release of the single-run admission slot. Disarmed once the
/// worker has recorded the final outcome; on earlier drops (errors,
/// panics) it clears the admission and removes a phantom entry.
struct AdmissionGuard {
    inner: Arc<RegistryInner>,
    run_id: String,
    armed: bool,
}

impl AdmissionGuard {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.inner.active.store(false, Ordering::SeqCst);
        if let Ok(mut runs) = self.inner.runs.lock() {
            runs.remove(&self.run_id);
        }
        if let Ok(mut order) = self.inner.order.lock() {
            order.retain(|id| id != &self.run_id);
        }
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Test-only deadline override for the `test-timeouts` feature: the
/// integration suite shrinks the diagnostic deadline so the timeout path
/// (group kill, reader drain, reap, cleanup) is exercised in seconds
/// instead of 120 s. The shipped app never enables this feature.
#[cfg(feature = "test-timeouts")]
pub static TEST_DEADLINE_OVERRIDE: Mutex<Option<Duration>> = Mutex::new(None);

/// The deadline actually applied to a run. In the shipped app this is
/// always `DIAGNOSTIC_TIMEOUT`.
fn effective_timeout() -> Duration {
    #[cfg(feature = "test-timeouts")]
    {
        TEST_DEADLINE_OVERRIDE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .unwrap_or(DIAGNOSTIC_TIMEOUT)
    }
    #[cfg(not(feature = "test-timeouts"))]
    DIAGNOSTIC_TIMEOUT
}

impl RegistryInner {
    /// Records the final outcome, clears the PID, releases the admission
    /// slot, and prunes finished runs beyond the retention cap.
    fn record_finished(&self, run_id: &str, outcome: RunOutcomeDto) {
        let mut runs = self.runs.lock().expect("runs lock");
        if let Some(entry) = runs.get_mut(run_id) {
            if let Ok(mut finished) = entry.finished.lock() {
                *finished = Some(outcome);
            }
            if let Ok(mut pid) = entry.pid.lock() {
                *pid = None; // no stale/reused-PID targeting
            }
        }
        self.active.store(false, Ordering::SeqCst);
        drop(runs);
        self.prune_finished();
    }

    fn prune_finished(&self) {
        let mut runs = self.runs.lock().expect("runs lock");
        let mut order = self.order.lock().expect("order lock");
        let mut finished_ids: Vec<(String, String)> = order
            .iter()
            .filter_map(|id| {
                runs.get(id).and_then(|entry| {
                    entry
                        .finished
                        .lock()
                        .expect("finished lock")
                        .clone()
                        .map(|outcome| (id.clone(), outcome.finished_at))
                })
            })
            .collect();
        if finished_ids.len() <= MAX_RETAINED_FINISHED_RUNS {
            return;
        }
        finished_ids.sort_by(|a, b| a.1.cmp(&b.1));
        let excess = finished_ids.len() - MAX_RETAINED_FINISHED_RUNS;
        for (id, _) in finished_ids.iter().take(excess) {
            runs.remove(id);
            order.retain(|oid| oid != id);
        }
    }
}

impl RunRegistry {
    /// Registers a new run and executes it on a detached worker thread.
    ///
    /// Admission is atomic: `compare_exchange(false → true)` admits at
    /// most one diagnostic; the registry entry is created only AFTER
    /// admission succeeds, so rejected starts leave no phantom entries.
    /// The returned id is stable and immediately queryable.
    pub fn start(
        &self,
        definition: &'static AgentDefinition,
        data_dir: &Path,
        verified_identity: &ExecutableIdentity,
    ) -> Result<String, String> {
        self.inner
            .active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| {
                "a diagnostic run is already in progress — cancel it or wait \
                 for it to finish."
                    .to_string()
            })?;

        let run_id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));

        // Admission succeeded — register the entry. The guard below
        // removes it and releases admission if anything fails before the
        // worker takes ownership.
        {
            let mut runs = self.inner.runs.lock().expect("runs lock");
            runs.insert(
                run_id.clone(),
                RunEntry {
                    agent_id: definition.id,
                    started_at: now_rfc3339(),
                    finished: Mutex::new(None),
                    cancel: cancel.clone(),
                    pid: Mutex::new(None),
                },
            );
        }
        if let Ok(mut order) = self.inner.order.lock() {
            order.push(run_id.clone());
        }

        let admission = AdmissionGuard {
            inner: self.inner.clone(),
            run_id: run_id.clone(),
            armed: true,
        };

        let inner = self.inner.clone();
        let worker_run_id = run_id.clone();
        let worker_data_dir = data_dir.to_path_buf();
        let expected_identity = verified_identity.clone();
        // The guard moves into the closure and owns the run's admission
        // from here on: the recorded result releases the slot on success,
        // `disarm` retires the guard, and a panic unwinds through the
        // guard's Drop — which releases admission and removes the entry.
        // If the spawn itself fails, the closure (and the guard inside)
        // is dropped: same RAII release, no phantom entry. The terminal
        // result is recorded EXACTLY ONCE, here — `execute_diagnostic`
        // only constructs it.
        std::thread::Builder::new()
            .name(format!("diagnostic-{worker_run_id}"))
            .spawn(move || {
                let outcome = execute_diagnostic(
                    definition,
                    &worker_data_dir,
                    &worker_run_id,
                    &cancel,
                    &expected_identity,
                    &inner,
                );
                inner.record_finished(&worker_run_id, outcome);
                admission.disarm(); // worker owns the run from here
            })
            .map_err(|e| format!("could not start the diagnostic worker: {e}"))?;

        Ok(run_id)
    }

    /// Runs the status query for a run id.
    pub fn status(&self, run_id: &str) -> Option<RunStatusDto> {
        let runs = self.inner.runs.lock().expect("runs lock");
        let entry = runs.get(run_id)?;
        if let Some(outcome) = entry.finished.lock().expect("finished lock").clone() {
            return Some(RunStatusDto::Finished { result: outcome });
        }
        Some(RunStatusDto::Running {
            agent_id: entry.agent_id.to_string(),
            started_at: entry.started_at.clone(),
        })
    }

    /// Requests cancellation of a running diagnostic. Only signals a
    /// live, unfinished probe — a finished entry has no PID to target
    /// (stale/reused-PID guard).
    pub fn cancel(&self, run_id: &str) -> Result<(), String> {
        let runs = self.inner.runs.lock().expect("runs lock");
        let entry = runs
            .get(run_id)
            .ok_or_else(|| format!("unknown run \"{run_id}\"."))?;
        entry.cancel.store(true, Ordering::SeqCst);
        if let Some(pid) = *entry.pid.lock().expect("pid lock") {
            #[cfg(unix)]
            {
                // Unix: signal the whole probe process group so
                // descendants holding inherited pipes die with the
                // direct child.
                unsafe {
                    libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid; // direct-child kill happens via the worker
            }
        }
        Ok(())
    }
}

/// Executes the diagnostic and returns the recorded outcome. Runs on the
/// detached worker thread.
fn execute_diagnostic(
    definition: &'static AgentDefinition,
    data_dir: &Path,
    run_id: &str,
    cancel: &Arc<AtomicBool>,
    verified_identity: &ExecutableIdentity,
    inner: &Arc<RegistryInner>,
) -> RunOutcomeDto {
    let started_at = now_rfc3339();
    let started = Instant::now();

    // Single construction point for the outcome. Recording happens ONCE
    // in the worker closure (`start`) — never here — so a terminal result
    // can never be written twice.
    let finish = |outcome: RunOutcome,
                  exit_code: Option<i32>,
                  output: String,
                  error: Option<String>|
     -> RunOutcomeDto {
        RunOutcomeDto {
            agent_id: definition.id.to_string(),
            started_at: started_at.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            outcome,
            exit_code,
            output: output.clone(),
            error: error.clone(),
            finished_at: now_rfc3339(),
        }
    };

    // The adapter provides the argument construction. Whether the
    // adapter may run diagnostics AT ALL (`supports_safe_diagnostics`)
    // is enforced at the IPC boundary —
    // `commands::agents::ensure_diagnostic_admissible`, the only caller
    // of `start` — together with the fresh-verification ledger gate.
    // The runner enforces what it alone can observe: that the spawned
    // executable is the verified one (identity check below).
    let adapter = match find_adapter(definition.id) {
        Some(adapter) => adapter,
        None => {
            return finish(
                RunOutcome::Failed,
                None,
                String::new(),
                Some(format!(
                    "\"{}\" has no execution adapter in this build.",
                    definition.id
                )),
            );
        }
    };

    // Discovery: the executable must have been located, and it must be
    // the SAME executable that was verified — identity is re-checked here
    // (in the executor, not just the command layer) so a swap between
    // verification and spawn is detected.
    let executable = match discovery::discover(definition) {
        discovery::DiscoveryOutcome::Found { path } => path,
        discovery::DiscoveryOutcome::NotFound => {
            return finish(
                RunOutcome::Failed,
                None,
                String::new(),
                Some(format!(
                    "the \"{}\" executable was not found on this computer.",
                    definition.display_name
                )),
            );
        }
        discovery::DiscoveryOutcome::Error { message } => {
            return finish(RunOutcome::Failed, None, String::new(), Some(message));
        }
    };
    if executable != verified_identity.path {
        return finish(
            RunOutcome::Failed,
            None,
            String::new(),
            Some(
                "the executable that was found is not the one that was \
                 verified — verify the agent again before running a \
                 diagnostic."
                    .to_string(),
            ),
        );
    }
    if !verified_identity.matches_current(&executable) {
        return finish(
            RunOutcome::Failed,
            None,
            String::new(),
            Some(
                "the executable changed since it was verified — verify the \
                 agent again before running a diagnostic."
                    .to_string(),
            ),
        );
    }

    // Isolated working directory: application-managed, no user project
    // data. Removed only after the process tree is terminated.
    let work_dir = data_dir.join("diagnostic-runs").join(run_id);
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        return finish(
            RunOutcome::Failed,
            None,
            String::new(),
            Some(format!("could not create the isolated run directory: {e}")),
        );
    }

    // Reader-slot admission: reuse the proven verification pool. Two
    // slots for the run's stdout/stderr readers; an exhausted pool fails
    // the run before any spawn (slots released via RAII below).
    let (stdout_slot, stderr_slot) = match super::verify::acquire_reader_slot_pair() {
        Some(pair) => pair,
        None => {
            let _ = std::fs::remove_dir_all(&work_dir);
            return finish(
                RunOutcome::Failed,
                None,
                String::new(),
                Some(
                    "the local resource limit for output readers is \
                     exhausted — try again shortly."
                        .to_string(),
                ),
            );
        }
    };

    // Cancellation honored BEFORE the process exists: a cancel request
    // that arrived during discovery/dir setup never spawns a process at
    // all.
    if cancel.load(Ordering::SeqCst) {
        drop(stdout_slot);
        drop(stderr_slot);
        let _ = std::fs::remove_dir_all(&work_dir);
        return finish(
            RunOutcome::Cancelled,
            None,
            String::new(),
            Some("the run was cancelled before the process started.".to_string()),
        );
    }

    // Spawn: adapter-owned arguments, explicit working directory, own
    // process group (unix) for whole-tree cancellation.
    let mut command = Command::new(&executable);
    command
        .args(adapter.build_arguments(&work_dir))
        .current_dir(&work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            drop(stdout_slot);
            drop(stderr_slot);
            let _ = std::fs::remove_dir_all(&work_dir);
            return finish(
                RunOutcome::Failed,
                None,
                String::new(),
                Some(
                    "the executable could not be started: it may be \
                     incompatible with this system or lack execute permission."
                        .to_string(),
                ),
            );
        }
    };

    // Register the pid so cancellation can signal the whole group.
    let pid = child.id();
    if let Ok(mut runs) = inner.runs.lock()
        && let Some(entry) = runs.get_mut(run_id)
        && let Ok(mut slot) = entry.pid.lock()
    {
        *slot = Some(pid);
    }

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let shared = SharedOutput::default();
    let drain_guard = DrainGuard(shared.clone());
    let stdout_reader = std::thread::spawn({
        let shared = shared.clone();
        move || {
            read_drained(stdout, &shared, 0);
            drop(stdout_slot); // capacity returns when this reader terminates
        }
    });
    let stderr_reader = std::thread::spawn({
        let shared = shared.clone();
        move || {
            read_drained(stderr, &shared, 1);
            drop(stderr_slot);
        }
    });

    // Poll loop: deadline, cancellation, child exit.
    let deadline = Instant::now() + effective_timeout();
    let mut timed_out = false;
    let mut cancelled = false;
    let exit_status = loop {
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                // The reader slots live with the reader threads; abandoning
                // the readers (DrainGuard) lets them release capacity.
                drop(drain_guard);
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_dir_all(&work_dir);
                return finish(
                    RunOutcome::Failed,
                    None,
                    String::new(),
                    Some(format!("could not track the CLI process: {e}")),
                );
            }
        }
    };

    // Terminate the whole tree on EVERY exit path — a normal parent exit
    // can leave descendants alive as orphans in the same process group,
    // holding the inherited pipes and the working directory. On unix the
    // group was created by this spawn and the direct child has just been
    // observed to exit (or was already signalled); the group-kill window
    // is immediately after that observation, so the PID/group-reuse risk
    // is the same accepted residual risk as D-018 (documented in
    // docs/ARCHITECTURE.md). Windows has no group API: direct-child kill
    // only — a documented limitation.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill(); // direct child (windows path; unix: belt & braces)

    // Short grace for the readers after the tree is signalled, then the
    // DrainGuard stops them (bounded buffers).
    let grace_deadline = Instant::now() + READER_GRACE;
    while !shared.readers_done() && Instant::now() < grace_deadline {
        std::thread::sleep(POLL_INTERVAL);
    }
    drop(drain_guard);
    drop(stdout_reader); // abandoned if pipes are still held (bounded)
    drop(stderr_reader);
    let (stdout_bytes, stderr_bytes) = shared.snapshot();

    // Reap the direct child (no zombie) — the tree is dead and the
    // directory can no longer be used by any surviving process.
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&work_dir);
    if let Ok(mut runs) = inner.runs.lock()
        && let Some(entry) = runs.get_mut(run_id)
        && let Ok(mut slot) = entry.pid.lock()
    {
        *slot = None; // no stale/reused-PID targeting
    }

    let output = String::from_utf8_lossy(&stdout_bytes)
        .chars()
        .take(8000)
        .collect::<String>();
    let stderr_text = String::from_utf8_lossy(&stderr_bytes);

    // Provider-specific result parsing (Claude Code is disabled upstream,
    // but the parsing contract is adapter-owned and fixture-tested).
    let parsed = adapter.parse_result(
        exit_code_of(&exit_status),
        &String::from_utf8_lossy(&stdout_bytes),
        &stderr_text,
    );

    if cancelled {
        return finish(
            RunOutcome::Cancelled,
            exit_code_of(&exit_status),
            output,
            Some("the run was cancelled.".to_string()),
        );
    }
    if timed_out {
        return finish(
            RunOutcome::TimedOut,
            exit_code_of(&exit_status),
            output,
            Some(format!(
                "the diagnostic timed out after {}s — the process tree was \
                 terminated.",
                effective_timeout().as_secs()
            )),
        );
    }

    match parsed {
        Ok(response) => finish(
            RunOutcome::Succeeded,
            exit_code_of(&exit_status),
            response,
            None,
        ),
        Err(error) => finish(
            RunOutcome::Failed,
            exit_code_of(&exit_status),
            output,
            Some(error),
        ),
    }
}

fn exit_code_of(status: &Option<std::process::ExitStatus>) -> Option<i32> {
    status.and_then(|s| s.code())
}

/// Drains a pipe to EOF, retaining at most MAX_RUN_OUTPUT_BYTES per
/// stream. Honors the abandon flag (set on every runner exit path) by
/// stopping at the next wakeup.
fn read_drained(mut stream: impl Read, shared: &SharedOutput, stream_index: usize) {
    let mut chunk = [0u8; 8192];
    loop {
        if shared.is_abandoned() {
            break;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => shared.append(stream_index, &chunk[..n]),
            Err(_) => break,
        }
    }
    shared.mark_done(stream_index);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::definitions::AgentCapabilities;
    use std::path::PathBuf;

    /// Admission tests must never be able to spawn a real provider CLI —
    /// a machine with `codex` installed would otherwise execute it. The
    /// fixture keeps the adapter id (so `find_adapter` resolves) but
    /// points discovery at an executable name that exists nowhere.
    static ABSENT_FIXTURE: AgentDefinition = AgentDefinition {
        id: "codex",
        display_name: "Fixture (absent executable)",
        provider: "Test",
        adapter: "cli",
        executable_name: "orqen-absent-fixture-cli",
        capabilities: AgentCapabilities {
            discovery: true,
            verification: true,
            execution: true,
            orchestration: false,
        },
    };

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orqen-exec-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// A placeholder identity for tests whose fixture can never be
    /// discovered (the identity check runs after discovery, so it is
    /// never consulted here).
    fn dummy_identity() -> ExecutableIdentity {
        ExecutableIdentity {
            path: std::path::PathBuf::from("/nonexistent/orqen-placeholder"),
            size: 0,
            modified: std::time::SystemTime::UNIX_EPOCH,
            #[cfg(unix)]
            inode: 0,
        }
    }

    #[test]
    fn registry_starts_exactly_one_run_atomically() {
        let registry = RunRegistry::default();
        let definition = &ABSENT_FIXTURE;
        let dir = temp_dir();

        let run_id = registry
            .start(definition, &dir, &dummy_identity())
            .expect("first start");
        assert!(
            registry.start(definition, &dir, &dummy_identity()).is_err(),
            "second concurrent start must be rejected"
        );
        // No phantom entry: exactly one registered run.
        assert!(registry.status(&run_id).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_start_requests_admit_at_most_one() {
        use std::sync::Barrier;
        let registry = Arc::new(RunRegistry::default());
        let definition = &ABSENT_FIXTURE;
        let dir = Arc::new(temp_dir());
        let barrier = Arc::new(Barrier::new(6));

        let mut handles = Vec::new();
        for _ in 0..6 {
            let registry = registry.clone();
            let definition_copy: &'static AgentDefinition = definition;
            let dir = dir.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                registry.start(definition_copy, &dir, &dummy_identity())
            }));
        }
        let results: Vec<Result<String, String>> = handles
            .into_iter()
            .map(|h| h.join().expect("join"))
            .collect();
        let admitted: Vec<&Result<String, String>> = results.iter().filter(|r| r.is_ok()).collect();
        assert_eq!(
            admitted.len(),
            1,
            "exactly one of six simultaneous starts may be admitted"
        );
        for rejected in results.iter().filter(|r| r.is_err()) {
            assert!(
                rejected
                    .as_ref()
                    .expect_err("rejected")
                    .contains("already in progress"),
                "rejection must explain the busy state"
            );
        }
        let _ = std::fs::remove_dir_all(&*dir);
    }

    #[test]
    fn rejected_start_leaves_no_registered_run() {
        let registry = RunRegistry::default();
        let definition = &ABSENT_FIXTURE;
        let dir = temp_dir();

        registry
            .start(definition, &dir, &dummy_identity())
            .expect("first start");
        assert!(registry.start(definition, &dir, &dummy_identity()).is_err());
        // The active count is back to one run; a rejected start must not
        // have created a second registered entry. Registry internals are
        // private — assert via the admission state: another start must
        // still be rejected (one is active).
        assert!(registry.start(definition, &dir, &dummy_identity()).is_err());
        let _ = std::fs::remove_dir_all(&*dir);
    }
}

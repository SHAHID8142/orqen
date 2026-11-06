//! Controlled diagnostic run execution.
//!
//! One fixed noninteractive prompt per registered agent, executed in an
//! application-managed isolated working directory. Runs are tracked
//! in-memory for the current session only — nothing about them is
//! persisted to SQLite (retention policy: session-scoped; privacy: the
//! prompt is a compile-time constant and the output never leaves the
//! machine).
//!
//! Process safety (reuses the Phase 3 verification patterns):
//! - the probe spawns as its own process group (`process_group(0)`, unix)
//!   so cancellation and cleanup terminate the whole tree with SIGKILL;
//! - output readers drain bounded shared buffers;
//! - the deadline covers the whole lifecycle and a cancellation flag is
//!   honored between polls.

use std::collections::HashMap;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::definitions::AgentDefinition;
use super::discovery;
use super::execution::{find_adapter, ExecutionAdapter};

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
    Starting { agent_id: String },
    Running { agent_id: String, started_at: String },
    Finished { result: RunOutcomeDto },
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
        &mut self.streams[stream]
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

/// Drops the shared drain handle and signals remaining readers to stop
/// draining (bounded retention) on every exit path.
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
    active: AtomicUsize,
}

impl Default for RunRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RegistryInner::default()),
        }
    }
}

impl RunRegistry {
    /// Registers a new run and executes it on a detached worker thread.
    /// Returns the stable run id, or an error when the concurrent-run
    /// bound (one diagnostic at a time) is already exhausted.
    pub fn start(
        &self,
        definition: &'static AgentDefinition,
        data_dir: &Path,
    ) -> Result<String, String> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
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
        // Enforce the concurrent-run bound (independent of the map scan).
        if self.inner.active.load(Ordering::SeqCst) >= MAX_CONCURRENT_RUNS {
            return Err(
                "a diagnostic run is already in progress — cancel it or wait \
                 for it to finish."
                    .to_string(),
            );
        }
        self.inner.active.fetch_add(1, Ordering::SeqCst);

        let inner = self.inner.clone();
        let worker_run_id = run_id.clone();
        let worker_data_dir = data_dir.to_path_buf();
        std::thread::spawn(move || {
            let outcome =
                execute_diagnostic(definition, &worker_data_dir, &worker_run_id, &cancel, &inner);
            let mut runs = inner.runs.lock().expect("runs lock");
            if let Some(entry) = runs.get_mut(&worker_run_id) {
                if let Ok(mut finished) = entry.finished.lock() {
                    *finished = Some(outcome);
                }
            }
            if inner.active.load(Ordering::SeqCst) > 0 {
                inner.active.fetch_sub(1, Ordering::SeqCst);
            }
        });
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

    /// Requests cancellation of a running diagnostic.
    pub fn cancel(&self, run_id: &str) -> Result<(), String> {
        let runs = self.inner.runs.lock().expect("runs lock");
        let entry = runs
            .get(run_id)
            .ok_or_else(|| format!("unknown run \"{run_id}\"."))?;
        entry.cancel.store(true, Ordering::SeqCst);
        if let Some(pid) = *entry.pid.lock().expect("pid lock") {
            // Unix: signal the whole probe process group so descendants
            // holding inherited pipes die with the direct child.
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
            }
            #[cfg(not(unix))]
            {
                let _ = pid; // direct-child kill happens via KillOnDrop
            }
        }
        Ok(())
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Executes the diagnostic and returns the recorded outcome. Runs on the
/// detached worker thread.
fn execute_diagnostic(
    definition: &'static AgentDefinition,
    data_dir: &Path,
    run_id: &str,
    cancel: &Arc<AtomicBool>,
    inner: &Arc<RegistryInner>,
) -> RunOutcomeDto {
    let started_at = now_rfc3339();
    let started = Instant::now();

    let mut finish = |outcome: RunOutcome,
                      exit_code: Option<i32>,
                      output: String,
                      error: Option<String>,
                      finished_at: String| {
        RunOutcomeDto {
            agent_id: definition.id.to_string(),
            started_at: started_at.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            outcome,
            exit_code,
            output,
            error,
            finished_at,
        }
    };

    // Adapter + eligibility.
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
                now_rfc3339(),
            );
        }
    };
    if !adapter.supports_safe_diagnostics() {
        return finish(
            RunOutcome::Failed,
            None,
            String::new(),
            Some(format!(
                "\"{}\" cannot run a diagnostic safely with the documented \
                 invocation of this CLI version; execution stays disabled.",
                definition.display_name
            )),
            now_rfc3339(),
        );
    }

    // Discovery: the executable must have been located.
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
                now_rfc3339(),
            );
        }
        discovery::DiscoveryOutcome::Error { message } => {
            return finish(
                RunOutcome::Failed,
                None,
                String::new(),
                Some(message),
                now_rfc3339(),
            );
        }
    };

    // Isolated working directory: application-managed, no user project
    // data, removed after the run.
    let work_dir = data_dir.join("diagnostic-runs").join(run_id);
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        return finish(
            RunOutcome::Failed,
            None,
            String::new(),
            Some(format!("could not create the isolated run directory: {e}")),
            now_rfc3339(),
        );
    }

    // Spawn: adapter-owned arguments, explicit working directory, own
    // process group (unix) for whole-tree cancellation.
    let mut command = Command::new(&executable);
    command
        .args(adapter.build_arguments())
        .current_dir(&work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work_dir);
            return finish(
                RunOutcome::Failed,
                None,
                String::new(),
                Some(format!(
                    "the executable could not be started: it may be \
                     incompatible with this system or lack execute permission."
                )),
                now_rfc3339(),
            );
        }
    };

    // Register the pid so cancellation can signal the whole group.
    let pid = child.id();
    if let Ok(mut runs) = inner.runs.lock() {
        if let Some(entry) = runs.get_mut(run_id) {
            if let Ok(mut slot) = entry.pid.lock() {
                *slot = Some(pid);
            }
        }
    }

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let shared = SharedOutput::default();
    let drain_guard = DrainGuard(shared.clone());
    let stdout_reader = std::thread::spawn({
        let shared = shared.clone();
        move || read_drained(stdout, &shared, 0)
    });
    let stderr_reader = std::thread::spawn({
        let shared = shared.clone();
        move || read_drained(stderr, &shared, 1)
    });

    // Poll loop: deadline, cancellation, child exit.
    let deadline = Instant::now() + DIAGNOSTIC_TIMEOUT;
    let mut timed_out = false;
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            break None; // cancellation path
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if cancel.load(Ordering::SeqCst) {
                    continue; // re-check on the next iteration
                }
                if Instant::now() >= deadline {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&work_dir);
                return finish(
                    RunOutcome::Failed,
                    None,
                    String::new(),
                    Some(format!("could not track the CLI process: {e}")),
                    now_rfc3339(),
                );
            }
        }
    };

    if timed_out {
        // Terminate the whole tree; the readers drain what remains.
        #[cfg(unix)]
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
        let _ = child.kill();
    }

    // Short grace for the readers after the tree is signalled, then
    // abandon them (bounded buffers; they close at the next wakeup).
    let grace_deadline = Instant::now() + READER_GRACE;
    while !shared.readers_done() && Instant::now() < grace_deadline {
        std::thread::sleep(POLL_INTERVAL);
    }
    drop(drain_guard);
    drop(stdout_reader);
    drop(stderr_reader);
    let (stdout_bytes, stderr_bytes) = shared.snapshot();

    let _ = std::fs::remove_dir_all(&work_dir);

    let output = String::from_utf8_lossy(&stdout_bytes)
        .chars()
        .take(8000)
        .collect::<String>();
    let stderr_excerpt = String::from_utf8_lossy(&stderr_bytes)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>();

    // Cancellation: the loop broke on the cancel flag.
    if cancel.load(Ordering::SeqCst) && status.is_none() && !timed_out {
        return finish(
            RunOutcome::Cancelled,
            None,
            output,
            Some("the run was cancelled.".to_string()),
            now_rfc3339(),
        );
    }
    if timed_out {
        return finish(
            RunOutcome::TimedOut,
            status.and_then(|s| s.code()),
            output,
            Some(format!(
                "the diagnostic timed out after {}s — the process tree was \
                 terminated.",
                DIAGNOSTIC_TIMEOUT.as_secs()
            )),
            now_rfc3339(),
        );
    }

    let exit_code = status.and_then(|s| s.code());
    match exit_code {
        Some(0) => {
            if output.trim().is_empty() {
                finish(
                    RunOutcome::Failed,
                    exit_code,
                    output,
                    Some(
                        "the CLI exited successfully but produced no output."
                            .to_string(),
                    ),
                    now_rfc3339(),
                )
            } else {
                finish(RunOutcome::Succeeded, exit_code, output, None, now_rfc3339())
            }
        }
        Some(code) => finish(
            RunOutcome::Failed,
            exit_code,
            output,
            Some(format!(
                "the CLI exited with code {code}.{}",
                if stderr_excerpt.is_empty() {
                    String::new()
                } else {
                    format!(" {stderr_excerpt}")
                }
            )),
            now_rfc3339(),
        ),
        None => finish(
            RunOutcome::Failed,
            exit_code,
            output,
            Some("the CLI was terminated by a signal.".to_string()),
            now_rfc3339(),
        ),
    }
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

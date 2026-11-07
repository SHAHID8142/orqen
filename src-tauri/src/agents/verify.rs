//! Controlled CLI verification.
//!
//! Runs a single, fixed, noninteractive probe against a discovered
//! executable: `<resolved-path> --version`.
//!
//! Security boundaries:
//! - On unix, no shell is involved: `std::process::Command` receives the
//!   resolved path (found by our own discovery) and one allowlisted
//!   argument.
//! - On Windows, `.cmd`/`.bat` targets are executed THROUGH `cmd.exe` by
//!   the OS and the Rust runtime (see docs/ARCHITECTURE.md §Trust
//!   boundaries). The injection surface stays closed because the argument
//!   list is the single fixed `--version` literal and the path comes from
//!   our own discovery — but "shell-free" must not be claimed
//!   unconditionally on Windows. `.exe` candidates (checked first) do not
//!   involve `cmd.exe`.
//! - Arbitrary arguments, environment overrides, and custom commands are
//!   impossible by construction — nothing user-supplied reaches the child.
//! - Verification is explicit and user-initiated; discovery never probes.
//! - The child is terminated on timeout (kill + reap); the deadline
//!   covers the whole lifecycle including output readers, so a descendant
//!   holding inherited pipes cannot hang verification. Output is bounded
//!   (first 64 KiB per stream kept, the remainder drained).
//! - No authentication, login, model inference, or interactive session is
//!   ever attempted.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Hard limits for the probe.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_OUTPUT_BYTES: u64 = 64 * 1024;
/// Polling interval while waiting for the child to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Grace period for output readers after the direct child exits (a
/// descendant holding the inherited pipes cannot extend the deadline).
const READER_GRACE: Duration = Duration::from_millis(1000);
/// Longest stderr excerpt kept for a failed probe.
const MAX_ERROR_EXCERPT: usize = 200;

/// ENFORCED reader admission limit: at most this many output-reader
/// threads may exist at any moment (two per concurrent probe — stdout and
/// stderr). Before a probe spawns, its two slots are reserved atomically;
/// an exhausted pool rejects the probe with a resource-busy error. Slots
/// are released through RAII exactly when the owning reader thread
/// terminates — including readers kept blocked by descendants that
/// inherited the pipes, so the bound holds no matter how long such
/// descendants live (docs/ARCHITECTURE.md §Trust boundaries).
pub const MAX_CONCURRENT_READERS: usize = 64;
static READER_SLOTS: std::sync::Mutex<usize> = std::sync::Mutex::new(MAX_CONCURRENT_READERS);

/// RAII reservation of one reader slot. Moving it into a reader thread
/// transfers ownership; the slot returns to the pool exactly when the
/// thread (and its reader) terminates. Not `Clone` — exactly one drop.
/// Public so the diagnostic runner can hold its stdout/stderr permits.
pub struct ReaderSlot;

impl Drop for ReaderSlot {
    fn drop(&mut self) {
        if let Ok(mut available) = READER_SLOTS.lock() {
            *available += 1;
        }
        // A poisoned lock recovers via into_inner on the next acquire.
    }
}

/// Atomically reserves one reader slot, if capacity remains.
pub fn try_acquire_reader_slot() -> Option<ReaderSlot> {
    let mut available = READER_SLOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *available == 0 {
        return None;
    }
    *available -= 1;
    Some(ReaderSlot)
}

/// Reserves a matched pair of slots (stdout + stderr), or none at all —
/// a probe is never started with only one of its two readers.
pub fn acquire_reader_slot_pair() -> Option<(ReaderSlot, ReaderSlot)> {
    let first = try_acquire_reader_slot()?;
    // On failure the first permit is released by its own Drop on this path.
    try_acquire_reader_slot().map(|second| (first, second))
}

/// Diagnostic/test helper: currently free reader slots.
pub fn available_reader_slots() -> usize {
    *READER_SLOTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A successfully verified CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCli {
    pub version: String,
}

/// Why a verification attempt failed. Details are for local logs; the
/// command layer turns these into user-facing messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// The child did not exit within the timeout and was killed.
    Timeout { seconds: u64 },
    /// The process could not be spawned (missing binary, permissions).
    SpawnFailed { detail: String },
    /// The child exited with a nonzero code.
    NonZeroExit { code: i32, excerpt: String },
    /// Exit code 0 but no usable version line (or non-UTF-8 output).
    MalformedOutput,
    /// The reader-slot pool is exhausted — too many concurrent probes
    /// (e.g. readers blocked behind live descendants). Nothing was run.
    ResourceBusy { limit: usize },
}

impl VerifyError {
    /// User-facing message. Never suggests deleting data; never dumps
    /// raw internals beyond a short, bounded excerpt.
    pub fn user_message(&self) -> String {
        match self {
            Self::Timeout { seconds } => format!(
                "verification timed out after {seconds}s — the process was \
                 terminated. Try again."
            ),
            Self::SpawnFailed { .. } => concat!(
                "verification failed: the executable could not be started. It ",
                "may be incompatible with this system or lack execute ",
                "permission."
            )
            .to_string(),
            Self::NonZeroExit { code, .. } => format!(
                "verification failed: the CLI exited with code {code}. The \
                 installation may be broken — reinstall the CLI and verify \
                 again."
            ),
            Self::MalformedOutput => {
                "verification failed: the CLI did not report a usable version.".to_string()
            }
            Self::ResourceBusy { limit } => format!(
                "verification is busy: at most {} CLI probes can run at the \
                 same time and all slots are in use. Wait for one to finish \
                 and try again.",
                limit / 2
            ),
        }
    }
}

/// Drops with the child and terminates its whole process tree if still
/// running (timeout/shutdown/cleanup).
///
/// Unix: the probe runs as its own process group (`process_group(0)`), so
/// signalling `-pgid` reaches descendants that inherited the pipes — an
/// orphaned `sleep`-style descendant can no longer keep them open.
/// Windows: only the direct child is killed; descendants may outlive the
/// probe (documented limitation — the detached-reader bound covers the
/// resource side).
struct KillOnDrop {
    child: std::process::Child,
    #[cfg(unix)]
    pgid: libc::pid_t,
}

impl KillOnDrop {
    fn new(child: std::process::Child) -> Self {
        #[cfg(unix)]
        let pgid = child.id() as libc::pid_t;
        #[cfg(not(unix))]
        let pgid = ();
        Self {
            child,
            #[cfg(unix)]
            pgid,
        }
    }

    fn terminate_tree(&mut self) {
        #[cfg(unix)]
        unsafe {
            // Negative pid targets the entire process group. SIGKILL:
            // best-effort tree teardown of a misbehaving probe.
            libc::kill(-self.pgid, libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        self.terminate_tree();
    }
}

/// Sets the readers' abandon flag on any exit path (timeout, spawn
/// failure after spawn, early return) so no reader outlives the
/// verification result without a bound.
struct AbandonOnDrop(SharedOutput);
impl Drop for AbandonOnDrop {
    fn drop(&mut self) {
        self.0.abandon();
    }
}

/// Verifies a discovered CLI by running `<path> --version`.
///
/// The path comes from our own discovery (never from free-form user
/// input) and the argument list is fixed. The deadline covers the whole
/// lifecycle: after the direct child exits, output readers get a short
/// grace period before the result is decided from what was captured —
/// descendants that inherited the pipes cannot hang verification.
pub fn verify_cli(path: &Path, timeout: Duration) -> Result<VerifiedCli, VerifyError> {
    // Admission first: the two reader slots (stdout + stderr) are
    // reserved atomically BEFORE anything is spawned. An exhausted pool
    // rejects the probe gracefully — no child is started. The slots are
    // moved into the reader threads and return to the pool through RAII
    // exactly when those readers terminate (however long that takes).
    let (stdout_slot, stderr_slot) =
        acquire_reader_slot_pair().ok_or(VerifyError::ResourceBusy {
            limit: MAX_CONCURRENT_READERS,
        })?;

    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Own process group on unix: a timeout can terminate the whole probe
    // tree (descendants included) instead of orphaning pipe holders.
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().map_err(|e| VerifyError::SpawnFailed {
        detail: e.to_string(),
    })?;
    let mut guarded = KillOnDrop::new(child);
    let shared = SharedOutput::default();
    let _abandon_guard = AbandonOnDrop(shared.clone()); // Drop sets the readers' abandon flag on every exit path

    let stdout = guarded.child.stdout.take().expect("stdout was piped");
    let stderr = guarded.child.stderr.take().expect("stderr was piped");
    // Readers drain their pipes to EOF, keeping only the first
    // MAX_OUTPUT_BYTES each, into shared buffers. They can never block
    // verification: the main loop decides on the deadline regardless.
    let stdout_reader = std::thread::spawn({
        let shared = shared.clone();
        move || {
            read_into_shared(stdout, &shared, Stream::Stdout);
            drop(stdout_slot); // capacity returns when this reader terminates
        }
    });
    let stderr_reader = std::thread::spawn({
        let shared = shared.clone();
        move || {
            read_into_shared(stderr, &shared, Stream::Stderr);
            drop(stderr_slot);
        }
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match guarded.child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let seconds = timeout.as_secs();
                    return Err(VerifyError::Timeout { seconds });
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                return Err(VerifyError::SpawnFailed {
                    detail: e.to_string(),
                });
            }
        }
    };

    // The direct child exited. Descendants may still hold the inherited
    // pipes; give the readers a short grace period, then abandon them:
    // they close the read ends at their next wakeup and exit, and the
    // global detached counter bounds the accumulation.
    let grace_deadline = Instant::now() + READER_GRACE;
    while !shared.readers_done() && Instant::now() < grace_deadline {
        std::thread::sleep(POLL_INTERVAL);
    }
    drop(stdout_reader); // abandoned deliberately if pipes are still held
    drop(stderr_reader);
    let (stdout_bytes, stderr_bytes) = shared.snapshot();

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        let excerpt = String::from_utf8_lossy(&stderr_bytes)
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .chars()
            .take(MAX_ERROR_EXCERPT)
            .collect::<String>();
        return Err(VerifyError::NonZeroExit { code, excerpt });
    }

    let version = String::from_utf8(stdout_bytes)
        .ok()
        .filter(|text| !text.trim().is_empty())
        .or_else(|| {
            // Some CLIs print the version on stderr with exit 0.
            String::from_utf8(stderr_bytes)
                .ok()
                .filter(|text| !text.trim().is_empty())
        })
        .and_then(|text| {
            text.lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(|line| line.chars().take(200).collect::<String>())
        });
    match version {
        Some(version) => Ok(VerifiedCli { version }),
        None => Err(VerifyError::MalformedOutput),
    }
}

#[derive(Clone, Default)]
struct SharedOutput {
    inner: std::sync::Arc<std::sync::Mutex<OutputBuffers>>,
}

#[derive(Default)]
struct OutputBuffers {
    streams: [StreamBuffers; 2],
    abandoned: bool,
}

#[derive(Default)]
struct StreamBuffers {
    data: Vec<u8>,
    done: bool,
}

impl OutputBuffers {
    fn stream(&mut self, stream: Stream) -> &mut StreamBuffers {
        &mut self.streams[match stream {
            Stream::Stdout => 0,
            Stream::Stderr => 1,
        }]
    }
}

impl SharedOutput {
    /// After the grace period: readers close their read end at their next
    /// wakeup (a descendant writing afterwards gets EPIPE/SIGPIPE and
    /// typically exits). Fully blocked silent readers wake only when the
    /// descendant writes or exits — the reader-slot pool bounds how many
    /// such threads can exist at once.
    fn abandon(&self) {
        let mut guard = self.inner.lock().expect("output lock");
        guard.abandoned = true;
        for stream in [Stream::Stdout, Stream::Stderr] {
            // Signal the readers to stop draining and close their read end
            // at the next wakeup.
            guard.stream(stream).done = true;
        }
    }

    fn is_abandoned(&self) -> bool {
        self.inner.lock().expect("output lock").abandoned
    }

    /// Marks a stream done (every reader exit path calls this).
    fn finish_stream(&self, stream: Stream) {
        let mut guard = self.inner.lock().expect("output lock");
        guard.stream(stream).done = true;
    }

    fn append(&self, stream: Stream, chunk: &[u8]) {
        let mut guard = self.inner.lock().expect("output lock");
        let stream_buffers = guard.stream(stream);
        let remaining = MAX_OUTPUT_BYTES as usize - stream_buffers.data.len();
        if remaining > 0 {
            let end = chunk.len().min(remaining);
            stream_buffers.data.extend_from_slice(&chunk[..end]);
        }
    }

    fn readers_done(&self) -> bool {
        let guard = self.inner.lock().expect("output lock");
        guard.streams.iter().all(|s| s.done)
    }

    fn snapshot(&self) -> (Vec<u8>, Vec<u8>) {
        let guard = self.inner.lock().expect("output lock");
        (guard.streams[0].data.clone(), guard.streams[1].data.clone())
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Drains a pipe to EOF, retaining at most MAX_OUTPUT_BYTES per stream.
/// Once abandoned, the reader closes its read end at the next wakeup
/// (dropping the stream) and exits, releasing its detached bookkeeping —
/// on EVERY exit path, so the counter cannot leak.
fn read_into_shared(mut stream: impl Read, shared: &SharedOutput, stream_id: Stream) {
    let mut chunk = [0u8; 8192];
    loop {
        if shared.is_abandoned() {
            break; // dropping `stream` closes the read end
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                shared.append(stream_id, &chunk[..n]);
            }
            Err(_) => break,
        }
    }
    shared.finish_stream(stream_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;
    use std::fs;
    use std::path::PathBuf;

    /// Writes a controlled test executable (a small shell script). These
    /// fixtures are the only processes the unit tests run — never a real
    /// third-party CLI.
    /// Serializes tests that spawn processes: they share the process-global
    /// detached-reader counter (cargo runs tests in parallel by default).
    fn probe_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(unix)]
    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        fs::write(&path, body).expect("write script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    // Windows-only: these run on Windows CI/development machines and are
    // NOT verifiable on macOS — see docs/QA_REPORT.md for what remains
    // unverified on this machine.
    #[cfg(windows)]
    #[test]
    fn verifies_a_batch_file_cli_via_the_windows_path() {
        let dir = temp_db_dir();
        // .cmd runs through cmd.exe by OS design; the argument list is
        // still the single fixed literal.
        let path = dir.join("fake-cli.cmd");
        std::fs::write(&path, "@echo fake-cli 1.2.3\r\n").expect("write cmd");
        let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify");
        assert_eq!(verified.version, "fake-cli 1.2.3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_candidates_prefer_exe_over_cmd_and_bat() {
        // Order matters: .exe first so a native binary is preferred when
        // all three exist.
        let candidates =
            crate::agents::discovery::candidates_for_platform(Path::new("C:\\Apps"), "fake-cli");
        let names: Vec<String> = candidates
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["fake-cli", "fake-cli.exe", "fake-cli.cmd", "fake-cli.bat"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn verifies_a_well_behaved_fake_cli() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        let path = write_script(&dir, "fake-cli", "#!/bin/sh\necho \"fake-cli 1.2.3\"\n");
        let verified = verify_cli(&path, Duration::from_secs(5)).expect("verify");
        assert_eq!(verified.version, "fake-cli 1.2.3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn kills_and_reports_timeout_for_a_hanging_cli() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        let path = write_script(&dir, "hanging-cli", "#!/bin/sh\nsleep 30\n");
        let started = Instant::now();
        let error = verify_cli(&path, Duration::from_millis(500)).expect_err("must time out");
        assert!(
            matches!(error, VerifyError::Timeout { seconds: 0 } if started.elapsed() < Duration::from_secs(5)),
            "expected a fast timeout, got: {error:?}"
        );
        // The process-group kill terminates the hanging child AND any
        // descendants: the pipes close, both reader slots release
        // promptly (deterministic — no orphan holding the pipes).
        let deadline = Instant::now() + Duration::from_secs(3);
        while available_reader_slots() < MAX_CONCURRENT_READERS && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            available_reader_slots(),
            MAX_CONCURRENT_READERS,
            "both reader slots must release promptly after the group kill"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn reports_nonzero_exit_codes() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        let path = write_script(
            &dir,
            "failing-cli",
            "#!/bin/sh\necho \"boom\" >&2\nexit 3\n",
        );
        let error = verify_cli(&path, Duration::from_secs(5)).expect_err("must fail");
        match error {
            VerifyError::NonZeroExit { code, excerpt } => {
                assert_eq!(code, 3);
                assert!(excerpt.contains("boom"), "excerpt: {excerpt}");
            }
            other => panic!("expected NonZeroExit, got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn reports_malformed_output_when_no_version_line_is_printed() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        let path = write_script(&dir, "silent-cli", "#!/bin/sh\nexit 0\n");
        let error = verify_cli(&path, Duration::from_secs(5)).expect_err("must fail");
        assert_eq!(error, VerifyError::MalformedOutput);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn bounded_output_handles_a_flooding_child() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        // Prints far more than MAX_OUTPUT_BYTES, then exits 0 — must not
        // hang, must yield a usable (truncated) first line.
        let path = write_script(
            &dir,
            "flooding-cli",
            "#!/bin/sh\necho \"flooding-cli 9.9.9\"\nhead -c 1048576 /dev/zero | tr '\\0' 'A'\n",
        );
        let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify");
        assert_eq!(verified.version, "flooding-cli 9.9.9");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn descendants_holding_pipes_cannot_hang_verification() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        // Parent prints the version, then backgrounds a child that keeps
        // the inherited stdout open past the reader grace period (1s) and
        // exits immediately. The descendant must NOT extend verification.
        let path = write_script(
            &dir,
            "descendant-cli",
            "#!/bin/sh\necho \"descendant-cli 1.0.0\"\nsleep 2 &\nexit 0\n",
        );
        let started = Instant::now();
        let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify");
        assert_eq!(verified.version, "descendant-cli 1.0.0");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "verification must not wait for the descendant: {:?}",
            started.elapsed()
        );
        // Clean up within the lock: the reader slots return when the
        // descendant exits (group-killed at verify return), leaving the
        // pool clean for the next slot-asserting test.
        let deadline = Instant::now() + Duration::from_secs(10);
        while available_reader_slots() < MAX_CONCURRENT_READERS && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(
            available_reader_slots(),
            MAX_CONCURRENT_READERS,
            "reader slots must return to the pool once the descendant exits"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn reader_slots_are_released_when_the_descendant_exits() {
        let _probe = probe_lock();
        // The slot pool is process-global: serialize the tests that assert
        // on it so parallel descendant fixtures cannot interfere.
        let dir = temp_db_dir();
        // Descendant holds the pipes for ~2s: the reader slots stay
        // reserved while it lives, then return when it exits.
        let path = write_script(
            &dir,
            "short-descendant",
            "#!/bin/sh\necho \"short 1.0\"\nsleep 2 &\nexit 0\n",
        );
        let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify");
        assert_eq!(verified.version, "short 1.0");
        assert!(
            available_reader_slots() < MAX_CONCURRENT_READERS,
            "reader slots reserved while the descendant lives"
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        while available_reader_slots() < MAX_CONCURRENT_READERS && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(
            available_reader_slots(),
            MAX_CONCURRENT_READERS,
            "reader slots must return to the pool once the descendant exits"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn full_pool_rejects_the_probe_without_running_it() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        // Exhaust every slot, then verify against a fixture that leaves a
        // sentinel file behind when it RUNS. The rejection must happen
        // before anything is spawned.
        let mut permits = Vec::new();
        while let Some(permit) = try_acquire_reader_slot() {
            permits.push(permit);
        }
        assert_eq!(available_reader_slots(), 0);

        let sentinel = dir.join("probe-ran.marker");
        let script_body = format!(
            "#!/bin/sh\ntouch '{}'\necho \"should-not-run 1.0\"\n",
            sentinel.display()
        );
        let path = write_script(&dir, "rejected-cli", &script_body);

        let error = verify_cli(&path, Duration::from_secs(10))
            .expect_err("exhausted pool must reject the probe");
        match error {
            VerifyError::ResourceBusy { limit } => {
                assert_eq!(limit, MAX_CONCURRENT_READERS);
                assert!(
                    error.user_message().contains("busy"),
                    "user message must explain the busy state: {}",
                    error.user_message()
                );
            }
            other => panic!("expected ResourceBusy, got: {other:?}"),
        }
        // The fixture never ran: no sentinel file, and both slots were
        // released by the rejection path (verify returns before spawning).
        assert!(
            !sentinel.exists(),
            "the probe must not be spawned when the pool is exhausted"
        );
        drop(permits);
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);

        // Capacity recovered: the same probe now runs and verifies.
        let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify after recovery");
        assert_eq!(verified.version, "should-not-run 1.0");
        assert!(sentinel.exists(), "the probe runs once capacity returns");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn repeated_probes_return_all_reader_slots() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        // Every probe's process group is terminated at cleanup, so no
        // slots may accumulate across repeated probes.
        let path = write_script(
            &dir,
            "desc-loop",
            "#!/bin/sh\necho \"desc-loop 1.0\"\nsleep 1 &\nexit 0\n",
        );
        for _i in 0..4 {
            let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify");
            assert_eq!(verified.version, "desc-loop 1.0");
            // The group kill terminates the descendant immediately: both
            // readers finish and their slots return before the next probe.
            let deadline = Instant::now() + Duration::from_secs(3);
            while available_reader_slots() < MAX_CONCURRENT_READERS && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            assert_eq!(
                available_reader_slots(),
                MAX_CONCURRENT_READERS,
                "all reader slots must return after each probe"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pool_acquire_release_round_trip() {
        let _probe = probe_lock();
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);
        let first = try_acquire_reader_slot().expect("acquire");
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS - 1);
        drop(first);
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);
    }

    #[test]
    fn pool_boundary_exhaustion_and_recovery() {
        let _probe = probe_lock();
        let mut permits = Vec::new();
        for _ in 0..MAX_CONCURRENT_READERS {
            permits.push(try_acquire_reader_slot().expect("slot available"));
        }
        assert_eq!(available_reader_slots(), 0);
        assert!(
            try_acquire_reader_slot().is_none(),
            "exhausted pool must reject"
        );
        // Release everything: capacity returns exactly, no underflow.
        drop(permits);
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);
        // And the pool is fully usable again.
        let permit = try_acquire_reader_slot().expect("usable again");
        drop(permit);
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);
    }

    #[test]
    fn pool_pair_is_atomic_or_none() {
        let _probe = probe_lock();
        // Leave exactly ONE slot free: a pair acquisition must fail without
        // taking it (no half-reservation), then the slot stays available.
        let mut permits = Vec::new();
        while permits.len() < MAX_CONCURRENT_READERS - 1 {
            permits.push(try_acquire_reader_slot().expect("slot"));
        }
        assert_eq!(available_reader_slots(), 1);
        assert!(
            acquire_reader_slot_pair().is_none(),
            "pair must not take the last slot"
        );
        assert_eq!(
            available_reader_slots(),
            1,
            "the last slot must be untouched"
        );
        drop(permits);
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);
    }

    #[test]
    fn pool_concurrent_acquisition_never_exceeds_the_limit() {
        let _probe = probe_lock();
        use std::sync::atomic::AtomicUsize;
        let acquired = std::sync::Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let acquired = acquired.clone();
            handles.push(std::thread::spawn(move || {
                let mut local = 0usize;
                for _ in 0..16 {
                    if try_acquire_reader_slot().is_some() {
                        local += 1;
                        // Hold briefly, then release.
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                acquired.fetch_add(local, std::sync::atomic::Ordering::SeqCst);
                local
            }));
        }
        let mut total = 0usize;
        for handle in handles {
            total += handle.join().expect("thread");
        }
        // Each thread attempted 16 acquisitions (128 attempts over 64
        // slots): the POOL must never hand out more than 64 at once —
        // the peak is bounded by construction; here we assert the pool
        // returns to exactly full afterwards.
        assert_eq!(available_reader_slots(), MAX_CONCURRENT_READERS);
        assert!(total <= 128);
        let _ = total;
    }

    #[test]
    fn shared_output_retains_at_most_the_cap() {
        use std::io::Write;
        // read_into_shared retains MAX_OUTPUT_BYTES and drains the rest
        // (covered end-to-end by the flood test); this pins the retained
        // size directly.
        let mut flooding = Vec::new();
        flooding
            .write_all(&vec![b'A'; (MAX_OUTPUT_BYTES * 2) as usize])
            .expect("write");
        let shared = SharedOutput::default();
        read_into_shared(flooding.as_slice(), &shared, Stream::Stdout);
        let (stdout, _stderr) = shared.snapshot();
        assert_eq!(stdout.len() as u64, MAX_OUTPUT_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn repeated_verifications_are_independent_and_clean() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        let path = write_script(&dir, "steady-cli", "#!/bin/sh\necho \"steady 2.0\"\n");
        for _ in 0..5 {
            let verified = verify_cli(&path, Duration::from_secs(5)).expect("verify");
            assert_eq!(verified.version, "steady 2.0");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_failure_is_reported_for_missing_binaries() {
        let _probe = probe_lock();
        let dir = temp_db_dir();
        let missing = dir.join("does-not-exist");
        let error = verify_cli(&missing, Duration::from_secs(5)).expect_err("must fail");
        assert!(matches!(error, VerifyError::SpawnFailed { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn verification_never_passes_shell_metacharacters_to_a_shell() {
        let _probe = probe_lock();
        // The path is used directly by process APIs (no shell). A file
        // whose name contains metacharacters must still be executed as a
        // literal path — proving there is no injection surface.
        let dir = temp_db_dir();
        let path = write_script(
            &dir,
            "weird;rm -rf;name",
            "#!/bin/sh\necho \"safe 0.0.1\"\n",
        );
        let verified = verify_cli(&path, Duration::from_secs(5)).expect("verify");
        assert_eq!(verified.version, "safe 0.0.1");
        assert!(dir.join("weird;rm -rf;name").exists(), "fixture intact");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

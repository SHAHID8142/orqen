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
        }
    }
}

/// Drops with the child and kills it if still running (timeout/shutdown).
struct KillOnDrop(std::process::Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
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
    let child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| VerifyError::SpawnFailed {
            detail: e.to_string(),
        })?;
    let mut guarded = KillOnDrop(child);

    let stdout = guarded.0.stdout.take().expect("stdout was piped");
    let stderr = guarded.0.stderr.take().expect("stderr was piped");
    // Readers drain their pipes to EOF, keeping only the first
    // MAX_OUTPUT_BYTES each, into shared buffers. They can never block
    // verification: the main loop decides on the deadline regardless.
    let shared = SharedOutput::default();
    let stdout_reader = std::thread::spawn({
        let shared = shared.clone();
        move || read_into_shared(stdout, &shared, Stream::Stdout)
    });
    let stderr_reader = std::thread::spawn({
        let shared = shared.clone();
        move || read_into_shared(stderr, &shared, Stream::Stderr)
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match guarded.0.try_wait() {
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
    // pipes; give the readers a short grace period, then proceed with
    // whatever was captured (the reader threads keep draining in the
    // background, bounded, and end when the pipes close).
    let grace_deadline = Instant::now() + READER_GRACE;
    while !shared.readers_done() && Instant::now() < grace_deadline {
        std::thread::sleep(POLL_INTERVAL);
    }
    let _ = stdout_reader; // abandoned deliberately if pipes are still held
    let _ = stderr_reader;
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
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_done: bool,
    stderr_done: bool,
}

impl SharedOutput {
    fn append(&self, stream: Stream, chunk: &[u8]) {
        let mut guard = self.inner.lock().expect("output lock");
        let buffer = match stream {
            Stream::Stdout => &mut guard.stdout,
            Stream::Stderr => &mut guard.stderr,
        };
        let remaining = MAX_OUTPUT_BYTES as usize - buffer.len();
        if remaining > 0 {
            let end = chunk.len().min(remaining);
            buffer.extend_from_slice(&chunk[..end]);
        }
    }

    fn mark_done(&self, stream: Stream) {
        let mut guard = self.inner.lock().expect("output lock");
        match stream {
            Stream::Stdout => guard.stdout_done = true,
            Stream::Stderr => guard.stderr_done = true,
        }
    }

    fn readers_done(&self) -> bool {
        let guard = self.inner.lock().expect("output lock");
        guard.stdout_done && guard.stderr_done
    }

    fn snapshot(&self) -> (Vec<u8>, Vec<u8>) {
        let guard = self.inner.lock().expect("output lock");
        (guard.stdout.clone(), guard.stderr.clone())
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Drains a pipe to EOF, retaining at most MAX_OUTPUT_BYTES per stream.
fn read_into_shared(mut stream: impl Read, shared: &SharedOutput, stream_id: Stream) {
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => shared.append(stream_id, &chunk[..n]),
            Err(_) => break,
        }
    }
    shared.mark_done(stream_id);
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
        let dir = temp_db_dir();
        let path = write_script(&dir, "fake-cli", "#!/bin/sh\necho \"fake-cli 1.2.3\"\n");
        let verified = verify_cli(&path, Duration::from_secs(5)).expect("verify");
        assert_eq!(verified.version, "fake-cli 1.2.3");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn kills_and_reports_timeout_for_a_hanging_cli() {
        let dir = temp_db_dir();
        let path = write_script(&dir, "hanging-cli", "#!/bin/sh\nsleep 30\n");
        let started = Instant::now();
        let error = verify_cli(&path, Duration::from_millis(500)).expect_err("must time out");
        assert!(
            matches!(error, VerifyError::Timeout { seconds: 0 } if started.elapsed() < Duration::from_secs(5)),
            "expected a fast timeout, got: {error:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn reports_nonzero_exit_codes() {
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
        let dir = temp_db_dir();
        let path = write_script(&dir, "silent-cli", "#!/bin/sh\nexit 0\n");
        let error = verify_cli(&path, Duration::from_secs(5)).expect_err("must fail");
        assert_eq!(error, VerifyError::MalformedOutput);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn bounded_output_handles_a_flooding_child() {
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
        let dir = temp_db_dir();
        // Parent prints the version, then backgrounds a child that keeps
        // the inherited stdout open for 30s and exits immediately.
        let path = write_script(
            &dir,
            "descendant-cli",
            "#!/bin/sh\necho \"descendant-cli 1.0.0\"\nsleep 30 &\nexit 0\n",
        );
        let started = Instant::now();
        let verified = verify_cli(&path, Duration::from_secs(10)).expect("verify");
        assert_eq!(verified.version, "descendant-cli 1.0.0");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "verification must not wait for the descendant: {:?}",
            started.elapsed()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn repeated_verifications_are_independent_and_clean() {
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
        let dir = temp_db_dir();
        let missing = dir.join("does-not-exist");
        let error = verify_cli(&missing, Duration::from_secs(5)).expect_err("must fail");
        assert!(matches!(error, VerifyError::SpawnFailed { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn verification_never_passes_shell_metacharacters_to_a_shell() {
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

//! Controlled CLI verification.
//!
//! Runs a single, fixed, noninteractive probe against a discovered
//! executable: `<resolved-path> --version`.
//!
//! Security boundaries:
//! - No shell is involved: `std::process::Command` receives the resolved
//!   path (found by our own discovery) and one allowlisted argument.
//! - Arbitrary arguments, environment overrides, and custom commands are
//!   impossible by construction — nothing user-supplied reaches the child.
//! - Verification is explicit and user-initiated; discovery never probes.
//! - The child is terminated on timeout (kill + reap); output is bounded
//!   (first 64 KiB kept, the remainder drained) so a misbehaving binary
//!   cannot hang or flood the app.
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
/// input) and the argument list is fixed. Output is bounded; a child
/// that outlives `timeout` is killed and reaped.
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
    // Bounded readers: they stop after MAX_OUTPUT_BYTES so a flood cannot
    // grow memory unbounded; the timeout below terminates a child that
    // then blocks on a full pipe.
    let stdout_reader = std::thread::spawn(move || read_bounded(stdout));
    let stderr_reader = std::thread::spawn(move || read_bounded(stderr));

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
    // Child exited (or we are past the check) — close the pipes.
    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| VerifyError::MalformedOutput)?;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| VerifyError::MalformedOutput)?;

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

/// Reads a stream to EOF, keeping at most MAX_OUTPUT_BYTES.
///
/// Bytes beyond the cap are drained and discarded: stopping early would
/// close the pipe and kill a writing child with SIGPIPE (and could lose
/// the version line), while draining guarantees the child completes and
/// the memory footprint stays bounded.
fn read_bounded(mut stream: impl Read) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = MAX_OUTPUT_BYTES as usize - kept.len();
                if remaining > 0 {
                    let end = n.min(remaining);
                    kept.extend_from_slice(&chunk[..end]);
                }
            }
            Err(_) => break,
        }
    }
    kept
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

//! Backend-authoritative execution eligibility for diagnostic runs.
//!
//! Trust boundary: the UI confirmation captures USER INTENT; this module
//! plus the runner's identity check authorize the EXECUTION itself. A
//! frontend "Verified" badge is presentation state — eligibility is
//! decided here, in Rust, from facts this process observed:
//!
//! - the agent id resolves to a registry definition (checked by the
//!   command layer before this module is consulted);
//! - the adapter reports `supports_safe_diagnostics() == true` (checked
//!   in `commands::agents::ensure_diagnostic_admissible`);
//! - a successful backend-side `--version` verification exists and is
//!   FRESH (bounded window — verification is cheap and user-initiated);
//! - the executable that will be spawned still has the identity observed
//!   at verification time (path, size, mtime, unix inode) — re-checked by
//!   the runner immediately before spawning, so the file cannot be
//!   swapped between verification and execution without detection.
//!
//! A successful `--version` output is never treated as proof that the
//! executable is trustworthy; it only records WHAT was verified and WHEN,
//! so any change since then is detectable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// How long a backend-side successful verification authorizes a
/// diagnostic start. Bounded by design: the probe is cheap and
/// user-initiated, so a stale verification must simply be renewed.
pub const VERIFICATION_FRESHNESS: Duration = Duration::from_secs(600);

/// The executable identity observed at verification time. Identity
/// comparison is best-effort "as far as the platform allows": size,
/// modification time, and (unix) inode. A same-size, same-mtime rewrite
/// within the filesystem's mtime granularity could evade detection — a
/// documented residual limitation, not a guarantee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableIdentity {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
    #[cfg(unix)]
    pub inode: u64,
}

impl ExecutableIdentity {
    /// Observes the current identity of a file. `None` when the file
    /// cannot be stat'ed (missing, inaccessible).
    pub fn observe(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified().ok()?,
            #[cfg(unix)]
            inode: {
                use std::os::unix::fs::MetadataExt;
                metadata.ino()
            },
        })
    }

    /// Whether the file at `path` still has exactly this identity.
    pub fn matches_current(&self, path: &Path) -> bool {
        Self::observe(path)
            .map(|current| current == *self)
            .unwrap_or(false)
    }
}

/// Cheaply clonable (shared inner state) so async commands can move a
/// clone into `spawn_blocking` workers.
#[derive(Clone, Default)]
pub struct EligibilityLedger {
    /// agent id → (identity observed at verification time, when).
    records: Arc<Mutex<HashMap<String, (ExecutableIdentity, Instant)>>>,
}

impl EligibilityLedger {
    /// Records a successful backend-side verification of `agent_id`'s
    /// executable. Returns false (and records nothing) when the file
    /// could not be observed — there is nothing to bind authorization to.
    pub fn record_verified(&self, agent_id: &str, path: &Path) -> bool {
        match ExecutableIdentity::observe(path) {
            Some(identity) => {
                if let Ok(mut records) = self.records.lock() {
                    records.insert(agent_id.to_string(), (identity, Instant::now()));
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    }

    /// Removes any authorization for `agent_id` (verification failed or
    /// the executable vanished — prior grants are no longer grounded).
    pub fn clear(&self, agent_id: &str) {
        if let Ok(mut records) = self.records.lock() {
            records.remove(agent_id);
        }
    }

    /// Returns the verified executable identity when a verification
    /// exists AND is fresh under `freshness`. The caller passes its
    /// policy window (production uses `VERIFICATION_FRESHNESS`).
    pub fn fresh_identity(
        &self,
        agent_id: &str,
        freshness: Duration,
    ) -> Result<ExecutableIdentity, String> {
        let records = self.records.lock().expect("eligibility ledger lock");
        let (identity, verified_at) = records.get(agent_id).ok_or_else(|| {
            format!(
                "\"{agent_id}\" has not been verified in this session — \
                 run Verify first; the backend decides execution \
                 eligibility, not the UI."
            )
        })?;
        if verified_at.elapsed() > freshness {
            return Err(format!(
                "the verification of \"{agent_id}\" is stale — verify the \
                 agent again to run a diagnostic."
            ));
        }
        Ok(identity.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orqen-eligibility-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_file(path: &Path, contents: &[u8]) {
        fs::write(path, contents).expect("write file");
    }

    #[test]
    fn identity_matches_until_the_file_changes() {
        let dir = temp_dir();
        let path = dir.join("tool");
        write_file(&path, b"v1");
        let identity = ExecutableIdentity::observe(&path).expect("observe");

        assert!(identity.matches_current(&path));
        // Same size is not enough: any rewrite updates mtime (or inode on
        // copy-replace) — here size changes, the clearest signal.
        write_file(&path, b"v1-with-more-bytes");
        assert!(
            !identity.matches_current(&path),
            "a changed executable must not match the verified identity"
        );

        // Replacement via rename: different inode (unix), same bytes.
        let replacement = dir.join("tool-next");
        write_file(&replacement, b"v1");
        fs::rename(&replacement, &path).expect("swap executable");
        #[cfg(unix)]
        assert!(
            !identity.matches_current(&path),
            "a swapped executable (new inode) must not match"
        );
        #[cfg(not(unix))]
        {
            let _ = identity; // size+mtime still checked above
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn observe_fails_for_missing_files() {
        assert!(ExecutableIdentity::observe(Path::new("/nonexistent/orqen-tool")).is_none());
    }

    #[test]
    fn unverified_agents_have_no_eligibility() {
        let ledger = EligibilityLedger::default();
        let error = ledger
            .fresh_identity("codex", VERIFICATION_FRESHNESS)
            .expect_err("never verified");
        assert!(error.contains("has not been verified"), "got: {error}");
    }

    #[test]
    fn fresh_verification_grants_eligibility_within_the_window() {
        let ledger = EligibilityLedger::default();
        let dir = temp_dir();
        let path = dir.join("tool");
        write_file(&path, b"binary");
        assert!(ledger.record_verified("codex", &path));

        let identity = ledger
            .fresh_identity("codex", VERIFICATION_FRESHNESS)
            .expect("fresh");
        assert_eq!(identity.path, path);
        assert_eq!(identity.size, 6);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_verification_is_rejected() {
        let ledger = EligibilityLedger::default();
        let dir = temp_dir();
        let path = dir.join("tool");
        write_file(&path, b"binary");
        assert!(ledger.record_verified("codex", &path));

        // The freshness window is a policy parameter: a zero window makes
        // any recorded verification stale (bounded-freshness proof).
        let error = ledger
            .fresh_identity("codex", Duration::ZERO)
            .expect_err("stale");
        assert!(error.contains("stale"), "got: {error}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clearing_removes_authorization() {
        let ledger = EligibilityLedger::default();
        let dir = temp_dir();
        let path = dir.join("tool");
        write_file(&path, b"binary");
        assert!(ledger.record_verified("codex", &path));
        ledger.clear("codex");
        assert!(
            ledger
                .fresh_identity("codex", VERIFICATION_FRESHNESS)
                .is_err()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unobservable_files_are_never_recorded() {
        let ledger = EligibilityLedger::default();
        assert!(!ledger.record_verified("codex", Path::new("/nonexistent/orqen-tool")));
        assert!(
            ledger
                .fresh_identity("codex", VERIFICATION_FRESHNESS)
                .is_err()
        );
    }
}

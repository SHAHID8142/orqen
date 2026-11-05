//! Local CLI discovery: locate an agent's executable without executing it.
//!
//! Strategy: search the `PATH` entries (plus a fixed Windows extension
//! allowlist) for an existing, regular file named after the agent's
//! executable. Discovery never runs the binary — existence is not
//! authentication, connectivity, or a successful execution.

use std::path::{Path, PathBuf};

use super::definitions::AgentDefinition;

/// Result of a discovery pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryOutcome {
    Found {
        path: PathBuf,
    },
    NotFound,
    /// The environment made discovery impossible (e.g. `PATH` unset).
    Error {
        message: String,
    },
}

/// Windows executable extensions tried in order (fixed allowlist — the
/// `PATHEXT` environment variable is not honored, so a manipulated value
/// cannot broaden what gets resolved).
#[cfg(windows)]
const WINDOWS_EXTENSIONS: &[&str] = &[".exe", ".cmd", ".bat"];

/// Discovers the agent executable using the process `PATH`.
pub fn discover(definition: &AgentDefinition) -> DiscoveryOutcome {
    let path_var = match std::env::var_os("PATH") {
        Some(value) => value,
        None => {
            return DiscoveryOutcome::Error {
                message: "The PATH environment variable is not set, so installed \
                 CLIs cannot be located. Check your system configuration."
                    .to_string(),
            };
        }
    };
    let dirs: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    discover_in_dirs(definition, &dirs)
}

/// Testable discovery over explicit directories.
pub fn discover_in_dirs(definition: &AgentDefinition, dirs: &[PathBuf]) -> DiscoveryOutcome {
    for dir in dirs {
        let candidates = candidates_for_platform(dir, definition.executable_name);
        for candidate in candidates {
            // Inaccessible or non-regular entries are skipped, not errors:
            // one broken PATH entry must not hide other installations.
            match std::fs::metadata(&candidate) {
                Ok(metadata) if metadata.is_file() && is_executable(&metadata) => {
                    return DiscoveryOutcome::Found { path: candidate };
                }
                _ => continue,
            }
        }
    }
    DiscoveryOutcome::NotFound
}

/// Platform executability signal. Unix exposes the permission bits; on
/// Windows the extension allowlist applied by `candidates_for_platform`
/// is the available signal.
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        true
    }
}

/// Builds the candidate paths for one PATH directory. Windows resolves
/// `.exe`/`.cmd`/`.bat` without an explicit extension; unix does not.
#[cfg(windows)]
pub(crate) fn candidates_for_platform(dir: &Path, executable_name: &str) -> Vec<PathBuf> {
    let mut candidates = vec![dir.join(executable_name)];
    for extension in WINDOWS_EXTENSIONS {
        candidates.push(dir.join(format!("{executable_name}{extension}")));
    }
    candidates
}

#[cfg(not(windows))]
fn candidates_for_platform(dir: &Path, executable_name: &str) -> Vec<PathBuf> {
    vec![dir.join(executable_name)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::temp_db_dir;
    use std::fs;

    const DEFINITION: AgentDefinition = super::super::definitions::CODEX;

    /// Creates an executable file. On unix this is a tiny script with the
    /// executable bit (a controlled test fixture — it is never run here).
    fn make_executable(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, b"#!/bin/sh\nexit 0\n").expect("write fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
    }

    #[test]
    fn finds_executable_in_the_first_directory() {
        let dir = temp_db_dir();
        make_executable(&dir.join("codex"));
        let outcome = discover_in_dirs(&DEFINITION, std::slice::from_ref(&dir));
        match outcome {
            DiscoveryOutcome::Found { path } => assert_eq!(path, dir.join("codex")),
            other => panic!("expected Found, got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_executable_in_a_later_directory() {
        let empty = temp_db_dir();
        let later = temp_db_dir();
        make_executable(&later.join("codex"));
        let outcome = discover_in_dirs(&DEFINITION, &[empty.clone(), later.clone()]);
        match outcome {
            DiscoveryOutcome::Found { path } => assert_eq!(path, later.join("codex")),
            other => panic!("expected Found, got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&empty);
        let _ = std::fs::remove_dir_all(&later);
    }

    #[test]
    fn handles_spaces_in_directory_names() {
        let dir = temp_db_dir().join("My Apps With Spaces");
        make_executable(&dir.join("codex"));
        let outcome = discover_in_dirs(&DEFINITION, std::slice::from_ref(&dir));
        match outcome {
            DiscoveryOutcome::Found { path } => {
                assert_eq!(path, dir.join("codex"));
                assert!(path.to_string_lossy().contains(' '));
            }
            other => panic!("expected Found despite spaces, got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reports_not_found_when_no_directory_contains_the_executable() {
        let empty1 = temp_db_dir();
        let empty2 = temp_db_dir();
        let outcome = discover_in_dirs(&DEFINITION, &[empty1.clone(), empty2.clone()]);
        assert_eq!(outcome, DiscoveryOutcome::NotFound);
        let _ = std::fs::remove_dir_all(&empty1);
        let _ = std::fs::remove_dir_all(&empty2);
    }

    #[test]
    fn skips_directories_and_regular_files_used_as_path_entries() {
        // A PATH entry that is a regular file (not a directory) must be
        // skipped without failing the whole discovery.
        let file_entry = temp_db_dir().join("not-a-dir");
        fs::write(&file_entry, b"file").expect("write file entry");
        let real_dir = temp_db_dir();
        make_executable(&real_dir.join("codex"));

        let outcome = discover_in_dirs(&DEFINITION, &[file_entry.clone(), real_dir.clone()]);
        match outcome {
            DiscoveryOutcome::Found { path } => assert_eq!(path, real_dir.join("codex")),
            other => panic!("expected Found after skipping a file entry, got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&file_entry);
        let _ = std::fs::remove_dir_all(&real_dir);
    }

    #[test]
    fn directories_without_executable_permission_do_not_panic() {
        // A PATH entry that cannot be inspected must be skipped gracefully.
        let dir = temp_db_dir();
        make_executable(&dir.join("codex"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");
        }
        let outcome = discover_in_dirs(&DEFINITION, std::slice::from_ref(&dir));
        // On unix without read permission, metadata on children fails and
        // the entry is skipped (NotFound); the important part is no panic
        // and no execution.
        assert!(matches!(
            outcome,
            DiscoveryOutcome::NotFound | DiscoveryOutcome::Found { .. }
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_files_do_not_count_as_found() {
        // A regular file without execute bits must NOT be reported as an
        // executable candidate (unix exposes the permission bits — use
        // them; see docs/ARCHITECTURE.md for the Windows boundary).
        let first = temp_db_dir();
        fs::write(first.join("codex"), b"not executable").expect("write plain file");
        let outcome = discover_in_dirs(&DEFINITION, std::slice::from_ref(&first));
        assert_eq!(outcome, DiscoveryOutcome::NotFound);
        let _ = std::fs::remove_dir_all(&first);
    }

    #[test]
    fn symlinks_to_executables_are_resolved_safely() {
        // metadata() follows symlinks: a link pointing at a real
        // executable is a valid candidate; a broken link is skipped.
        let dir = temp_db_dir();
        let target = dir.join("real-codex");
        make_executable(&target);
        // The link itself carries the candidate name (`codex`).
        std::os::unix::fs::symlink(&target, dir.join("codex")).expect("symlink");
        let outcome = discover_in_dirs(&DEFINITION, std::slice::from_ref(&dir));
        match outcome {
            DiscoveryOutcome::Found { path } => {
                assert_eq!(path, dir.join("codex"))
            }
            other => panic!("expected Found via symlink, got: {other:?}"),
        }

        let broken = temp_db_dir();
        std::os::unix::fs::symlink(broken.join("missing-target"), broken.join("codex"))
            .expect("broken symlink");
        let outcome = discover_in_dirs(&DEFINITION, std::slice::from_ref(&broken));
        assert_eq!(outcome, DiscoveryOutcome::NotFound);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&broken);
    }
}

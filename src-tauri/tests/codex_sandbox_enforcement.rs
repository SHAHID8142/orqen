//! REAL sandbox-enforcement tests for the Codex diagnostic policy.
//!
//! These tests run the PRODUCTION policy builder
//! (`codex_diagnostic_run_policy`) through the provider-free
//! `codex sandbox` harness — the same policy engine `codex exec` uses —
//! against harmless temporary fixture files. NO provider service is ever
//! contacted (the `sandbox` subcommand runs a local command under the
//! Seatbelt policy; no model, no network to the provider, no auth).
//!
//! The suite self-skips (with a logged reason) when no Codex CLI is
//! available: set `ORQEN_CODEX_BIN` to a codex binary (or have `codex`
//! on PATH). The enforced-policy evidence recorded in docs/QA_REPORT.md
//! was produced with codex-cli 0.164.0; older versions skip too, since
//! the confinement profile is version-gated (fail closed).
//!
//! macOS-only: the verified platform (D-023).

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use orqen_lib::agents::execution::{
    CODEX_MINIMUM_VERSION, codex_diagnostic_run_policy, version_meets_minimum,
};

/// Resolves the Codex CLI under test: `ORQEN_CODEX_BIN` or `codex` on
/// PATH. Returns None (and logs a skip reason) when unavailable or too
/// old for the version-gated policy.
fn codex_binary() -> Option<PathBuf> {
    let candidate = std::env::var_os("ORQEN_CODEX_BIN")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("codex")));
    let binary = candidate?;
    let version_output = Command::new(&binary)
        .arg("--version")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())?;
    if !version_meets_minimum(&version_output, CODEX_MINIMUM_VERSION) {
        eprintln!(
            "skipping codex sandbox enforcement tests: {version_output:?} is older than \
             the verified minimum {CODEX_MINIMUM_VERSION:?}"
        );
        return None;
    }
    Some(binary)
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("orqen-sbx-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Runs `codex sandbox` with the given `-c` overrides and command,
/// returning (exit_code, combined_output). Selection uses
/// `default_permissions` — the SAME mechanism the production `codex
/// exec` invocation uses (the `-P` flag conflicts with it).
fn run_under_policy(
    codex: &Path,
    work_dir: &Path,
    config_overrides: &[String],
    command: &[&str],
) -> (Option<i32>, String) {
    let mut cmd = Command::new(codex);
    cmd.arg("sandbox")
        .args(
            config_overrides
                .iter()
                .flat_map(|value| ["-c".to_string(), value.clone()]),
        )
        .arg("--")
        .args(command)
        .current_dir(work_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = cmd.output().expect("spawn codex sandbox");
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code(), text)
}

/// A temporary file OUTSIDE the work dir that is removed on drop.
struct ProtectedFixture {
    path: PathBuf,
}

impl ProtectedFixture {
    fn create(location: &Path, contents: &str) -> Self {
        std::fs::write(location, contents).expect("write protected fixture");
        ProtectedFixture {
            path: location.to_path_buf(),
        }
    }

    fn home() -> Self {
        let home = std::env::var_os("HOME").expect("HOME is set");
        Self::create(
            &PathBuf::from(home).join(format!("orqen-sbx-protected-{}", uuid::Uuid::new_v4())),
            "PROTECTED-HOME-CONTENT",
        )
    }

    fn in_tmp() -> Self {
        Self::create(
            &std::env::temp_dir().join(format!("orqen-sbx-protected-{}", uuid::Uuid::new_v4())),
            "PROTECTED-TMP-CONTENT",
        )
    }
}

impl Drop for ProtectedFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn permitted_read_inside_the_work_dir_succeeds() {
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("allow-read");
    std::fs::write(work_dir.join("allowed.txt"), "workdir-content").expect("write");
    let policy = codex_diagnostic_run_policy(&work_dir);

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["cat", "allowed.txt"],
    );
    assert_eq!(code, Some(0), "output: {output}");
    assert!(output.contains("workdir-content"), "output: {output}");
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn protected_read_in_home_is_denied() {
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("deny-home");
    let protected = ProtectedFixture::home();
    let policy = codex_diagnostic_run_policy(&work_dir);

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["cat", protected.path.to_string_lossy().as_ref()],
    );
    assert_ne!(code, Some(0), "the protected read must fail: {output}");
    assert!(
        !output.contains("PROTECTED-HOME-CONTENT"),
        "the protected content must never leak: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn protected_read_in_tmp_is_denied() {
    // /tmp leaks through the documented `:minimal` runtime token; the
    // production policy re-denies it explicitly. This test pins that.
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("deny-tmp");
    let protected = ProtectedFixture::in_tmp();
    let policy = codex_diagnostic_run_policy(&work_dir);

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["cat", protected.path.to_string_lossy().as_ref()],
    );
    assert_ne!(code, Some(0), "the /tmp read must fail: {output}");
    assert!(
        !output.contains("PROTECTED-TMP-CONTENT"),
        "the protected content must never leak: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn write_outside_the_work_dir_is_denied() {
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("deny-write");
    let target = std::env::temp_dir().join(format!("orqen-sbx-write-{}", uuid::Uuid::new_v4()));
    let policy = codex_diagnostic_run_policy(&work_dir);

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["touch", target.to_string_lossy().as_ref()],
    );
    assert_ne!(code, Some(0), "the outside write must fail: {output}");
    assert!(
        !target.exists(),
        "no file may be created outside the work dir"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn write_inside_the_work_dir_succeeds() {
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("allow-write");
    let policy = codex_diagnostic_run_policy(&work_dir);

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["touch", "inside-ok.txt"],
    );
    assert_eq!(code, Some(0), "output: {output}");
    assert!(
        work_dir.join("inside-ok.txt").exists(),
        "the work-dir write must succeed"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn invalid_profile_fails_closed() {
    // A malformed access value must be a hard error — never a silent
    // fallback to a broader policy.
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("invalid-profile");
    let policy = codex_diagnostic_run_policy(&work_dir);
    let mut broken_overrides = policy.config_overrides.clone();
    broken_overrides[0] = broken_overrides[0].replace("\"deny\"", "\"bogus-value\"");

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &broken_overrides,
        &["echo", "should-not-run"],
    );
    assert_ne!(
        code,
        Some(0),
        "an invalid profile must fail closed: {output}"
    );
    assert!(!output.contains("should-not-run"));
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn unknown_profile_fails_closed() {
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("unknown-profile");
    // Selecting a profile that is never defined must fail closed.
    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &["default_permissions=\"orqen-diag-does-not-exist\"".to_string()],
        &["echo", "should-not-run"],
    );
    assert_ne!(
        code,
        Some(0),
        "an unknown profile must fail closed: {output}"
    );
    assert!(!output.contains("should-not-run"));
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn nested_escalation_is_denied() {
    // A process inside the confined profile must not be able to spawn a
    // broader codex sandbox and read the protected file through it — the
    // OUTER policy is OS-enforced on the whole process tree.
    let Some(codex) = codex_binary() else {
        eprintln!("skipping: no Codex CLI available (set ORQEN_CODEX_BIN)");
        return;
    };
    let work_dir = temp_dir("escalation");
    let protected = ProtectedFixture::home();
    let policy = codex_diagnostic_run_policy(&work_dir);

    let inner = format!(
        "{} sandbox -c sandbox_mode=\"danger-full-access\" -- cat {}",
        codex.display(),
        protected.path.display()
    );
    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["sh", "-c", &inner],
    );
    assert!(
        !output.contains("PROTECTED-HOME-CONTENT"),
        "escalation through a nested codex must not leak: {output}"
    );
    let _deadline = Instant::now() + Duration::from_secs(1);
    assert_ne!(code.unwrap_or(-1), 0, "the escalation must not succeed");
    let _ = std::fs::remove_dir_all(&work_dir);
}

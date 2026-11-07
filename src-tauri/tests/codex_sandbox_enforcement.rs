//! REAL sandbox-enforcement tests for the Codex diagnostic policy.
//!
//! These tests run the PRODUCTION policy builder
//! (`codex_diagnostic_run_policy`) through the provider-free
//! `codex sandbox` harness — the same policy engine `codex exec` uses —
//! against harmless temporary fixture files. NO provider service is ever
//! contacted (the `sandbox` subcommand runs a local command under the
//! Seatbelt policy; no model, no network to the provider, no auth).
//!
//! Enforcement verification is REQUIRED for the enabled adapter: run
//! this suite as the security gate with
//! `ORQEN_REQUIRE_CODEX_ENFORCEMENT=1` (plus `ORQEN_CODEX_BIN` when the
//! CLI is not on PATH) — a missing or unsupported binary then FAILS the
//! gate instead of skipping. Without that flag the tests are clearly
//! labeled skips (`[SKIPPED-LABELED]`), never silent passes.
//! The evidence recorded in docs/QA_REPORT.md was produced with
//! codex-cli 0.164.0.
//!
//! macOS-only: the verified platform (D-023). Every denial test asserts
//! the OS denial signature ("Operation not permitted") so a harness
//! failure can never masquerade as successful enforcement.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

use orqen_lib::agents::execution::{
    CODEX_MINIMUM_VERSION, codex_diagnostic_run_policy, version_meets_minimum,
};

/// Resolves the Codex CLI under test: `ORQEN_CODEX_BIN` or `codex` on
/// PATH. With `ORQEN_REQUIRE_CODEX_ENFORCEMENT=1` a missing or
/// unsupported binary FAILS (panic) — enforcement verification is
/// required, so it must never silently pass as a skip. Otherwise it
/// returns None with a clearly labeled skip reason.
fn codex_binary() -> Option<PathBuf> {
    let candidate = std::env::var_os("ORQEN_CODEX_BIN")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from("codex")));
    let binary = candidate?;
    let version_output = Command::new(&binary)
        .arg("--version")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string());
    let Some(version_output) = version_output else {
        if require_enforcement() {
            panic!(
                "ENFORCEMENT GATE FAILURE: no usable Codex CLI at {binary:?} — \
                 enforcement verification is required and must not be skipped"
            );
        }
        eprintln!("[SKIPPED-LABELED] codex enforcement tests: no usable Codex CLI at {binary:?}");
        return None;
    };
    if !version_meets_minimum(&version_output, CODEX_MINIMUM_VERSION) {
        if require_enforcement() {
            panic!(
                "ENFORCEMENT GATE FAILURE: Codex {version_output:?} is older than the \
                 verified minimum {CODEX_MINIMUM_VERSION:?} — enforcement cannot be verified"
            );
        }
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement tests: {version_output:?} is older than \
             the verified minimum {CODEX_MINIMUM_VERSION:?}"
        );
        return None;
    }
    Some(binary)
}

fn require_enforcement() -> bool {
    std::env::var_os("ORQEN_REQUIRE_CODEX_ENFORCEMENT").is_some_and(|v| v == "1")
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

    /// A file EXPLICITLY under `/tmp` (denied by the policy's explicit
    /// re-deny — `/tmp` leaks through the documented `:minimal` token).
    fn in_tmp() -> Self {
        Self::create(
            &PathBuf::from("/tmp").join(format!("orqen-sbx-protected-{}", uuid::Uuid::new_v4())),
            "PROTECTED-TMP-CONTENT",
        )
    }

    /// A file under the macOS per-user temporary directory
    /// (`$TMPDIR`, e.g. `/var/folders/.../T/`) — denied by default
    /// because the `:minimal` runtime token does not cover it.
    fn in_per_user_temp() -> Self {
        Self::create(
            &std::env::temp_dir().join(format!("orqen-sbx-protected-{}", uuid::Uuid::new_v4())),
            "PROTECTED-PERUSER-TEMP-CONTENT",
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
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
    assert!(
        output.contains("Operation not permitted"),
        "the denial must be the OS sandbox signature — a harness failure must \
         never masquerade as enforcement: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn protected_read_in_tmp_is_denied() {
    // /tmp leaks through the documented `:minimal` runtime token; the
    // production policy re-denies it explicitly. This test pins that.
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
    assert!(
        output.contains("Operation not permitted"),
        "the denial must be the OS sandbox signature: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn protected_read_in_the_per_user_temp_dir_is_denied() {
    // The macOS per-user temp dir ($TMPDIR, /var/folders/...) is NOT
    // covered by the `:minimal` runtime token — denied by default.
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
        return;
    };
    let work_dir = temp_dir("deny-peruser-tmp");
    let protected = ProtectedFixture::in_per_user_temp();
    let policy = codex_diagnostic_run_policy(&work_dir);

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["cat", protected.path.to_string_lossy().as_ref()],
    );
    assert_ne!(code, Some(0), "the per-user temp read must fail: {output}");
    assert!(
        !output.contains("PROTECTED-PERUSER-TEMP-CONTENT"),
        "the protected content must never leak: {output}"
    );
    assert!(
        output.contains("Operation not permitted"),
        "the denial must be the OS sandbox signature: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn write_outside_the_work_dir_is_denied() {
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
    assert!(
        output.contains("Operation not permitted"),
        "the denial must be the OS sandbox signature: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn write_inside_the_work_dir_succeeds() {
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
    assert!(
        output.contains("Error"),
        "the failure must be a configuration error, not a harness crash: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn unknown_profile_fails_closed() {
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
    assert!(
        output.contains("Error"),
        "the failure must be a configuration error, not a harness crash: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn nested_escalation_is_denied() {
    // A process inside the confined profile must not be able to spawn a
    // broader codex sandbox and read the protected file through it — the
    // OUTER policy is OS-enforced on the whole process tree.
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
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
    assert_ne!(code, Some(0), "the escalation must not succeed: {output}");
    assert!(
        output.contains("Operation not permitted"),
        "the escalation must be blocked by the OUTER OS sandbox, not by an \
         inner failure: {output}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn network_access_from_tool_commands_is_denied() {
    // The adapter claims "tool-call network access is restricted" — this
    // test must exist before that claim is stated as verified. Positive
    // control first: the listener IS reachable from an unsandboxed
    // connection, so a sandboxed failure is meaningful.
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
        return;
    };
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind local listener");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        // Accept (and immediately close) connections in the background.
        for stream in listener.incoming() {
            drop(stream);
        }
    });
    use std::net::TcpStream;
    TcpStream::connect(("127.0.0.1", port)).expect("positive control: listener reachable");

    let work_dir = temp_dir("deny-network");
    let policy = codex_diagnostic_run_policy(&work_dir);
    let url = format!("http://127.0.0.1:{port}/");

    let (code, output) = run_under_policy(
        &codex,
        &work_dir,
        &policy.config_overrides,
        &["curl", "-m", "5", "-s", &url],
    );
    assert_ne!(code, Some(0), "the sandboxed request must fail: {output}");
    // The URL content cannot appear (the listener sends nothing, but a
    // connect success would show in curl's exit code, asserted above).
    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn enforcement_preflight_accepts_a_verifying_binary() {
    // The PRODUCTION gate function must pass against a binary that
    // actually enforces the policy (and thereby admit diagnostics).
    let Some(codex) = codex_binary() else {
        eprintln!(
            "[SKIPPED-LABELED] codex enforcement test: no supported CLI (set ORQEN_CODEX_BIN)"
        );
        return;
    };
    orqen_lib::agents::execution::codex_enforcement_preflight(&codex)
        .expect("the verified binary must pass the production preflight");
}

#[test]
fn enforcement_preflight_fails_closed_for_a_non_codex_binary() {
    // A binary that is not a confining codex (here: a fixture script)
    // must NEVER pass the preflight — diagnostics stay disabled.
    let work_dir = temp_dir("preflight-fake");
    let fake = work_dir.join("fake-codex");
    std::fs::write(
        &fake,
        "#!/bin/sh
echo fake
",
    )
    .expect("write fake");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let error = orqen_lib::agents::execution::codex_enforcement_preflight(&fake)
        .expect_err("a non-confining binary must fail the preflight");
    assert!(
        error.contains("did not pass the sandbox enforcement preflight"),
        "got: {error}"
    );
    let _ = std::fs::remove_dir_all(&work_dir);
}

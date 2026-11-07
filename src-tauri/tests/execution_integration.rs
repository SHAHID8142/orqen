//! Backend integration tests for controlled diagnostic execution.
//!
//! These tests exercise the REAL `RunRegistry`/runner path end to end
//! against controlled shell-script fixtures placed first on `PATH` —
//! never a real third-party CLI. Covered:
//!
//! - a successful run reports the fixture's truthful output;
//! - the diagnostic process executes inside the application-managed
//!   isolated working directory (`<data>/diagnostic-runs/<run_id>`),
//!   NOT the caller's directory (filesystem boundary evidence);
//! - cancellation terminates the run promptly and reports `cancelled`;
//! - finished-run retention is bounded (older results are pruned);
//! - reader-slot exhaustion fails a run before any spawn, and the pool
//!   recovers once the reservations are released;
//! - simultaneous starts admit exactly one run.
//!
//! Unix-only: the fixtures are POSIX shell scripts and the process-group
//! guarantees under test are the Unix `process_group(0)` + SIGKILL path.
//! Windows behavior (direct-child kill only) is a documented limitation
//! in docs/ARCHITECTURE.md and is not verifiable from this environment.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Barrier, Mutex};
use std::time::{Duration, Instant};

use orqen_lib::agents::definitions::{AgentCapabilities, AgentDefinition};
use orqen_lib::agents::eligibility::ExecutableIdentity;
use orqen_lib::agents::runner::{RunOutcome, RunRegistry, RunStatusDto};

/// Fixture definition: adapter id `codex` (so the enabled adapter is
/// selected) but discovery resolves to the per-test shell fixture — the
/// automated suites never execute a real provider CLI.
static FIXTURE_CODEX: AgentDefinition = AgentDefinition {
    id: "codex",
    display_name: "Fixture Codex (integration test)",
    provider: "Test",
    adapter: "cli",
    executable_name: "orqen-fixture-codex",
    capabilities: AgentCapabilities {
        discovery: true,
        verification: true,
        execution: true,
        orchestration: false,
    },
};

/// PATH is process-global: tests that place a fixture on it must hold
/// this lock for their whole body, or parallel tests could observe each
/// other's fixtures.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Prepends `dir` to `PATH` for the test's duration and restores the
/// previous value on drop (also on panic).
struct PathGuard {
    previous: std::ffi::OsString,
}

impl PathGuard {
    fn fixture_first(dir: &Path) -> Self {
        let previous = std::env::var_os("PATH").unwrap_or_default();
        let next = std::env::join_paths(
            std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&previous)),
        )
        .expect("join fixture PATH");
        // Edition 2024: PATH mutation is process-global.
        unsafe { std::env::set_var("PATH", &next) };
        PathGuard { previous }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        unsafe { std::env::set_var("PATH", &self.previous) };
    }
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("orqen-exec-it-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Writes a controlled, executable fixture script into `dir/bin`.
fn write_fixture(data_dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = data_dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create fixture bin dir");
    let path = bin.join(name);
    std::fs::write(&path, body).expect("write fixture script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("chmod fixture script");
    path
}

/// The verified identity of this suite's fixture (as the eligibility
/// ledger would have recorded it at verification time).
fn fixture_identity(data_dir: &Path) -> ExecutableIdentity {
    ExecutableIdentity::observe(&data_dir.join("bin").join("orqen-fixture-codex"))
        .expect("observe fixture identity")
}

/// Polls until the run reports a terminal outcome (bounded), returning
/// the recorded outcome.
fn wait_for_finished(
    registry: &RunRegistry,
    run_id: &str,
    max: Duration,
) -> orqen_lib::agents::runner::RunOutcomeDto {
    let deadline = Instant::now() + max;
    loop {
        match registry.status(run_id) {
            Some(RunStatusDto::Finished { result }) => return result,
            Some(_) => {
                assert!(
                    Instant::now() < deadline,
                    "run {run_id} did not finish within {max:?}"
                );
            }
            None => panic!("run {run_id} vanished from the registry"),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn successful_run_reports_the_fixture_output_truthfully() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    write_fixture(
        &dir,
        "orqen-fixture-codex",
        "#!/bin/sh\necho \"fixture agent available\"\n",
    );
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        .expect("start");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));

    assert_eq!(result.outcome, RunOutcome::Succeeded);
    assert_eq!(result.output.trim(), "fixture agent available");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.agent_id, "codex");
    assert!(result.error.is_none(), "success must carry no error");

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn diagnostic_process_runs_inside_the_isolated_directory() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    // The fixture prints its working directory — direct evidence of the
    // filesystem boundary (the process CANNOT be told where to run; the
    // runner sets `current_dir`).
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\npwd\n");
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        .expect("start");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));

    assert_eq!(
        result.outcome,
        RunOutcome::Succeeded,
        "error: {:?}",
        result.error
    );
    let cwd = result.output.trim();
    assert!(
        cwd.ends_with(&format!("diagnostic-runs/{run_id}"))
            || cwd.ends_with(&format!("diagnostic-runs/{run_id}/")),
        "the diagnostic must run in the isolated directory, got cwd: {cwd}"
    );

    // The isolated directory itself is removed after the run.
    let work_dir = dir.join("diagnostic-runs").join(&run_id);
    assert!(
        !work_dir.exists(),
        "the isolated working directory must be removed after the run"
    );

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelled_run_terminates_promptly_and_reports_cancelled() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\nsleep 30\n");
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        .expect("start");
    registry.cancel(&run_id).expect("cancel");
    let started = Instant::now();
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));

    assert_eq!(result.outcome, RunOutcome::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancellation must terminate the run promptly"
    );
    // The tree was SIGKILLed: no normal exit code can exist (unix signal).
    assert_eq!(result.exit_code, None, "a killed process has no exit code");

    let work_dir = dir.join("diagnostic-runs").join(&run_id);
    assert!(!work_dir.exists(), "cleanup must remove the work dir");

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn finished_run_retention_is_bounded() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\necho ok\n");
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    // One more finished run than the retention cap: the oldest must be
    // pruned from the registry (bounded memory over a long session).
    let mut ids = Vec::new();
    for _ in 0..17 {
        let run_id = registry
            .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
            .expect("start");
        wait_for_finished(&registry, &run_id, Duration::from_secs(15));
        ids.push(run_id);
    }
    assert_eq!(ids.len(), 17);
    assert!(
        registry.status(&ids[0]).is_none(),
        "the oldest finished run must be pruned"
    );
    let recent = registry
        .status(&ids[16])
        .expect("the most recent run must remain queryable");
    assert!(matches!(recent, RunStatusDto::Finished { .. }));

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reader_slot_exhaustion_fails_the_run_before_spawn_and_recovers() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\necho ok\n");
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    // Reserve the whole reader pool (pairs until none remain — the
    // failing call releases its own first slot internally).
    let mut held = Vec::new();
    while let Some(pair) = orqen_lib::agents::verify::acquire_reader_slot_pair() {
        held.push(pair);
    }
    assert!(!held.is_empty(), "the pool must have initial capacity");
    assert_eq!(orqen_lib::agents::verify::available_reader_slots(), 0);

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        .expect("start admits");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));
    assert_eq!(result.outcome, RunOutcome::Failed);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("resource limit"),
        "exhaustion must be reported, got: {:?}",
        result.error
    );

    // Recovery: releasing the reservations restores capacity, and a new
    // run executes normally.
    drop(held);
    assert_eq!(orqen_lib::agents::verify::available_reader_slots(), 64);
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        .expect("start recovers");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));
    assert_eq!(result.outcome, RunOutcome::Succeeded);

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simultaneous_starts_admit_exactly_one() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = std::sync::Arc::new(temp_dir());
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\nsleep 30\n");
    // PATH must already contain the fixture before the threads start
    // (mutation after spawn would race the workers' discovery).
    let previous_path = std::env::var_os("PATH").unwrap_or_default();
    let next = std::env::join_paths(
        std::iter::once(dir.join("bin")).chain(std::env::split_paths(&previous_path)),
    )
    .expect("join PATH");
    unsafe { std::env::set_var("PATH", &next) };

    let registry = std::sync::Arc::new(RunRegistry::default());
    let barrier = std::sync::Arc::new(Barrier::new(6));
    let mut handles = Vec::new();
    for _ in 0..6 {
        let registry = registry.clone();
        let barrier = barrier.clone();
        let dir = dir.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            registry.start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        }));
    }
    let results: Vec<Result<String, String>> = handles
        .into_iter()
        .map(|h| h.join().expect("join"))
        .collect();
    let admitted: Vec<String> = results
        .iter()
        .filter_map(|r| r.as_ref().ok().cloned())
        .collect();
    assert_eq!(admitted.len(), 1, "exactly one start may be admitted");
    for rejected in results.iter().filter(|r| r.is_err()) {
        assert!(
            rejected
                .as_ref()
                .expect_err("rejected")
                .contains("already in progress"),
            "rejections must explain the busy state"
        );
    }

    // Leave a clean state: cancel the admitted run and wait for it.
    registry.cancel(&admitted[0]).expect("cancel admitted run");
    let result = wait_for_finished(&registry, &admitted[0], Duration::from_secs(15));
    assert_eq!(result.outcome, RunOutcome::Cancelled);

    unsafe { std::env::set_var("PATH", &previous_path) };
    let _ = std::fs::remove_dir_all(&*dir);
}

#[test]
fn swapped_executable_is_rejected_before_spawn() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\necho original\n");
    let identity = fixture_identity(&dir);
    // The executable changes AFTER verification but BEFORE execution.
    write_fixture(&dir, "orqen-fixture-codex", "#!/bin/sh\necho REPLACED\n");
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &identity)
        .expect("start is admitted; the runner must reject it");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));

    assert_eq!(result.outcome, RunOutcome::Failed);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("changed since it was verified"),
        "got: {:?}",
        result.error
    );
    assert!(
        !result.output.contains("REPLACED"),
        "the swapped executable must never have been spawned"
    );

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn executable_at_a_different_path_is_rejected() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    // The identity was verified for the fixture in `bin`; a DIFFERENT
    // directory (same executable name) is now first on PATH.
    let verified_bin = dir.join("bin");
    write_fixture(
        &dir,
        "orqen-fixture-codex",
        "#!/bin/sh\necho verified-one\n",
    );
    let identity = ExecutableIdentity::observe(&verified_bin.join("orqen-fixture-codex"))
        .expect("observe verified identity");

    let bin2 = dir.join("bin2");
    std::fs::create_dir_all(&bin2).expect("bin2");
    let second = bin2.join("orqen-fixture-codex");
    std::fs::write(&second, "#!/bin/sh\necho impostor\n").expect("write impostor");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&second, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let previous = std::env::var_os("PATH").unwrap_or_default();
    let next =
        std::env::join_paths(std::iter::once(bin2.clone()).chain(std::env::split_paths(&previous)))
            .expect("join PATH");
    unsafe { std::env::set_var("PATH", &next) };
    struct Restore(std::ffi::OsString);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe { std::env::set_var("PATH", &self.0) };
        }
    }
    let _restore = Restore(previous);

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &identity)
        .expect("start is admitted; the runner must reject it");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));

    assert_eq!(result.outcome, RunOutcome::Failed);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("not the one that was verified"),
        "got: {:?}",
        result.error
    );
    assert!(
        !result.output.contains("impostor"),
        "the impostor executable must never have been spawned"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn descendants_are_terminated_after_a_normal_exit() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = temp_dir();
    // The parent exits SUCCESSFULLY while a descendant keeps running in
    // the same process group, holding the inherited pipes. The runner
    // must terminate the whole group even on the success path.
    write_fixture(
        &dir,
        "orqen-fixture-codex",
        "#!/bin/sh\nsleep 60 &\necho \"descendant: $!\"\n",
    );
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    let run_id = registry
        .start(&FIXTURE_CODEX, &dir, &fixture_identity(&dir))
        .expect("start");
    let result = wait_for_finished(&registry, &run_id, Duration::from_secs(15));

    // The parent's normal exit still yields a truthful success.
    assert_eq!(
        result.outcome,
        RunOutcome::Succeeded,
        "error: {:?}",
        result.error
    );
    let descendant_pid: u32 = result
        .output
        .trim()
        .strip_prefix("descendant: ")
        .expect("fixture reports the descendant pid")
        .trim()
        .parse()
        .expect("pid is numeric");

    // No lingering descendant: poll until the pid no longer exists.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let alive = std::process::Command::new("ps")
            .args(["-p", &descendant_pid.to_string(), "-o", "pid="])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !alive {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "descendant {descendant_pid} must be terminated after the parent's normal exit"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // The work directory is removed and no orphan holds the pipes.
    let work_dir = dir.join("diagnostic-runs").join(&run_id);
    assert!(!work_dir.exists());

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

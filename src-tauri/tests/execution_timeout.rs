//! Integration test for the diagnostic-run TIMEOUT path.
//!
//! Compiled only with the test-only `test-timeouts` feature (never
//! enabled by the app or release builds): it shrinks the 120 s deadline
//! via `TEST_DEADLINE_OVERRIDE` so a hung fixture can be observed to
//! hit the deadline in seconds. The deadline, the process-group kill,
//! reader draining, child reaping, PID clearing, and work-dir removal
//! are the production code paths being exercised.

#![cfg(all(unix, feature = "test-timeouts"))]

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use orqen_lib::agents::definitions::{AgentCapabilities, AgentDefinition};
use orqen_lib::agents::runner::{RunOutcome, RunRegistry, RunStatusDto, TEST_DEADLINE_OVERRIDE};

static FIXTURE_CODEX: AgentDefinition = AgentDefinition {
    id: "codex",
    display_name: "Fixture Codex (timeout test)",
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

static ENV_LOCK: Mutex<()> = Mutex::new(());

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
        unsafe { std::env::set_var("PATH", &next) };
        PathGuard { previous }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        unsafe { std::env::set_var("PATH", &self.previous) };
    }
}

struct DeadlineGuard;

impl DeadlineGuard {
    fn set(d: Duration) -> Self {
        *TEST_DEADLINE_OVERRIDE
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(d);
        DeadlineGuard
    }
}

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        *TEST_DEADLINE_OVERRIDE
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    }
}

#[test]
fn hung_run_hits_the_deadline_and_is_terminated() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _deadline = DeadlineGuard::set(Duration::from_secs(2));

    let dir = std::env::temp_dir().join(format!("orqen-exec-timeout-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create fixture bin dir");
    let script = bin.join("orqen-fixture-codex");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").expect("write fixture");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod fixture");
    let _path = PathGuard::fixture_first(&dir.join("bin"));

    let registry = RunRegistry::default();
    let started = Instant::now();
    let run_id = registry.start(&FIXTURE_CODEX, &dir).expect("start");

    let deadline = started + Duration::from_secs(15);
    let result = loop {
        match registry.status(&run_id) {
            Some(RunStatusDto::Finished { result }) => break result,
            Some(_) => assert!(
                Instant::now() < deadline,
                "the hung run must hit the 2 s deadline"
            ),
            None => panic!("run vanished"),
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    assert_eq!(result.outcome, RunOutcome::TimedOut);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("timed out"),
        "the timeout must be reported, got: {:?}",
        result.error
    );
    assert!(
        result.duration_ms >= 1_500,
        "the run must actually have run to the deadline, got {} ms",
        result.duration_ms
    );
    assert_eq!(result.exit_code, None, "a killed process has no exit code");
    let work_dir = dir.join("diagnostic-runs").join(&run_id);
    assert!(!work_dir.exists(), "cleanup must remove the work dir");

    drop(_path);
    let _ = std::fs::remove_dir_all(&dir);
}

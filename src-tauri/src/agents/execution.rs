//! Diagnostic execution adapters.
//!
//! A provider-independent execution contract with one narrow, controlled
//! purpose: run a FIXED noninteractive diagnostic prompt through an
//! installed, verified CLI and report the truthful outcome.
//!
//! Boundaries:
//! - The prompt is a compile-time constant — no user-supplied text reaches
//!   the process arguments.
//! - The diagnostic runs in an application-managed, isolated working
//!   directory that contains no user project data.
//! - Each adapter owns its per-run confinement policy; an adapter that
//!   cannot establish and verify adequate restrictions must report
//!   `supports_safe_diagnostics() == false` and stay disabled.
//! - Authentication always comes from the CLI's own configuration; no
//!   credentials are read, stored, or logged.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The fixed diagnostic prompt. Deliberately not configurable: this
/// feature validates the execution architecture, not the model.
pub const DIAGNOSTIC_PROMPT: &str = "Respond with a short confirmation that \
the agent is available. Do not read, modify, create, or delete any local \
project files.";

/// The minimum Codex CLI version for which the per-run read-confinement
/// profile is VERIFIED (0.164.0 — the version used for the enforcement
/// evidence in docs/QA_REPORT.md). Older versions fail closed at the
/// eligibility gate: they must never receive a diagnostic.
pub const CODEX_MINIMUM_VERSION: (u64, u64, u64) = (0, 164, 0);

/// Adapter contract: how one supported CLI is invoked noninteractively.
///
/// Implementations own their provider-specific argument construction and
/// are unit-tested. The execution runner stays provider-agnostic.
pub trait ExecutionAdapter: Send + Sync {
    /// Stable id — matches the registry definition id.
    fn id(&self) -> &'static str;
    /// The noninteractive argument list AFTER the executable. Built from
    /// compile-time constants, the fixed diagnostic prompt, and the
    /// adapter's own per-run confinement policy for `work_dir`.
    fn build_arguments(&self, work_dir: &Path) -> Vec<String>;
    /// Whether this adapter's invocation establishes adequate
    /// least-privilege restrictions for a diagnostic run. Adapters that
    /// cannot must report false and stay disabled.
    fn supports_safe_diagnostics(&self) -> bool;
    /// Whether starting a diagnostic requires a provider-free sandbox
    /// enforcement preflight against the verified executable. The
    /// confinement mechanism is documented BETA ("under active
    /// development and may change"), so safety must be proven on the
    /// actual installed binary — never inferred from the version number
    /// alone.
    fn requires_enforcement_preflight(&self) -> bool {
        false
    }
    /// The minimum CLI version the confinement policy requires, when the
    /// policy is version-gated. This is a compatibility FLOOR (the flags
    /// and profile schema must exist), never a safety claim — the
    /// enforcement preflight is the actual gate.
    fn minimum_cli_version(&self) -> Option<(u64, u64, u64)> {
        None
    }
    /// Short human-readable summary of the applied restrictions.
    fn restriction_summary(&self) -> &'static str;
    /// Why safe diagnostics are not supported (empty string when they
    /// are supported).
    fn unsafe_diagnostics_reason(&self) -> &'static str {
        ""
    }
    /// Parses the raw process result into a truthful agent response or an
    /// error. Provider-specific: machine-readable output modes are used
    /// where documented, and provider/SDK failure signals (even with exit
    /// code 0) are classified as failures — never as success.
    fn parse_result(
        &self,
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<String, String>;
}

/// Quotes a string as a TOML basic string (the `-c` override values are
/// parsed as TOML).
fn toml_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// Extracts a (major, minor, patch) triple from a `--version` output
/// such as `codex-cli 0.164.0`. `None` when no version token exists —
/// the caller must fail closed.
fn parse_semver_triple(version_output: &str) -> Option<(u64, u64, u64)> {
    version_output.split_whitespace().find_map(|token| {
        let mut parts = token.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        Some((major, minor, patch))
    })
}

/// Whether a `--version` output denotes a CLI at least `minimum`.
/// Unparseable output fails closed (false).
pub fn version_meets_minimum(version_output: &str, minimum: (u64, u64, u64)) -> bool {
    parse_semver_triple(version_output)
        .map(|triple| triple >= minimum)
        .unwrap_or(false)
}

/// The per-run confinement policy for one Codex diagnostic: the `-c`
/// overrides that DEFINE an app-owned, read-deny-by-default permissions
/// profile and SELECT it for this invocation only.
///
/// Guarantees (verified on macOS Seatbelt, codex-cli 0.164.0 — see
/// docs/QA_REPORT.md):
/// - filesystem reads are denied everywhere except the platform/runtime
///   paths covered by the documented `:minimal` token and the diagnostic
///   directory; `/tmp` is explicitly re-denied (it leaks through
///   `:minimal`);
/// - filesystem writes are granted ONLY inside the diagnostic directory;
/// - the profile name carries a per-run random suffix, so it can never
///   collide with an entry in the user's global configuration — nothing
///   is read from or written to the user's configuration, and no
///   inherited profile can broaden this one;
/// - nothing here passes `--sandbox`/`sandbox_mode`: mixing those with
///   permissions profiles makes Codex silently fall back to the legacy
///   (write-only) sandbox, which would defeat read confinement.
pub struct CodexRunPolicy {
    /// `-c key=value` overrides to place on the `codex` invocation.
    pub config_overrides: Vec<String>,
    /// The profile name, for tooling that selects a profile explicitly
    /// (`codex sandbox -P <name>` — used by the enforcement tests).
    pub profile_name: String,
}

/// Builds the confinement policy for one diagnostic run. The user's
/// global configuration is never modified.
pub fn codex_diagnostic_run_policy(work_dir: &Path) -> CodexRunPolicy {
    // Per-run random suffix: a static name could collide with an
    // entry the user's global configuration defines, and (because a
    // more-specific rule can reopen a broader deny) such a collision
    // could GRANT reads. A fresh random name makes that impossible.
    let profile_name = format!(
        "orqen-diag-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    let work_dir_toml = toml_quote(&work_dir.to_string_lossy());
    let profile_definition = format!(
        "permissions.{profile_name}={{filesystem={{\":root\"=\"deny\",\
         \":minimal\"=\"read\",\"/tmp\"=\"deny\",{work_dir_toml}=\"write\"}}}}",
        profile_name = profile_name,
    );
    CodexRunPolicy {
        config_overrides: vec![
            profile_definition,
            format!("default_permissions={}", toml_quote(&profile_name)),
            // Tool-call children get only the documented core environment
            // (PATH-like basics), never arbitrary inherited variables that
            // could carry credentials, and credential-shaped names are
            // excluded even from that.
            "shell_environment_policy.inherit=\"core\"".to_string(),
            "shell_environment_policy.exclude=[\"*KEY*\",\"*TOKEN*\",\"*SECRET*\",\"*PASSWORD*\",\"*CREDENTIAL*\"]"
                .to_string(),
        ],
        profile_name,
    }
}

/// Hard timeout for one preflight probe (the sandbox harness normally
/// completes in well under a second).
const PREFLIGHT_PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs a command to completion with a hard timeout, returning its exit
/// code and combined output. A timed-out child is killed (fail closed).
/// Probes produce tiny output, so reading the pipes after exit cannot
/// deadlock.
fn run_probe(mut command: Command, timeout: Duration) -> Result<(Option<i32>, String), String> {
    use std::io::Read;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start the sandbox probe: {e}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut combined = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut combined);
                }
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut combined);
                }
                return Ok((status.code(), combined));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("the sandbox probe timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("the sandbox probe failed: {e}")),
        }
    }
}

/// Removes every preflight fixture on drop (also on error paths).
struct PreflightFixtures {
    work_dir: PathBuf,
    protected: PathBuf,
    write_target: PathBuf,
}

impl Drop for PreflightFixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work_dir);
        let _ = std::fs::remove_file(&self.protected);
        let _ = std::fs::remove_file(&self.write_target);
    }
}

/// Provider-free sandbox enforcement preflight (D-024): proves that the
/// ACTUAL installed CLI at `executable` enforces the diagnostic
/// confinement policy before a run is admitted. Runs the production
/// policy builder through `codex sandbox` — the same policy engine
/// `codex exec` uses — against harmless temporary fixtures; NO provider
/// service is contacted.
///
/// Probes, in order:
/// 1. positive control — the harness accepts the policy arguments and
///    the permitted work-dir read succeeds (this is what distinguishes
///    a real denial from "the command never ran");
/// 2. a protected read OUTSIDE the granted roots is denied and its
///    content never appears in the output;
/// 3. a write outside the work dir is denied and creates nothing;
/// 4. an invalid profile is a hard error (the fail-closed mechanism is
///    alive, not silently ignored).
///
/// Any violation, harness error, or timeout fails closed: the caller
/// must refuse to start the diagnostic.
pub fn codex_enforcement_preflight(executable: &Path) -> Result<(), String> {
    let fixtures = PreflightFixtures {
        work_dir: std::env::temp_dir().join(format!("orqen-preflight-{}", uuid::Uuid::new_v4())),
        protected: std::env::temp_dir().join(format!(
            "orqen-preflight-protected-{}",
            uuid::Uuid::new_v4()
        )),
        write_target: std::env::temp_dir()
            .join(format!("orqen-preflight-write-{}", uuid::Uuid::new_v4())),
    };
    std::fs::create_dir_all(&fixtures.work_dir)
        .map_err(|e| format!("could not create the preflight work dir: {e}"))?;
    std::fs::write(
        fixtures.work_dir.join("orqen-preflight-allowed.txt"),
        "orqen-preflight-ok",
    )
    .map_err(|e| format!("could not write the preflight fixture: {e}"))?;
    std::fs::write(&fixtures.protected, "orqen-preflight-protected-content")
        .map_err(|e| format!("could not write the preflight fixture: {e}"))?;

    let policy = codex_diagnostic_run_policy(&fixtures.work_dir);
    let sandbox_probe =
        |overrides: &[String], args: &[&str]| -> Result<(Option<i32>, String), String> {
            let mut command = Command::new(executable);
            command.arg("sandbox");
            for override_value in overrides {
                command.arg("-c").arg(override_value);
            }
            command.arg("--").args(args).current_dir(&fixtures.work_dir);
            run_probe(command, PREFLIGHT_PROBE_TIMEOUT)
        };
    let excerpt = |output: &str| -> String { output.trim().chars().take(200).collect() };

    // 1. Positive control: the harness runs with our policy arguments
    // and the permitted read succeeds.
    let (code, output) = sandbox_probe(
        &policy.config_overrides,
        &["cat", "orqen-preflight-allowed.txt"],
    )?;
    if code != Some(0) || !output.contains("orqen-preflight-ok") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: the permitted diagnostic read did not succeed ({})",
            excerpt(&output)
        ));
    }

    // 2. A protected read outside the granted roots must be denied —
    // the content must never appear.
    let protected_arg = fixtures.protected.to_string_lossy().to_string();
    let (code, output) = sandbox_probe(&policy.config_overrides, &["cat", &protected_arg])?;
    if code == Some(0) || output.contains("orqen-preflight-protected-content") {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: a protected file outside the diagnostic \
             directory was readable ({})",
            excerpt(&output)
        ));
    }

    // 3. A write outside the work dir must be denied and create nothing.
    let write_arg = fixtures.write_target.to_string_lossy().to_string();
    let (code, output) = sandbox_probe(&policy.config_overrides, &["touch", &write_arg])?;
    if code == Some(0) || fixtures.write_target.exists() {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: a file could be created outside the diagnostic \
             directory ({})",
            excerpt(&output)
        ));
    }

    // 4. An invalid profile must be a hard error — the fail-closed
    // mechanism is alive (an update that silently IGNORED an unknown
    // value would otherwise downgrade the policy without notice).
    let mut broken_overrides = policy.config_overrides.clone();
    broken_overrides[0] = broken_overrides[0].replace("\"deny\"", "\"bogus-value\"");
    let (code, output) = sandbox_probe(&broken_overrides, &["echo", "preflight-should-not-run"])?;
    if code == Some(0) {
        return Err(format!(
            "the installed CLI did not pass the sandbox enforcement \
             preflight: an invalid permissions profile was accepted \
             instead of failing closed ({})",
            excerpt(&output)
        ));
    }

    Ok(())
}

/// OpenAI Codex CLI: `codex exec` is the documented noninteractive mode.
///
/// Per-run confinement (D-023): the invocation loads NONE of the user's
/// configuration (`--ignore-user-config` — auth still resolves from
/// `CODEX_HOME`), loads no execpolicy rules (`--ignore-rules`), persists
/// no session files (`--ephemeral`), and applies an app-owned,
/// read-deny-by-default permissions profile defined entirely via `-c`
/// overrides (the highest-precedence configuration layer). The legacy
/// `--sandbox` flag is deliberately NOT passed: mixing it with a
/// permissions profile silently reverts to the legacy write-only
/// sandbox.
///
/// Verified on macOS Seatbelt (codex-cli 0.164.0, `codex sandbox`
/// enforcement harness — see docs/QA_REPORT.md). Other platforms fail
/// closed until independently verified there.
pub struct CodexAdapter;

impl ExecutionAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn build_arguments(&self, work_dir: &Path) -> Vec<String> {
        let policy = codex_diagnostic_run_policy(work_dir);
        let mut args = vec![
            "exec".to_string(),
            // Nothing inherited: no MCP servers, hooks, plugins, model
            // providers, or profiles from the user's configuration.
            "--ignore-user-config".to_string(),
            "--ignore-rules".to_string(),
            "--ephemeral".to_string(),
            // The isolated working directory is not a git repository.
            "--skip-git-repo-check".to_string(),
            // Unknown configuration fields are a hard error, never a
            // silent ignore — a schema change in a future CLI cannot
            // silently drop part of the confinement profile.
            "--strict-config".to_string(),
        ];
        // Each override is its own `-c key=value` pair.
        args.extend(
            policy
                .config_overrides
                .iter()
                .flat_map(|override_value| ["-c".to_string(), override_value.clone()]),
        );
        args.push(DIAGNOSTIC_PROMPT.to_string());
        args
    }

    fn supports_safe_diagnostics(&self) -> bool {
        // The read-confinement profile is VERIFIED on macOS Seatbelt
        // only (D-023). Linux (Landlock/bubblewrap) and Windows
        // enforcement are not verified — fail closed there.
        cfg!(target_os = "macos")
    }

    fn requires_enforcement_preflight(&self) -> bool {
        // The permissions mechanism is documented BETA: every diagnostic
        // start must PROVE the installed binary enforces the confinement
        // policy (D-024) — the version floor is only a compatibility
        // guard, never a safety claim.
        cfg!(target_os = "macos")
    }

    fn minimum_cli_version(&self) -> Option<(u64, u64, u64)> {
        Some(CODEX_MINIMUM_VERSION)
    }

    fn unsafe_diagnostics_reason(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            ""
        } else {
            "diagnostic execution for Codex is disabled on this platform: \
             the per-run read-confinement profile is verified on macOS \
             (Seatbelt) only — enforcement here is unverified, so the \
             adapter fails closed"
        }
    }

    fn restriction_summary(&self) -> &'static str {
        "Runs noninteractively with an app-owned per-run permissions \
         profile: filesystem reads and writes are denied outside the \
         diagnostic directory (platform/runtime paths stay readable; \
         /tmp is explicitly denied), tool-call network access is \
         restricted, the tool-call environment carries no arbitrary \
         inherited variables, and your global Codex configuration is \
         not loaded."
    }

    fn parse_result(
        &self,
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<String, String> {
        // Documented contract: `codex exec` exits nonzero on provider and
        // sandbox errors; the agent's final message is printed to stdout.
        match exit_code {
            Some(0) => {
                let text = stdout.trim();
                if text.is_empty() {
                    Err("the CLI exited successfully but produced no output.".to_string())
                } else {
                    Ok(text.chars().take(8000).collect())
                }
            }
            Some(code) => {
                let excerpt = stderr_excerpt_of(stderr);
                Err(format!("the CLI exited with code {code}.{excerpt}"))
            }
            None => Err("the CLI was terminated by a signal.".to_string()),
        }
    }
}

/// Anthropic Claude Code CLI: `claude -p` is the documented print mode
/// (noninteractive) and `--output-format json` is the documented
/// machine-readable mode.
///
/// Diagnostic execution is currently DISABLED for this adapter: the CLI
/// inherits the user's global configuration (settings, hooks, MCP servers,
/// plugins), tool-denial lists are version-dependent, and the print mode
/// does not enforce read-only filesystem access — so confidentiality and
/// tool isolation cannot be verified across CLI versions. See
/// docs/ARCHITECTURE.md for the exact boundary and docs/QA_REPORT.md for
/// the real-CLI evidence.
pub struct ClaudeCodeAdapter;

/// Known SDK/config failure signals observed in Claude Code output.
const CLAUDE_ERROR_SIGNALS: &[&str] = &["unrecognized_model", "claude-code:"];

impl ClaudeCodeAdapter {
    /// Classifies captured output: an SDK/config failure signal (even with
    /// exit code 0) is a failure, never a success.
    fn detect_error_signal(output: &str) -> Option<String> {
        for signal in CLAUDE_ERROR_SIGNALS {
            if output.contains(signal) {
                let line = output
                    .lines()
                    .find(|line| line.contains(signal))
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>();
                return Some(format!(
                    "the CLI reported a provider error ({signal}): {line}"
                ));
            }
        }
        None
    }
}

impl ExecutionAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn build_arguments(&self, _work_dir: &Path) -> Vec<String> {
        vec![
            "-p".to_string(),
            DIAGNOSTIC_PROMPT.to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "--disallowedTools".to_string(),
            "Bash,Edit,Write,NotebookEdit".to_string(),
        ]
    }

    fn supports_safe_diagnostics(&self) -> bool {
        false
    }

    fn unsafe_diagnostics_reason(&self) -> &'static str {
        "diagnostic execution for Claude Code is disabled: the CLI inherits \
         the user's global configuration (hooks, MCP servers, plugins) and \
         tool restrictions are version-dependent, so isolation cannot be \
         verified across CLI versions"
    }

    fn restriction_summary(&self) -> &'static str {
        "Diagnostic execution is disabled: confidentiality and tool \
         isolation cannot be verified across CLI versions."
    }

    fn parse_result(
        &self,
        exit_code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<String, String> {
        // Machine-readable mode: stdout is a JSON object with `result` and
        // `is_error` fields (documented). Provider/SDK failure signals are
        // classified as failures even with exit code 0.
        if let Some(signal) = ClaudeCodeAdapter::detect_error_signal(stdout)
            .or_else(|| ClaudeCodeAdapter::detect_error_signal(stderr))
        {
            return Err(signal);
        }
        match exit_code {
            Some(0) => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout);
                match parsed {
                    Ok(value) => {
                        let is_error = value
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let result = value
                            .get("result")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if is_error {
                            Err(format!(
                                "the CLI reported an error: {}",
                                if result.is_empty() {
                                    "unknown error"
                                } else {
                                    &result
                                }
                            ))
                        } else if result.is_empty() {
                            Err("the CLI exited successfully but produced no usable result."
                                .to_string())
                        } else {
                            Ok(result.chars().take(8000).collect())
                        }
                    }
                    Err(_) => {
                        Err("the CLI produced malformed machine-readable output.".to_string())
                    }
                }
            }
            Some(code) => {
                let excerpt = stderr_excerpt_of(stderr);
                Err(format!("the CLI exited with code {code}.{excerpt}"))
            }
            None => Err("the CLI was terminated by a signal.".to_string()),
        }
    }
}

/// Resolves the adapter for a registry id.
pub fn find_adapter(id: &str) -> Option<&'static dyn ExecutionAdapter> {
    const CODEX: CodexAdapter = CodexAdapter;
    const CLAUDE: ClaudeCodeAdapter = ClaudeCodeAdapter;
    match id {
        "codex" => Some(&CODEX),
        "claude-code" => Some(&CLAUDE),
        _ => None,
    }
}

/// Extracts a short stderr excerpt for error messages.
fn stderr_excerpt_of(stderr: &str) -> String {
    stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>()
}

/// The fixed diagnostic prompt (exposed for documentation/tests).
pub fn diagnostic_prompt() -> &'static str {
    DIAGNOSTIC_PROMPT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_work_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("orqen-adapter-test-workdir")
    }

    #[test]
    fn adapters_match_registry_ids() {
        assert_eq!(CodexAdapter.id(), "codex");
        assert_eq!(ClaudeCodeAdapter.id(), "claude-code");
        assert!(find_adapter("codex").is_some());
        assert!(find_adapter("claude-code").is_some());
        assert!(find_adapter("ghost").is_none());
    }

    #[test]
    fn codex_invocation_loads_no_user_config_and_applies_the_confinement_profile() {
        let args = CodexAdapter.build_arguments(&any_work_dir());
        assert_eq!(args[0], "exec", "codex exec is the noninteractive mode");
        // Nothing inherited: no MCP servers, hooks, plugins, profiles.
        assert!(
            args.iter().any(|a| a == "--ignore-user-config"),
            "the user's global configuration must not be loaded: {args:?}"
        );
        assert!(args.iter().any(|a| a == "--ignore-rules"));
        assert!(args.iter().any(|a| a == "--ephemeral"));
        assert!(args.iter().any(|a| a == "--skip-git-repo-check"));

        // FALLBACK GUARD: mixing --sandbox/sandbox_mode with a permissions
        // profile makes Codex silently use the legacy write-only sandbox,
        // defeating read confinement. The flag must never be present.
        assert!(
            !args.iter().any(|a| a == "--sandbox" || a == "-s"),
            "the legacy sandbox flag must not be passed: {args:?}"
        );
        assert!(
            !args.iter().any(|a| a.starts_with("sandbox_mode=")),
            "sandbox_mode must not be set: {args:?}"
        );

        // The profile: read-deny-by-default with only the documented
        // runtime token and the work dir granted.
        let profile_def = args
            .iter()
            .find(|a| a.starts_with("permissions.orqen-diag-"))
            .expect("the app-owned profile must be defined via -c");
        assert!(
            profile_def.contains("\":root\"=\"deny\""),
            "reads must be denied by default: {profile_def}"
        );
        assert!(
            profile_def.contains("\":minimal\"=\"read\""),
            "the documented runtime token must be granted: {profile_def}"
        );
        assert!(
            profile_def.contains("\"/tmp\"=\"deny\""),
            "/tmp leaks through :minimal and must be re-denied: {profile_def}"
        );
        assert!(
            profile_def.contains("=\"write\""),
            "the work dir must be writable: {profile_def}"
        );
        // The work dir grant quotes the path (spaces survive as one -c
        // value; TOML basic-string escaping applies).
        let work_dir_quoted = toml_quote(&any_work_dir().to_string_lossy());
        assert!(
            profile_def.contains(&format!("{work_dir_quoted}=\"write\"")),
            "the grant must bind to THIS run's work dir: {profile_def}"
        );
        // Selection: the same profile is selected as the default.
        let selection = args
            .iter()
            .find(|a| a.starts_with("default_permissions="))
            .expect("the profile must be selected");
        let name = profile_def
            .trim_start_matches("permissions.")
            .split('=')
            .next()
            .unwrap_or("");
        assert_eq!(
            selection.as_str(),
            format!("default_permissions={}", toml_quote(name)),
            "the selected profile must be the defined one"
        );

        // The environment passed to tool-call children is restricted.
        assert!(
            args.iter()
                .any(|a| a == "shell_environment_policy.inherit=\"core\""),
            "tool-call env must be the documented core set: {args:?}"
        );
        assert!(
            args.iter()
                .any(|a| a.starts_with("shell_environment_policy.exclude=")),
            "credential-shaped env names must be excluded: {args:?}"
        );

        // The prompt is the final argument.
        assert_eq!(args.last().expect("prompt"), DIAGNOSTIC_PROMPT);
    }

    #[test]
    fn the_confinement_profile_name_is_random_per_run() {
        let first = codex_diagnostic_run_policy(&any_work_dir()).profile_name;
        let second = codex_diagnostic_run_policy(&any_work_dir()).profile_name;
        assert_ne!(
            first, second,
            "a per-run random name makes collisions with the user's global configuration impossible"
        );
        assert!(first.starts_with("orqen-diag-"));
    }

    #[test]
    fn toml_quote_escapes_special_characters() {
        assert_eq!(toml_quote("/tmp/a"), "\"/tmp/a\"");
        assert_eq!(toml_quote("/a \"b\""), "\"/a \\\"b\\\"\"");
        assert_eq!(toml_quote("back\\slash"), "\"back\\\\slash\"");
        // A path with spaces stays a single quoted TOML string.
        assert_eq!(
            toml_quote("/Users/x/Application Support/dir"),
            "\"/Users/x/Application Support/dir\""
        );
    }

    #[test]
    fn version_gate_fails_closed() {
        assert!(version_meets_minimum(
            "codex-cli 0.164.0",
            CODEX_MINIMUM_VERSION
        ));
        assert!(version_meets_minimum(
            "codex-cli 0.165.1",
            CODEX_MINIMUM_VERSION
        ));
        assert!(version_meets_minimum(
            "codex-cli 1.0.0",
            CODEX_MINIMUM_VERSION
        ));
        assert!(
            !version_meets_minimum("codex-cli 0.163.9", CODEX_MINIMUM_VERSION),
            "versions below the verified minimum must fail closed"
        );
        assert!(
            !version_meets_minimum("", CODEX_MINIMUM_VERSION),
            "unparseable output must fail closed"
        );
        assert!(!version_meets_minimum("unknown", CODEX_MINIMUM_VERSION));
        assert!(CodexAdapter.minimum_cli_version().is_some());
        // Claude stays disabled and has no version gate to bypass into.
        assert!(ClaudeCodeAdapter.minimum_cli_version().is_none());
    }

    #[test]
    fn adapter_posture_matches_verified_platforms() {
        // Codex is enabled ONLY where the confinement profile was
        // independently verified (macOS Seatbelt, D-023); everywhere else
        // it must report disabled with an accurate reason.
        assert_eq!(
            CodexAdapter.supports_safe_diagnostics(),
            cfg!(target_os = "macos")
        );
        if CodexAdapter.supports_safe_diagnostics() {
            assert!(CodexAdapter.unsafe_diagnostics_reason().is_empty());
        } else {
            assert!(
                CodexAdapter
                    .unsafe_diagnostics_reason()
                    .contains("disabled")
            );
        }
        // Claude Code remains disabled on every platform until
        // independently reviewed.
        assert!(!ClaudeCodeAdapter.supports_safe_diagnostics());
        assert!(
            ClaudeCodeAdapter
                .unsafe_diagnostics_reason()
                .contains("disabled")
        );
        assert!(ClaudeCodeAdapter.restriction_summary().contains("disabled"));
    }

    #[test]
    fn claude_uses_documented_print_mode_with_denied_mutating_tools() {
        let args = ClaudeCodeAdapter.build_arguments(&any_work_dir());
        assert_eq!(args[0], "-p", "print mode is the noninteractive mode");
        assert!(args.contains(&DIAGNOSTIC_PROMPT.to_string()));
        let deny_position = args
            .iter()
            .position(|a| a == "--disallowedTools")
            .expect("mutating tools must be denied");
        assert!(
            args[deny_position + 1]
                .split(',')
                .all(|tool| matches!(tool, "Bash" | "Edit" | "Write" | "NotebookEdit")),
            "only known mutating tools may be denied: {:?}",
            args
        );
    }

    #[test]
    fn claude_parse_classifies_sdk_errors_as_failures() {
        // Real-CLI evidence: exit 0 with an SDK error is a FAILURE.
        let error = ClaudeCodeAdapter
            .parse_result(
                Some(0),
                "{\"type\":\"result\",\"is_error\":false,\"result\":\"\"}\n[claude-code:unrecognized_model] {\"model\":\"auto\"}\n",
                "",
            )
            .expect_err("SDK error must not classify as success");
        assert!(error.contains("provider error"), "got: {error}");
    }

    #[test]
    fn claude_parse_extracts_json_results() {
        let parsed = ClaudeCodeAdapter
            .parse_result(
                Some(0),
                "{\"type\":\"result\",\"is_error\":false,\"result\":\"Agent available.\"}\n",
                "",
            )
            .expect("valid json result");
        assert_eq!(parsed, "Agent available.");
    }

    #[test]
    fn claude_parse_rejects_error_results_and_malformed_json() {
        assert!(
            ClaudeCodeAdapter
                .parse_result(
                    Some(0),
                    "{\"is_error\":true,\"result\":\"permission denied\"}",
                    ""
                )
                .is_err()
        );
        assert!(
            ClaudeCodeAdapter
                .parse_result(Some(0), "not json", "")
                .is_err()
        );
        assert!(ClaudeCodeAdapter.parse_result(Some(0), "", "").is_err());
        assert!(ClaudeCodeAdapter.parse_result(Some(2), "", "boom").is_err());
    }

    #[test]
    fn codex_parse_uses_the_documented_exit_code_contract() {
        assert_eq!(
            CodexAdapter
                .parse_result(Some(0), "codex 1.0 ready", "")
                .expect("ok"),
            "codex 1.0 ready"
        );
        assert!(CodexAdapter.parse_result(Some(0), "   ", "").is_err());
        assert!(CodexAdapter.parse_result(Some(3), "", "boom").is_err());
        assert!(CodexAdapter.parse_result(None, "", "").is_err());
    }

    #[test]
    fn prompt_is_fixed_and_does_not_request_file_operations() {
        let prompt = diagnostic_prompt();
        assert!(prompt.contains("Do not read, modify, create, or delete"));
        // The prompt must be identical for every adapter (no per-provider
        // prompt drift in the diagnostic).
        let codex_args = CodexAdapter.build_arguments(&any_work_dir());
        let claude_args = ClaudeCodeAdapter.build_arguments(&any_work_dir());
        assert_eq!(codex_args.last().expect("codex prompt"), prompt);
        assert!(claude_args.contains(&prompt.to_string()));
    }
}

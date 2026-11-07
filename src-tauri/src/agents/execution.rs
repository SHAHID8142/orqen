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
//! - Each adapter documents its provider's least-privilege settings;
//!   an adapter that cannot establish adequate restrictions must report
//!   `supports_safe_diagnostics() == false` and stay disabled.
//! - Authentication always comes from the CLI's own configuration; no
//!   credentials are read, stored, or logged.

/// The fixed diagnostic prompt. Deliberately not configurable: this
/// feature validates the execution architecture, not the model.
pub const DIAGNOSTIC_PROMPT: &str = "Respond with a short confirmation that \
the agent is available. Do not read, modify, create, or delete any local \
project files.";

/// Adapter contract: how one supported CLI is invoked noninteractively.
///
/// Implementations own their provider-specific argument construction and
/// are unit-tested. The execution runner stays provider-agnostic.
pub trait ExecutionAdapter: Send + Sync {
    /// Stable id — matches the registry definition id.
    fn id(&self) -> &'static str;
    /// The noninteractive argument list AFTER the executable. Built from
    /// compile-time constants and the fixed diagnostic prompt only.
    fn build_arguments(&self) -> Vec<String>;
    /// Whether this adapter's invocation establishes adequate
    /// least-privilege restrictions for a diagnostic run. Adapters that
    /// cannot must report false and stay disabled.
    fn supports_safe_diagnostics(&self) -> bool;
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

/// OpenAI Codex CLI: `codex exec` is the documented noninteractive mode;
/// `--sandbox read-only` is the documented least-privilege setting;
/// `--skip-git-repo-check` is required because the isolated working
/// directory is not a git repository.
pub struct CodexAdapter;

impl ExecutionAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn build_arguments(&self) -> Vec<String> {
        vec![
            "exec".to_string(),
            "--sandbox".to_string(),
            "read-only".to_string(),
            "--skip-git-repo-check".to_string(),
            DIAGNOSTIC_PROMPT.to_string(),
        ]
    }

    fn supports_safe_diagnostics(&self) -> bool {
        true
    }

    fn restriction_summary(&self) -> &'static str {
        "Runs noninteractively with the documented read-only sandbox."
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

    fn build_arguments(&self) -> Vec<String> {
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

    #[test]
    fn adapters_match_registry_ids() {
        assert_eq!(CodexAdapter.id(), "codex");
        assert_eq!(ClaudeCodeAdapter.id(), "claude-code");
        assert!(find_adapter("codex").is_some());
        assert!(find_adapter("claude-code").is_some());
        assert!(find_adapter("ghost").is_none());
    }

    #[test]
    fn codex_uses_documented_noninteractive_invocation() {
        let args = CodexAdapter.build_arguments();
        assert_eq!(args[0], "exec", "codex exec is the noninteractive mode");
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--sandbox" && w[1] == "read-only"),
            "the documented read-only sandbox must be applied: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--skip-git-repo-check"),
            "the isolated working directory is not a git repository"
        );
        assert!(args.last().expect("prompt") == super::super::execution::DIAGNOSTIC_PROMPT);
    }

    #[test]
    fn claude_uses_documented_print_mode_with_denied_mutating_tools() {
        let args = ClaudeCodeAdapter.build_arguments();
        assert_eq!(args[0], "-p", "print mode is the noninteractive mode");
        assert!(args.contains(&super::super::execution::DIAGNOSTIC_PROMPT.to_string()));
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
    fn codex_supports_diagnostics_claude_is_disabled() {
        assert!(CodexAdapter.supports_safe_diagnostics());
        // Claude Code diagnostics stay disabled: confidentiality and tool
        // isolation cannot be verified across CLI versions (see D-019-era
        // docs and QA_REPORT for the real-CLI evidence).
        assert!(!ClaudeCodeAdapter.supports_safe_diagnostics());
        assert!(!ClaudeCodeAdapter.unsafe_diagnostics_reason().is_empty());
        assert!(ClaudeCodeAdapter.restriction_summary().contains("disabled"));
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
        let codex_args = CodexAdapter.build_arguments();
        let claude_args = ClaudeCodeAdapter.build_arguments();
        assert_eq!(codex_args.last().expect("codex prompt"), prompt);
        assert!(claude_args.contains(&prompt.to_string()));
    }
}

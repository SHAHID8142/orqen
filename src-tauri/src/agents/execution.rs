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

use serde::Serialize;

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
}

/// Anthropic Claude Code CLI: `claude -p` is the documented print mode
/// (noninteractive). In print mode, permission prompts cannot be answered
/// and are denied automatically, so file-modifying tool use is denied by
/// default; the mutating tools are additionally denied explicitly as
/// defense in depth. Tool names verified against Claude Code 2.1.316
/// (`MultiEdit` no longer exists there — it was merged into `Edit`).
pub struct ClaudeCodeAdapter;

impl ExecutionAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn build_arguments(&self) -> Vec<String> {
        vec![
            "-p".to_string(),
            DIAGNOSTIC_PROMPT.to_string(),
            "--disallowedTools".to_string(),
            "Bash,Edit,Write,NotebookEdit".to_string(),
        ]
    }

    fn supports_safe_diagnostics(&self) -> bool {
        true
    }

    fn restriction_summary(&self) -> &'static str {
        "Runs noninteractively (print mode); file-modifying tools are denied \
         and permission prompts are auto-denied."
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
            args.windows(2).any(|w| w[0] == "--sandbox" && w[1] == "read-only"),
            "the documented read-only sandbox must be applied: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--skip-git-repo-check"),
            "the isolated working directory is not a git repository"
        );
        assert!(
            args.last().expect("prompt") == super::super::execution::DIAGNOSTIC_PROMPT
        );
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
    fn both_adapters_support_safe_diagnostics() {
        assert!(CodexAdapter.supports_safe_diagnostics());
        assert!(ClaudeCodeAdapter.supports_safe_diagnostics());
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

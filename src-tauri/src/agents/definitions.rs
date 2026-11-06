//! Static agent definitions.
//!
//! This module is the single source of truth for supported agents. UI,
//! commands, and persistence validate against it — no agent-specific
//! logic may appear elsewhere.

use serde::Serialize;

/// What a supported agent can currently do inside Orqen versus what is
/// planned. `execution` is `true` for the Phase 3 diagnostic test run
/// (a fixed, noninteractive prompt in an isolated directory — not
/// autonomous coding); `orchestration` stays `false` until a future
/// feature implements it. The UI relies on this distinction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub discovery: bool,
    pub verification: bool,
    pub execution: bool,
    pub orchestration: bool,
}

/// A supported agent definition.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    /// Stable identifier used by preferences and IPC. Never changes.
    pub id: &'static str,
    pub display_name: &'static str,
    pub provider: &'static str,
    /// Adapter kind — only CLI adapters exist today.
    pub adapter: &'static str,
    /// Executable name used for PATH discovery (without extension; the
    /// platform layer appends Windows extensions when searching).
    pub executable_name: &'static str,
    pub capabilities: AgentCapabilities,
}

/// OpenAI Codex CLI (`codex`, documented noninteractive `--version`).
pub const CODEX: AgentDefinition = AgentDefinition {
    id: "codex",
    display_name: "OpenAI Codex CLI",
    provider: "OpenAI",
    adapter: "cli",
    executable_name: "codex",
    capabilities: AgentCapabilities {
        discovery: true,
        verification: true,
        execution: true,
        orchestration: false,
    },
};

/// Anthropic Claude Code CLI (`claude`, documented noninteractive `--version`).
pub const CLAUDE_CODE: AgentDefinition = AgentDefinition {
    id: "claude-code",
    display_name: "Claude Code",
    provider: "Anthropic",
    adapter: "cli",
    executable_name: "claude",
    capabilities: AgentCapabilities {
        discovery: true,
        verification: true,
        execution: true,
        orchestration: false,
    },
};

/// All agents supported by this build, in display order.
pub const AGENT_DEFINITIONS: &[AgentDefinition] = &[CODEX, CLAUDE_CODE];

/// Looks up a definition by stable id. IPC inputs validate against this.
pub fn find_definition(id: &str) -> Option<&'static AgentDefinition> {
    AGENT_DEFINITIONS.iter().find(|d| d.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_the_two_initial_agents() {
        assert_eq!(AGENT_DEFINITIONS.len(), 2);
        assert_eq!(AGENT_DEFINITIONS[0].id, "codex");
        assert_eq!(AGENT_DEFINITIONS[0].display_name, "OpenAI Codex CLI");
        assert_eq!(AGENT_DEFINITIONS[0].provider, "OpenAI");
        assert_eq!(AGENT_DEFINITIONS[1].id, "claude-code");
        assert_eq!(AGENT_DEFINITIONS[1].display_name, "Claude Code");
        assert_eq!(AGENT_DEFINITIONS[1].provider, "Anthropic");
    }

    #[test]
    fn all_definitions_are_cli_adapters_with_diagnostic_execution() {
        for definition in AGENT_DEFINITIONS {
            assert_eq!(definition.adapter, "cli");
            assert!(definition.capabilities.discovery);
            assert!(definition.capabilities.verification);
            // Phase 3 Feature 002: diagnostic execution is implemented
            // (a fixed noninteractive prompt in an isolated directory).
            assert!(definition.capabilities.execution);
            assert!(
                !definition.capabilities.orchestration,
                "orchestration must stay false until a future feature implements it"
            );
            assert!(!definition.executable_name.is_empty());
        }
    }

    #[test]
    fn ids_are_stable_and_lookup_rejects_unknown_ids() {
        for definition in AGENT_DEFINITIONS {
            assert_eq!(
                find_definition(definition.id).expect("registered id").id,
                definition.id
            );
        }
        assert!(find_definition("unknown-agent").is_none());
        assert!(find_definition("").is_none());
        // Ids must be safe as single path/URL segments.
        for definition in AGENT_DEFINITIONS {
            assert!(
                definition
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "agent id must be a safe identifier: {}",
                definition.id
            );
        }
    }
}

//! Agent registry IPC commands.
//!
//! `list_agents` returns the static registry (nothing has been probed
//! yet); `discover_agent` locates an executable (never runs it);
//! `verify_agent` is the explicit, user-initiated version probe. Agent
//! ids are validated against the built-in registry — an unknown id can
//! never reach the process layer.

use crate::agents::definitions::{AGENT_DEFINITIONS, find_definition};
use crate::agents::{discovery, verify};
use crate::error::{CommandError, CommandResult};
use serde::Serialize;

/// Capability metadata mirrored to the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilitiesDto {
    pub discovery: bool,
    pub verification: bool,
    pub execution: bool,
    pub orchestration: bool,
}

/// Discovery state of one agent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum DiscoveryDto {
    /// Nothing has been probed yet (fresh registry load).
    NotChecked,
    /// An executable was located (existence only — not verified).
    Found { path: String },
    /// No executable found on the search path.
    NotFound,
    /// Discovery itself failed (e.g. PATH unset).
    Error { message: String },
}

/// Verification state of one agent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum VerificationDto {
    /// The `--version` probe succeeded.
    Verified { version: String },
    /// The probe ran and failed; message is user-presentable.
    Failed { message: String },
}

/// Full registry record for one agent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusDto {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub adapter: String,
    pub executable_name: String,
    pub capabilities: AgentCapabilitiesDto,
    pub discovery: DiscoveryDto,
    pub verification: Option<VerificationDto>,
    /// When the last discovery/verification probe ran, if any.
    pub checked_at: Option<String>,
}

fn definition_to_status(
    definition: &crate::agents::definitions::AgentDefinition,
) -> AgentStatusDto {
    AgentStatusDto {
        id: definition.id.to_string(),
        display_name: definition.display_name.to_string(),
        provider: definition.provider.to_string(),
        adapter: definition.adapter.to_string(),
        executable_name: definition.executable_name.to_string(),
        capabilities: AgentCapabilitiesDto {
            discovery: definition.capabilities.discovery,
            verification: definition.capabilities.verification,
            execution: definition.capabilities.execution,
            orchestration: definition.capabilities.orchestration,
        },
        discovery: DiscoveryDto::NotChecked,
        verification: None,
        checked_at: None,
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn definition_for_id(
    id: &str,
) -> Result<&'static crate::agents::definitions::AgentDefinition, CommandError> {
    find_definition(id).ok_or_else(|| {
        CommandError::InvalidInput(format!(
            "\"{id}\" is not a supported agent. Supported agents: {}.",
            AGENT_DEFINITIONS
                .iter()
                .map(|d| d.id)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// Returns the static registry. Discovery state is `not_checked` —
/// discovery and verification are explicit user actions.
#[tauri::command]
pub fn list_agents() -> CommandResult<Vec<AgentStatusDto>> {
    Ok(AGENT_DEFINITIONS.iter().map(definition_to_status).collect())
}

/// Locates the agent's executable on the search path. Never executes it.
#[tauri::command]
pub fn discover_agent(id: String) -> CommandResult<AgentStatusDto> {
    let definition = definition_for_id(&id)?;
    let mut status = definition_to_status(definition);
    status.checked_at = Some(now_rfc3339());
    status.discovery = match discovery::discover(definition) {
        discovery::DiscoveryOutcome::Found { path } => DiscoveryDto::Found {
            path: path.to_string_lossy().to_string(),
        },
        discovery::DiscoveryOutcome::NotFound => DiscoveryDto::NotFound,
        discovery::DiscoveryOutcome::Error { message } => DiscoveryDto::Error { message },
    };
    Ok(status)
}

/// Explicit, user-initiated CLI verification: resolves the executable
/// via discovery, then runs the fixed `--version` probe. Discovery
/// status is reported alongside so the UI can show both truthfully.
#[tauri::command]
pub fn verify_agent(id: String) -> CommandResult<AgentStatusDto> {
    let definition = definition_for_id(&id)?;
    let mut status = definition_to_status(definition);
    status.checked_at = Some(now_rfc3339());

    let path = match discovery::discover(definition) {
        discovery::DiscoveryOutcome::Found { path } => {
            status.discovery = DiscoveryDto::Found {
                path: path.to_string_lossy().to_string(),
            };
            path
        }
        discovery::DiscoveryOutcome::NotFound => {
            status.discovery = DiscoveryDto::NotFound;
            return Ok(status);
        }
        discovery::DiscoveryOutcome::Error { message } => {
            status.discovery = DiscoveryDto::Error { message };
            return Ok(status);
        }
    };

    status.verification = Some(match verify::verify_cli(&path, verify::DEFAULT_TIMEOUT) {
        Ok(verified) => VerificationDto::Verified {
            version: verified.version,
        },
        Err(error) => {
            eprintln!("orqen agents: verification of {id} failed: {error:?}");
            VerificationDto::Failed {
                message: error.user_message(),
            }
        }
    });
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_returns_the_static_registry_not_checked() {
        let agents = list_agents().expect("list");
        assert_eq!(agents.len(), 2);
        assert!(matches!(agents[0].discovery, DiscoveryDto::NotChecked));
        assert!(agents[0].verification.is_none());
        assert!(!agents[0].capabilities.execution);
    }

    #[test]
    fn unknown_agent_ids_are_rejected() {
        let error = discover_agent("ghost".to_string()).expect_err("must reject");
        match error {
            CommandError::InvalidInput(message) => {
                assert!(message.contains("not a supported agent"), "got: {message}");
            }
            other => panic!("expected InvalidInput, got: {other:?}"),
        }
        assert!(verify_agent("ghost".to_string()).is_err());
    }
}

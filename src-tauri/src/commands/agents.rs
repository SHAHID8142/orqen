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
use tauri::Manager;

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

/// Starts a controlled diagnostic run for one agent. Returns the run id
/// immediately; poll `get_diagnostic_run` for live status. The run
/// executes a FIXED noninteractive prompt in an isolated directory and
/// never touches user project files.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_diagnostic_run(
    app: tauri::AppHandle,
    registry: tauri::State<'_, crate::agents::runner::RunRegistry>,
    agent_id: String,
) -> CommandResult<String> {
    let definition = definition_for_id(&agent_id)?;
    if !crate::agents::execution::find_adapter(definition.id)
        .map(|a| a.supports_safe_diagnostics())
        .unwrap_or(false)
    {
        return Err(CommandError::InvalidInput(format!(
            "\"{}\" cannot run a diagnostic safely with the documented \
             invocation of this CLI version; execution stays disabled.",
            definition.display_name
        )));
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| CommandError::Storage(format!("could not resolve the data directory: {e}")))?;
    registry
        .start(definition, &data_dir)
        .map_err(CommandError::Storage)
}

/// Live status of a diagnostic run (polled by the frontend).
#[tauri::command]
pub async fn get_diagnostic_run(
    registry: tauri::State<'_, crate::agents::runner::RunRegistry>,
    run_id: String,
) -> CommandResult<crate::agents::runner::RunStatusDto> {
    Ok(registry.status(&run_id).unwrap_or(
        crate::agents::runner::RunStatusDto::Unknown,
    ))
}

/// Cancels a running diagnostic: signals the process group (unix) and
/// flags the worker, which terminates the probe tree.
#[tauri::command]
pub async fn cancel_diagnostic_run(
    registry: tauri::State<'_, crate::agents::runner::RunRegistry>,
    run_id: String,
) -> CommandResult<()> {
    registry
        .cancel(&run_id)
        .map_err(CommandError::Storage)
}

/// Locates the agent's executable on the search path. Never executes it.
///
/// Async: the filesystem walk runs on a dedicated blocking worker so the
/// Tauri main thread and the async executor stay responsive.
#[tauri::command]
pub async fn discover_agent(id: String) -> CommandResult<AgentStatusDto> {
    let definition = definition_for_id(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| CommandError::Storage(format!("discovery task failed: {e}")))?
}

/// Explicit, user-initiated CLI verification: resolves the executable
/// via discovery, then runs the fixed `--version` probe. Discovery
/// status is reported alongside so the UI can show both truthfully.
///
/// Async: process spawning, polling, and pipe draining run on a dedicated
/// blocking worker — the Tauri main thread is never blocked by a slow or
/// hung probe, and the database is untouched during verification.
#[tauri::command]
pub async fn verify_agent(id: String) -> CommandResult<AgentStatusDto> {
    let definition = definition_for_id(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let id = definition.id;
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
    })
    .await
    .map_err(|e| CommandError::Storage(format!("verification task failed: {e}")))?
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
        // Phase 3 Feature 002: diagnostic execution is now implemented.
        assert!(agents[0].capabilities.execution);
        assert!(!agents[0].capabilities.orchestration);
    }

    #[test]
    fn unknown_agent_ids_are_rejected() {
        // Validation happens synchronously before any spawn_blocking work.
        let error = tauri::async_runtime::block_on(discover_agent("ghost".to_string()))
            .expect_err("must reject");
        match error {
            CommandError::InvalidInput(message) => {
                assert!(message.contains("not a supported agent"), "got: {message}");
            }
            other => panic!("expected InvalidInput, got: {other:?}"),
        }
        assert!(tauri::async_runtime::block_on(verify_agent("ghost".to_string())).is_err());
    }
}

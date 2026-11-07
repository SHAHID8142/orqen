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
    /// Whether THIS build can execute a safe diagnostic for the agent —
    /// the adapter's own eligibility, not just the capability category.
    pub diagnostic_execution: bool,
    /// Accurate, user-facing explanation of the adapter's diagnostic
    /// posture (why it is disabled, or what it enforces when enabled).
    pub diagnostic_execution_note: String,
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
            diagnostic_execution: crate::agents::execution::find_adapter(definition.id)
                .map(|adapter| adapter.supports_safe_diagnostics())
                .unwrap_or(false),
            diagnostic_execution_note: crate::agents::execution::find_adapter(definition.id)
                .map(|adapter| {
                    if adapter.supports_safe_diagnostics() {
                        adapter.restriction_summary().to_string()
                    } else {
                        adapter.unsafe_diagnostics_reason().to_string()
                    }
                })
                .unwrap_or_else(|| "no execution adapter in this build".to_string()),
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

/// Backend-authoritative admission gate for a diagnostic run. The UI
/// confirmation authorizes intent; THIS check authorizes execution:
/// the adapter must permit safe diagnostics in this build, a fresh
/// backend-side verification must exist (the ledger decides — frontend
/// "Verified" status is never trusted), and the verified CLI version
/// must support the enforced confinement policy (unsupported or
/// unparseable versions fail closed). Returns the verified executable
/// identity the runner will re-check before spawning.
pub fn ensure_diagnostic_admissible(
    definition: &'static crate::agents::definitions::AgentDefinition,
    ledger: &crate::agents::eligibility::EligibilityLedger,
) -> Result<crate::agents::eligibility::ExecutableIdentity, String> {
    let adapter = crate::agents::execution::find_adapter(definition.id).ok_or_else(|| {
        format!(
            "\"{}\" has no execution adapter in this build.",
            definition.id
        )
    })?;
    if !adapter.supports_safe_diagnostics() {
        return Err(adapter.unsafe_diagnostics_reason().to_string());
    }
    let identity = ledger.fresh_identity(
        definition.id,
        crate::agents::eligibility::VERIFICATION_FRESHNESS,
    )?;
    if let Some(minimum) = adapter.minimum_cli_version() {
        let version = ledger
            .verified_cli_version(definition.id)
            .unwrap_or_default();
        if !crate::agents::execution::version_meets_minimum(&version, minimum) {
            return Err(format!(
                "the installed \"{}\" CLI ({}) does not support the \
                 enforced per-run permissions profile — update the CLI \
                 and verify again.",
                definition.display_name,
                if version.is_empty() {
                    "version unknown"
                } else {
                    &version
                }
            ));
        }
    }
    Ok(identity)
}

/// Starts a controlled diagnostic run for one agent. Returns the run id
/// immediately; poll `get_diagnostic_run` for live status. The run
/// executes a FIXED noninteractive prompt in an isolated directory and
/// never touches user project files.
///
/// Eligibility is backend-authoritative and cannot be bypassed by a
/// direct IPC invocation: the adapter must permit diagnostics, the
/// agent must have been verified in this session within the bounded
/// freshness window and with a supported CLI version, the ACTUAL
/// installed executable must pass the provider-free sandbox enforcement
/// preflight (the confinement mechanism is documented beta — safety is
/// proven per run on the real binary, never inferred from the version),
/// and the runner re-checks the executable's identity before spawning.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_diagnostic_run(
    app: tauri::AppHandle,
    registry: tauri::State<'_, crate::agents::runner::RunRegistry>,
    ledger: tauri::State<'_, crate::agents::eligibility::EligibilityLedger>,
    agent_id: String,
) -> CommandResult<String> {
    let definition = definition_for_id(&agent_id)?;
    let verified_identity =
        ensure_diagnostic_admissible(definition, &ledger).map_err(CommandError::InvalidInput)?;
    // Enforcement preflight against the verified executable — before
    // admission, so a failed preflight never consumes the single-run
    // slot. The runner's identity re-check guarantees the spawned
    // executable IS the one preflighted here.
    let requires_preflight = crate::agents::execution::find_adapter(definition.id)
        .map(|adapter| adapter.requires_enforcement_preflight())
        .unwrap_or(false);
    if requires_preflight {
        let executable = verified_identity.path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::agents::execution::codex_enforcement_preflight(&executable)
        })
        .await
        .map_err(|e| CommandError::Storage(format!("the preflight task failed: {e}")))?
        .map_err(CommandError::Storage)?;
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| CommandError::Storage(format!("could not resolve the data directory: {e}")))?;
    registry
        .start(definition, &data_dir, &verified_identity)
        .map_err(CommandError::Storage)
}

/// Live status of a diagnostic run (polled by the frontend).
#[tauri::command]
pub async fn get_diagnostic_run(
    registry: tauri::State<'_, crate::agents::runner::RunRegistry>,
    run_id: String,
) -> CommandResult<crate::agents::runner::RunStatusDto> {
    Ok(registry
        .status(&run_id)
        .unwrap_or(crate::agents::runner::RunStatusDto::Unknown))
}

/// Cancels a running diagnostic: signals the process group (unix) and
/// flags the worker, which terminates the probe tree.
#[tauri::command]
pub async fn cancel_diagnostic_run(
    registry: tauri::State<'_, crate::agents::runner::RunRegistry>,
    run_id: String,
) -> CommandResult<()> {
    registry.cancel(&run_id).map_err(CommandError::Storage)
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
/// A successful verification is recorded backend-side (the eligibility
/// ledger binds it to the executable's observed identity); failed or
/// missing verifications clear any prior grant. `--version` output is
/// never treated as proof of trustworthiness — it only fixes WHAT was
/// verified and WHEN, so later changes are detectable.
///
/// Async: process spawning, polling, and pipe draining run on a dedicated
/// blocking worker — the Tauri main thread is never blocked by a slow or
/// hung probe, and the database is untouched during verification.
#[tauri::command]
pub async fn verify_agent(
    ledger: tauri::State<'_, crate::agents::eligibility::EligibilityLedger>,
    id: String,
) -> CommandResult<AgentStatusDto> {
    let definition = definition_for_id(&id)?;
    let ledger = ledger.inner().clone();
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
                ledger.clear(id);
                return Ok(status);
            }
            discovery::DiscoveryOutcome::Error { message } => {
                status.discovery = DiscoveryDto::Error { message };
                ledger.clear(id);
                return Ok(status);
            }
        };

        status.verification = Some(match verify::verify_cli(&path, verify::DEFAULT_TIMEOUT) {
            Ok(verified) => {
                if !ledger.record_verified(id, &path, Some(&verified.version)) {
                    eprintln!(
                        "orqen agents: could not observe the {id} executable for the eligibility ledger"
                    );
                }
                VerificationDto::Verified {
                    version: verified.version,
                }
            }
            Err(error) => {
                eprintln!("orqen agents: verification of {id} failed: {error:?}");
                ledger.clear(id);
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
    use crate::agents::eligibility::EligibilityLedger;

    #[test]
    fn list_returns_the_static_registry_not_checked() {
        let agents = list_agents().expect("list");
        assert_eq!(agents.len(), 2);
        assert!(matches!(agents[0].discovery, DiscoveryDto::NotChecked));
        assert!(agents[0].verification.is_none());
        // Diagnostic posture is accurate per adapter and per platform
        // (D-023): Codex is enabled only where its confinement profile is
        // verified (macOS); Claude Code stays disabled everywhere. Every
        // adapter surfaces a non-empty user-facing note.
        assert!(agents[0].capabilities.execution);
        assert_eq!(
            agents[0].capabilities.diagnostic_execution,
            cfg!(target_os = "macos")
        );
        assert!(
            !agents[0].capabilities.diagnostic_execution_note.is_empty(),
            "every adapter must surface its posture"
        );
        let claude = agents
            .iter()
            .find(|a| a.id == "claude-code")
            .expect("claude-code entry");
        assert!(
            !claude.capabilities.diagnostic_execution,
            "claude-code diagnostics must report disabled eligibility"
        );
        assert!(!claude.capabilities.diagnostic_execution_note.is_empty());
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
        // Both execution commands share this gate: an unknown id can
        // never reach the process layer.
        assert!(definition_for_id("ghost").is_err());
    }

    #[test]
    fn the_admission_gate_rejects_without_backend_authorization() {
        // Direct-IPC bypass proof: even with a valid id, the admission
        // gate rejects without a fresh backend-side verification — the
        // frontend "Verified" badge alone authorizes nothing.
        let ledger = EligibilityLedger::default();
        let definition = find_definition("codex").expect("definition");
        let error = ensure_diagnostic_admissible(definition, &ledger).expect_err("must reject");
        assert!(
            error.contains("has not been verified"),
            "the ledger gate must fire for an unverified agent: {error}"
        );
        // Claude Code stays disabled on every platform: the posture gate
        // fires before the ledger is even consulted.
        let claude = find_definition("claude-code").expect("definition");
        let error = ensure_diagnostic_admissible(claude, &ledger).expect_err("must reject");
        assert!(error.contains("disabled"), "got: {error}");
    }

    #[test]
    fn verified_supported_codex_is_admissible() {
        // On the verified platform (macOS), a fresh verification of a
        // sufficiently new CLI authorizes a diagnostic.
        let ledger = EligibilityLedger::default();
        let dir = std::env::temp_dir().join(format!("orqen-adm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let tool = dir.join("codex-fixture");
        std::fs::write(&tool, b"binary").expect("fixture");
        assert!(ledger.record_verified("codex", &tool, Some("codex-cli 0.164.0")));
        let definition = find_definition("codex").expect("definition");
        let result = ensure_diagnostic_admissible(definition, &ledger);
        if cfg!(target_os = "macos") {
            let identity = result.expect("verified + supported must be admissible");
            assert_eq!(identity.path, tool);
        } else {
            let error = result.expect_err("non-macOS fails closed");
            assert!(error.contains("disabled"), "got: {error}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_cli_versions_fail_closed() {
        // A verification of an OLD CLI (or one with unparseable version
        // output) must never authorize a diagnostic for an adapter whose
        // confinement policy is version-gated.
        let dir = std::env::temp_dir().join(format!("orqen-adm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let tool = dir.join("codex-fixture");
        std::fs::write(&tool, b"binary").expect("fixture");
        for version in [Some("codex-cli 0.163.9"), Some("garbage output"), None] {
            let fresh = EligibilityLedger::default();
            assert!(fresh.record_verified("codex", &tool, version));
            let definition = find_definition("codex").expect("definition");
            let error = ensure_diagnostic_admissible(definition, &fresh)
                .expect_err("unsupported version must be rejected");
            if cfg!(target_os = "macos") {
                assert!(
                    error.contains("does not support the enforced per-run permissions profile"),
                    "version {version:?}: {error}"
                );
            } else {
                // Non-macOS fails closed at the posture gate first.
                assert!(error.contains("disabled"), "version {version:?}: {error}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

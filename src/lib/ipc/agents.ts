/**
 * Typed IPC bridge for the agent registry.
 */
import { invoke } from "@tauri-apps/api/core";
import type { AgentRecord, RunStatusDto } from "../../types/agents";

/** Loads the static registry (all agents in `not_checked` state). */
export async function fetchAgents(): Promise<AgentRecord[]> {
  return invoke<AgentRecord[]>("list_agents");
}

/** Locates the agent's executable on the search path. Never runs it. */
export async function discoverAgentRequest(id: string): Promise<AgentRecord> {
  return invoke<AgentRecord>("discover_agent", { id });
}

/** Explicit, user-initiated `--version` verification probe. */
export async function verifyAgentRequest(id: string): Promise<AgentRecord> {
  return invoke<AgentRecord>("verify_agent", { id });
}

/** Starts a controlled diagnostic run; resolves with the run id. */
export async function startDiagnosticRun(agentId: string): Promise<string> {
  return invoke<string>("start_diagnostic_run", { agentId });
}

/** Polls the live status of a diagnostic run. */
export async function getDiagnosticRun(runId: string): Promise<RunStatusDto> {
  return invoke<RunStatusDto>("get_diagnostic_run", { runId });
}

/** Requests cancellation of a running diagnostic. */
export async function cancelDiagnosticRun(runId: string): Promise<void> {
  return invoke<void>("cancel_diagnostic_run", { runId });
}

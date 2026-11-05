/**
 * Agent registry domain types, mirroring the Rust DTOs in
 * `src-tauri/src/commands/agents.rs`. Both sides change in the same commit.
 */

/** What a supported agent can do today versus planned capabilities. */
export interface AgentCapabilities {
  discovery: boolean;
  verification: boolean;
  execution: boolean;
  orchestration: boolean;
}

/**
 * Discovery state of one agent's executable. "Found" means a file was
 * located on the search path — never authentication, connectivity, or a
 * successful run.
 */
export type AgentDiscoveryState =
  | { status: "not_checked" }
  | { status: "found"; path: string }
  | { status: "not_found" }
  | { status: "error"; message: string };

/** Result of the explicit `--version` verification probe. */
export type AgentVerificationState =
  | { status: "verified"; version: string }
  | { status: "failed"; message: string };

/** Full registry record for one agent. */
export interface AgentRecord {
  id: string;
  displayName: string;
  provider: string;
  adapter: string;
  executableName: string;
  capabilities: AgentCapabilities;
  discovery: AgentDiscoveryState;
  verification: AgentVerificationState | null;
  /** When the last discovery/verification probe ran, if any. */
  checkedAt: string | null;
}

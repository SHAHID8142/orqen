/**
 * React binding for the agent registry store (same pattern as the
 * workspace and settings stores).
 */
import { createContext, useContext, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { createAgentsStore, type AgentsStore } from "./store";
import type { AgentsState } from "./store";
import {
  discoverAgentRequest,
  fetchAgents,
  getDiagnosticRun,
  startDiagnosticRun,
  cancelDiagnosticRun,
  verifyAgentRequest,
} from "../ipc/agents";

const AgentsContext = createContext<AgentsStore | null>(null);

/** Creates the store instance used for the application lifetime. */
export function createAppAgentsStore(): AgentsStore {
  return createAgentsStore({
    load: fetchAgents,
    discover: discoverAgentRequest,
    verify: verifyAgentRequest,
    startRun: startDiagnosticRun,
    runStatus: getDiagnosticRun,
    cancelRun: cancelDiagnosticRun,
  });
}

export function AgentsProvider({
  store,
  children,
}: {
  store: AgentsStore;
  children: ReactNode;
}) {
  return (
    <AgentsContext.Provider value={store}>{children}</AgentsContext.Provider>
  );
}

/** Accesses the agents state; updates re-render the caller. */
export function useAgents(): AgentsState {
  const store = useContext(AgentsContext);
  if (!store) {
    throw new Error("useAgents must be used within an AgentsProvider");
  }
  return useSyncExternalStore(store.subscribe, store.getState);
}

/** Accesses the agents store's actions without subscribing to state. */
export function useAgentsStore(): AgentsStore {
  const store = useContext(AgentsContext);
  if (!store) {
    throw new Error("useAgentsStore must be used within an AgentsProvider");
  }
  return store;
}

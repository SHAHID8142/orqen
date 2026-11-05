/**
 * React binding for the workspace store.
 *
 * Provides the store via context and a `useWorkspace()` hook backed by
 * `useSyncExternalStore`, so components re-render only on real changes.
 */
import { createContext, useContext, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { createWorkspaceStore, type WorkspaceStore } from "./store";
import type { WorkspaceState } from "./types";

const WorkspaceContext = createContext<WorkspaceStore | null>(null);

/** Creates the store instance used for the application lifetime. */
export function createAppWorkspaceStore(): WorkspaceStore {
  return createWorkspaceStore();
}

export function WorkspaceProvider({
  store,
  children,
}: {
  store: WorkspaceStore;
  children: ReactNode;
}) {
  return (
    <WorkspaceContext.Provider value={store}>{children}</WorkspaceContext.Provider>
  );
}

/** Accesses the workspace store; state updates re-render the caller. */
export function useWorkspace(): WorkspaceState {
  const store = useContext(WorkspaceContext);
  if (!store) {
    throw new Error("useWorkspace must be used within a WorkspaceProvider");
  }
  return useSyncExternalStore(store.subscribe, store.getState);
}

/** Accesses the workspace store's actions without subscribing to state. */
export function useWorkspaceStore(): WorkspaceStore {
  const store = useContext(WorkspaceContext);
  if (!store) {
    throw new Error("useWorkspaceStore must be used within a WorkspaceProvider");
  }
  return store;
}

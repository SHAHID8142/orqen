/**
 * React binding for the settings store (mirrors WorkspaceContext).
 */
import { createContext, useContext, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { createSettingsStore, type SettingsStore } from "./store";
import type { SettingsState } from "./types";

const SettingsContext = createContext<SettingsStore | null>(null);

/** Creates the store instance used for the application lifetime. */
export function createAppSettingsStore(): SettingsStore {
  return createSettingsStore();
}

export function SettingsProvider({
  store,
  children,
}: {
  store: SettingsStore;
  children: ReactNode;
}) {
  return (
    <SettingsContext.Provider value={store}>{children}</SettingsContext.Provider>
  );
}

/** Accesses the settings state; updates re-render the caller. */
export function useSettings(): SettingsState {
  const store = useContext(SettingsContext);
  if (!store) {
    throw new Error("useSettings must be used within a SettingsProvider");
  }
  return useSyncExternalStore(store.subscribe, store.getState);
}

/** Accesses the settings store's actions without subscribing to state. */
export function useSettingsStore(): SettingsStore {
  const store = useContext(SettingsContext);
  if (!store) {
    throw new Error("useSettingsStore must be used within a SettingsProvider");
  }
  return store;
}

import { useEffect, useState } from "react";
import { createAppSettingsStore, SettingsProvider } from "../lib/settings/SettingsContext";
import type { SettingsStore } from "../lib/settings/store";
import { createAppWorkspaceStore, WorkspaceProvider } from "../lib/workspace/WorkspaceContext";
import type { WorkspaceStore } from "../lib/workspace/store";
import Agents from "../features/agents/Agents";
import Overview from "../features/overview/Overview";
import Projects from "../features/projects/Projects";
import SettingsSurface from "../features/settings/SettingsSurface";
import Workflows from "../features/workflows/Workflows";
import AppShell from "./AppShell";
import type { ViewId } from "./views";

/**
 * Application root.
 *
 * Holds the active navigation view (state-based switching, D-013) and the
 * two persistence-backed stores. On mount it loads the persisted state
 * from the Rust core; failures surface in the UI with retry controls.
 * Stores are injectable for tests.
 */

const defaultWorkspaceStore = createAppWorkspaceStore();
const defaultSettingsStore = createAppSettingsStore();

function App({
  workspace = defaultWorkspaceStore,
  settings = defaultSettingsStore,
}: {
  workspace?: WorkspaceStore;
  settings?: SettingsStore;
} = {}) {
  const [activeView, setActiveView] = useState<ViewId>("overview");

  useEffect(() => {
    void workspace.load();
    void settings.load();
  }, [workspace, settings]);

  return (
    <WorkspaceProvider store={workspace}>
      <SettingsProvider store={settings}>
        <AppShell activeView={activeView} onNavigate={setActiveView}>
          {activeView === "overview" && <Overview />}
          {activeView === "projects" && <Projects />}
          {activeView === "agents" && <Agents />}
          {activeView === "workflows" && <Workflows />}
          {activeView === "settings" && <SettingsSurface />}
        </AppShell>
      </SettingsProvider>
    </WorkspaceProvider>
  );
}

export default App;

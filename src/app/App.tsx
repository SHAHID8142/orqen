import { useEffect, useState } from "react";
import { createAppAgentsStore, AgentsProvider } from "../lib/agents/AgentsContext";
import { createAppChatStore, ChatProvider } from "../lib/chat/ChatContext";
import { createAppSettingsStore, SettingsProvider } from "../lib/settings/SettingsContext";
import type { SettingsStore } from "../lib/settings/store";
import { createAppWorkspaceStore, WorkspaceProvider } from "../lib/workspace/WorkspaceContext";
import type { WorkspaceStore } from "../lib/workspace/store";
import Agents from "../features/agents/Agents";
import Overview from "../features/overview/Overview";
import Projects from "../features/projects/Projects";
import SettingsSurface from "../features/settings/SettingsSurface";
import Chat from "../features/chat/Chat";
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
const defaultAgentsStore = createAppAgentsStore();
const defaultChatStore = createAppChatStore();

function App({
  workspace = defaultWorkspaceStore,
  settings = defaultSettingsStore,
  agents = defaultAgentsStore,
  chat = defaultChatStore,
}: {
  workspace?: WorkspaceStore;
  settings?: SettingsStore;
  agents?: import("../lib/agents/store").AgentsStore;
  chat?: import("../lib/chat/store").ChatStore;
} = {}) {
  const [activeView, setActiveView] = useState<ViewId>("overview");

  useEffect(() => {
    void workspace.load();
    void settings.load();
    void agents.load();
  }, [workspace, settings, agents]);

  return (
    <WorkspaceProvider store={workspace}>
      <SettingsProvider store={settings}>
        <AgentsProvider store={agents}>
        <ChatProvider store={chat}>
        <AppShell activeView={activeView} onNavigate={setActiveView}>
          {activeView === "overview" && <Overview />}
          {activeView === "projects" && <Projects />}
          {activeView === "agents" && <Agents />}
          {activeView === "chat" && <Chat />}
          {activeView === "workflows" && <Workflows />}
          {activeView === "settings" && <SettingsSurface />}
        </AppShell>
        </ChatProvider>
        </AgentsProvider>
      </SettingsProvider>
    </WorkspaceProvider>
  );
}

export default App;

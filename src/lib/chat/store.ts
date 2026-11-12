/**
 * Chat store: mode-aware, per-project conversations on the observable/
 * serial-queue pattern (same as the workspace and settings stores).
 *
 * Conversations belong to the SELECTED project: the list reloads whenever
 * the selected project changes. Mutating operations return the fresh
 * backend detail so the store always converges on Rust state. IPC
 * functions are injectable for tests.
 */
import type { CommandErrorPayload } from "../../types/app";
import type {
  ChatMessage,
  ChatMode,
  ChatModeInfo,
  Conversation,
  ConversationDetail,
  ConversationExport,
} from "../../types/chat";
import { validateDraftMessage, validateDraftTitle } from "../../types/chat";
import { createSerialQueue, type SerialQueue } from "../serialQueue";

/** IPC surface of the chat store (matches `lib/ipc/chat.ts`). */
export interface ChatIpc {
  loadModes: () => Promise<ChatModeInfo[]>;
  loadConversations: (projectId: string) => Promise<Conversation[]>;
  open: (id: string) => Promise<ConversationDetail>;
  create: (projectId: string, title: string, mode: ChatMode) => Promise<Conversation>;
  rename: (id: string, title: string) => Promise<ConversationDetail>;
  setMode: (id: string, mode: ChatMode) => Promise<ConversationDetail>;
  remove: (id: string) => Promise<void>;
  append: (conversationId: string, content: string) => Promise<ConversationDetail>;
  export: (id: string) => Promise<ConversationExport>;
}

export interface ChatState {
  /** The chat-mode catalog from the backend. */
  modes: ChatModeInfo[];
  /** True while the catalog loads; false once loaded (also on error). */
  modesLoading: boolean;
  /** The conversations of the selected project. */
  conversations: Conversation[];
  /** Which conversation's messages are loaded, with the messages. */
  openConversationId: string | null;
  openMessages: ChatMessage[];
  /** The single chat operation currently in flight. */
  pending: null | "create" | "rename" | "mode" | "delete" | "append" | "export" | "open";
  /** The selected project the conversations belong to (null = none selected). */
  projectId: string | null;
  /** Initial per-project load state: idle → loading → ready/error. */
  listPhase: "idle" | "loading" | "ready" | "error";
  /** The Markdown export currently shown in the export panel (null = hidden). */
  export: ConversationExport | null;
  loadError: CommandErrorPayload | null;
  actionError: CommandErrorPayload | null;
}

export interface ChatStore {
  getState: () => ChatState;
  subscribe: (listener: () => void) => () => void;
  /** Loads the mode catalog (once; subsequent calls are cheap no-ops). */
  loadModes: () => Promise<void>;
  /** Reloads the conversation list for the selected project. */
  loadConversations: (projectId: string | null) => Promise<void>;
  openConversation: (id: string) => Promise<void>;
  createConversation: (title: string, mode: ChatMode) => Promise<boolean>;
  renameConversation: (id: string, title: string) => Promise<boolean>;
  setMode: (id: string, mode: ChatMode) => Promise<boolean>;
  deleteConversation: (id: string) => Promise<void>;
  /** Appends a user message; returns false when validation or IPC fails. */
  sendMessage: (content: string) => Promise<boolean>;
  /** Renders the transcript export and shows it in the export panel. */
  showExport: () => Promise<void>;
  /** Hides the export panel. */
  dismissExport: () => void;
  clearActionError: () => void;
}

export function createChatStore(ipc: ChatIpc): ChatStore {
  const queue: SerialQueue = createSerialQueue();
  let state: ChatState = {
    modes: [],
    modesLoading: true,
    conversations: [],
    openConversationId: null,
    openMessages: [],
    pending: null,
    projectId: null,
    listPhase: "idle",
    export: null,
    loadError: null,
    actionError: null,
  };
  const listeners = new Set<() => void>();

  function setState(next: Partial<ChatState>) {
    state = { ...state, ...next };
    for (const listener of listeners) listener();
  }

  function normalizeError(error: unknown): CommandErrorPayload {
    if (
      typeof error === "object" &&
      error !== null &&
      "kind" in error &&
      "message" in error &&
      typeof (error as { kind: unknown }).kind === "string" &&
      typeof (error as { message: unknown }).message === "string"
    ) {
      const payload = error as { kind: string; message: string };
      return { kind: payload.kind as CommandErrorPayload["kind"], message: payload.message };
    }
    return {
      kind: "internal",
      message: error instanceof Error ? error.message : String(error),
    };
  }

  async function loadModes(): Promise<void> {
    if (state.modes.length > 0 || !state.modesLoading) return;
    try {
      const modes = await ipc.loadModes();
      setState({ modes, modesLoading: false });
    } catch (error) {
      // The catalog is static; a failure leaves the modes unavailable and
      // surfaces through the surface's error card.
      setState({ modes: [], modesLoading: false, loadError: normalizeError(error) });
    }
  }

  async function loadConversations(projectId: string | null): Promise<void> {
    if (state.projectId === projectId && state.listPhase !== "error") return;
    setState({ projectId, listPhase: projectId ? "loading" : "idle", loadError: null });
    if (!projectId) {
      setState({ conversations: [], openConversationId: null, openMessages: [] });
      return;
    }
    return queue.run(async () => {
      try {
        const conversations = await ipc.loadConversations(projectId);
        const stillCurrent = state.projectId === projectId;
        if (stillCurrent) {
          const openId = state.openConversationId;
          const openStillExists = openId !== null && conversations.some((c) => c.id === openId);
          setState({
            conversations,
            listPhase: "ready",
            loadError: null,
            ...(openStillExists
              ? {}
              : { openConversationId: null, openMessages: [] }),
          });
        }
      } catch (error) {
        if (state.projectId === projectId) {
          setState({ listPhase: "error", loadError: normalizeError(error) });
        }
      }
    });
  }

  async function openConversation(id: string): Promise<void> {
    if (state.pending) return;
    setState({ pending: "open", actionError: null });
    try {
      const detail = await ipc.open(id);
      setState({
        openConversationId: detail.conversation.id,
        openMessages: detail.messages,
        pending: null,
      });
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
    }
  }

  function applyDetail(detail: ConversationDetail | Conversation) {
    // Converge: list entry + open state from one backend payload. A bare
    // Conversation (a fresh create) carries no messages yet.
    const detailView: ConversationDetail =
      "messages" in detail
        ? detail
        : { conversation: detail, messages: [] };
    const conversations = state.conversations
      .map((c) => (c.id === detailView.conversation.id ? detailView.conversation : c))
      .slice()
      .sort((a, b) => {
        if (b.updatedAt !== a.updatedAt) return b.updatedAt < a.updatedAt ? -1 : 1;
        return a.id < b.id ? -1 : 1;
      });
    const isNew = !conversations.some((c) => c.id === detailView.conversation.id);
    if (isNew) conversations.unshift(detailView.conversation);
    setState({
      conversations,
      openConversationId: detailView.conversation.id,
      openMessages: detailView.messages,
    });
  }

  async function createConversation(title: string, mode: ChatMode): Promise<boolean> {
    if (!state.projectId || state.pending) return false;
    if (validateDraftTitle(title)) {
      setState({
        actionError: {
          kind: "invalid_input",
          message: validateDraftTitle(title) as string,
        },
      });
      return false;
    }
    setState({ pending: "create", actionError: null });
    try {
      const created = await ipc.create(state.projectId, title.trim(), mode);
      applyDetail(created);
      setState({ pending: null });
      return true;
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
      return false;
    }
  }

  async function renameConversation(id: string, title: string): Promise<boolean> {
    if (state.pending) return false;
    if (validateDraftTitle(title)) {
      setState({
        actionError: {
          kind: "invalid_input",
          message: validateDraftTitle(title) as string,
        },
      });
      return false;
    }
    setState({ pending: "rename", actionError: null });
    try {
      const detail = await ipc.rename(id, title.trim());
      applyDetail(detail);
      setState({ pending: null });
      return true;
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
      return false;
    }
  }

  async function setMode(id: string, mode: ChatMode): Promise<boolean> {
    if (state.pending) return false;
    setState({ pending: "mode", actionError: null });
    try {
      const detail = await ipc.setMode(id, mode);
      applyDetail(detail);
      setState({ pending: null });
      return true;
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
      return false;
    }
  }

  async function deleteConversation(id: string): Promise<void> {
    if (state.pending) return;
    setState({ pending: "delete", actionError: null });
    try {
      await ipc.remove(id);
      const conversations = state.conversations.filter((c) => c.id !== id);
      const openCleared = state.openConversationId === id;
      setState({
        conversations,
        ...(openCleared ? { openConversationId: null, openMessages: [] } : {}),
        pending: null,
      });
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
    }
  }

  async function sendMessage(content: string): Promise<boolean> {
    const conversationId = state.openConversationId;
    if (!conversationId || state.pending) return false;
    const validationError = validateDraftMessage(content);
    if (validationError) {
      setState({
        actionError: { kind: "invalid_input", message: validationError },
      });
      return false;
    }
    setState({ pending: "append", actionError: null });
    try {
      const detail = await ipc.append(conversationId, content.trim());
      applyDetail(detail);
      setState({ pending: null });
      return true;
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
      return false;
    }
  }

  async function showExport(): Promise<void> {
    const id = state.openConversationId;
    if (!id || state.pending) return;
    setState({ pending: "export", actionError: null });
    try {
      const exportData = await ipc.export(id);
      setState({ export: exportData, pending: null });
    } catch (error) {
      setState({ pending: null, actionError: normalizeError(error) });
    }
  }

  function dismissExport(): void {
    setState({ export: null });
  }

  function clearActionError(): void {
    setState({ actionError: null });
  }

  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    loadModes,
    loadConversations,
    openConversation,
    createConversation,
    renameConversation,
    setMode,
    deleteConversation,
    sendMessage,
    showExport,
    dismissExport,
    clearActionError,
  };
}

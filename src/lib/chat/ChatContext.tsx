/**
 * React binding for the chat store (same pattern as the workspace,
 * settings, and agents contexts).
 */
import { createContext, useContext, useSyncExternalStore } from "react";
import type { ReactNode } from "react";
import { createChatStore, type ChatStore } from "./store";
import type { ChatState } from "./store";
import {
  fetchChatModes,
  fetchConversations,
  openConversationRequest,
  createConversationRequest,
  renameConversationRequest,
  setConversationModeRequest,
  deleteConversationRequest,
  appendMessageRequest,
  exportConversationRequest,
} from "../ipc/chat";

const ChatContext = createContext<ChatStore | null>(null);

/** Creates the store instance used for the application lifetime. */
export function createAppChatStore(): ChatStore {
  return createChatStore({
    loadModes: fetchChatModes,
    loadConversations: fetchConversations,
    open: openConversationRequest,
    create: createConversationRequest,
    rename: renameConversationRequest,
    setMode: setConversationModeRequest,
    remove: deleteConversationRequest,
    append: (conversationId, content) => appendMessageRequest(conversationId, "user", content),
    export: exportConversationRequest,
  });
}

export function ChatProvider({
  store,
  children,
}: {
  store: ChatStore;
  children: ReactNode;
}) {
  return (
    <ChatContext.Provider value={store}>{children}</ChatContext.Provider>
  );
}

/** Accesses the chat state; updates re-render the caller. */
export function useChat(): ChatState {
  const store = useContext(ChatContext);
  if (!store) {
    throw new Error("useChat must be used within a ChatProvider");
  }
  return useSyncExternalStore(store.subscribe, store.getState);
}

/** Accesses the chat store's actions without subscribing to state. */
export function useChatStore(): ChatStore {
  const store = useContext(ChatContext);
  if (!store) {
    throw new Error("useChatStore must be used within a ChatProvider");
  }
  return store;
}

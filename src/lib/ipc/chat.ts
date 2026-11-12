/**
 * Typed IPC bridge for persistent chat operations.
 *
 * Validation happens authoritatively in Rust; the store adds client-side
 * validation for instant feedback only.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ChatMode,
  ChatModeInfo,
  Conversation,
  ConversationDetail,
  ConversationExport,
} from "../../types/chat";

/** Loads the backend's authoritative chat-mode catalog. */
export async function fetchChatModes(): Promise<ChatModeInfo[]> {
  return invoke<ChatModeInfo[]>("list_chat_modes");
}

/** Lists a project's conversations (most recently updated first). */
export async function fetchConversations(projectId: string): Promise<Conversation[]> {
  return invoke<Conversation[]>("list_conversations", { projectId });
}

/** Loads a conversation with its messages. */
export async function openConversationRequest(id: string): Promise<ConversationDetail> {
  return invoke<ConversationDetail>("open_conversation", { id });
}

/** Creates a conversation; resolves with the created record. */
export async function createConversationRequest(
  projectId: string,
  title: string,
  mode: ChatMode,
): Promise<Conversation> {
  return invoke<Conversation>("create_conversation", { projectId, title, mode });
}

/** Renames a conversation; resolves with the fresh detail. */
export async function renameConversationRequest(
  id: string,
  title: string,
): Promise<ConversationDetail> {
  return invoke<ConversationDetail>("rename_conversation", { id, title });
}

/** Switches a conversation's mode; resolves with the fresh detail. */
export async function setConversationModeRequest(
  id: string,
  mode: ChatMode,
): Promise<ConversationDetail> {
  return invoke<ConversationDetail>("set_conversation_mode", { id, mode });
}

/** Deletes a conversation (messages cascade). */
export async function deleteConversationRequest(id: string): Promise<void> {
  return invoke<void>("delete_conversation", { id });
}

/** Appends a message; resolves with the fresh conversation detail. */
export async function appendMessageRequest(
  conversationId: string,
  role: "user",
  content: string,
): Promise<ConversationDetail> {
  return invoke<ConversationDetail>("append_message", { conversationId, role, content });
}

/** Renders the conversation's Markdown transcript export. */
export async function exportConversationRequest(id: string): Promise<ConversationExport> {
  return invoke<ConversationExport>("export_conversation", { id });
}

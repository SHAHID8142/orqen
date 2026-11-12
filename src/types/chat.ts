/**
 * Chat domain types, mirroring the Rust DTOs in
 * `src-tauri/src/commands/chat.rs`. Both sides change in the same commit.
 */

/** The seven chat modes of GOVERNANCE.md §3 (lowercase ids). */
export type ChatMode =
  | "interview"
  | "research"
  | "planning"
  | "build"
  | "debug"
  | "review"
  | "discussion";

/** Message roles. The Phase 4 UI sends `user`; the schema accepts the
 * other roles for later phases' recorded turns. */
export type ChatRole = "user" | "assistant" | "system";

/** The backend's authoritative mode catalog (GOVERNANCE.md §3 purposes). */
export interface ChatModeInfo {
  id: ChatMode;
  purpose: string;
}

/** One persisted conversation. */
export interface Conversation {
  id: string;
  projectId: string;
  title: string;
  mode: ChatMode;
  createdAt: string;
  updatedAt: string;
  messageCount: number;
}

/** One persisted message. `mode` records the conversation's mode when
 * the message was sent, so later mode switches leave history honest. */
export interface ChatMessage {
  id: string;
  conversationId: string;
  role: ChatRole;
  mode: ChatMode;
  content: string;
  createdAt: string;
}

/** A conversation with its full message history. */
export interface ConversationDetail {
  conversation: Conversation;
  messages: ChatMessage[];
}

/** A Markdown transcript export (docs/DECISIONS.md D-026). */
export interface ConversationExport {
  conversationId: string;
  title: string;
  filename: string;
  content: string;
}

/** Shared validation limits — mirror `commands/chat.rs` (D-012: code
 * points, not UTF-16 units). */
export const MAX_TITLE_LENGTH = 120;
export const MAX_MESSAGE_LENGTH = 20000;

/** Client-side draft validation (instant feedback; Rust is
 * authoritative). Returns null when the value is acceptable. */
export function validateDraftTitle(raw: string): string | null {
  const title = raw.trim();
  if (title.length === 0) return "Conversation title must not be empty.";
  if ([...title].length > MAX_TITLE_LENGTH)
    return `Conversation title must not exceed ${MAX_TITLE_LENGTH} characters.`;
  return null;
}

export function validateDraftMessage(raw: string): string | null {
  const content = raw.trim();
  if (content.length === 0) return "Message must not be empty.";
  if ([...content].length > MAX_MESSAGE_LENGTH)
    return `Message must not exceed ${MAX_MESSAGE_LENGTH} characters.`;
  return null;
}

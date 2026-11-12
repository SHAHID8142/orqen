import { useEffect, useRef, useState } from "react";
import "./Chat.css";
import { useChat, useChatStore } from "../../lib/chat/ChatContext";
import { useWorkspace } from "../../lib/workspace/WorkspaceContext";
import type { ChatMode } from "../../types/chat";
import { MAX_MESSAGE_LENGTH, MAX_TITLE_LENGTH } from "../../types/chat";

/**
 * Chat surface (Phase 4): mode-aware, per-project conversations with
 * persistence and Markdown export.
 *
 * Truthful scoping: conversations are records only — NOTHING here invokes
 * an agent. Chat modes shape the conversation's framing (GOVERNANCE.md §3)
 * and never grant execution privileges; agent execution in chat arrives
 * with orchestration (Phase 6) behind its own trust decision.
 */

const MODE_LABELS: Record<ChatMode, string> = {
  interview: "Interview",
  research: "Research",
  planning: "Planning",
  build: "Build",
  debug: "Debug",
  review: "Review",
  discussion: "Discussion",
};

function formatTimestamp(iso: string): string {
  const parsed = new Date(iso);
  return Number.isNaN(parsed.getTime()) ? iso : parsed.toLocaleString();
}

function roleLabel(role: string): string {
  return role.charAt(0).toUpperCase() + role.slice(1);
}

function ConversationList() {
  const chat = useChat();
  const store = useChatStore();
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [deleteId, setDeleteId] = useState<string | null>(null);

  if (chat.conversations.length === 0) {
    return (
      <p className="chat-list-empty" data-testid="chat-list-empty">
        No conversations yet — start one below.
      </p>
    );
  }

  return (
    <ul className="chat-list" aria-label="Conversations">
      {chat.conversations.map((conversation) => (
        <li key={conversation.id}>
          {renamingId === conversation.id ? (
            <form
              className="chat-rename-form"
              onSubmit={(event) => {
                event.preventDefault();
                void store.renameConversation(conversation.id, renameDraft).then((ok) => {
                  if (ok) setRenamingId(null);
                });
              }}
            >
              <input
                type="text"
                value={renameDraft}
                maxLength={MAX_TITLE_LENGTH}
                onChange={(event) => setRenameDraft(event.target.value)}
                aria-label={`New title for ${conversation.title}`}
                autoFocus
              />
              <button type="submit" disabled={chat.pending !== null}>
                Save
              </button>
              <button type="button" onClick={() => setRenamingId(null)}>
                Cancel
              </button>
            </form>
          ) : (
            <div
              className={
                chat.openConversationId === conversation.id
                  ? "chat-list-item chat-list-item-active"
                  : "chat-list-item"
              }
            >
              <button
                type="button"
                className="chat-list-open"
                onClick={() => void store.openConversation(conversation.id)}
                aria-current={chat.openConversationId === conversation.id}
              >
                <strong>{conversation.title}</strong>
                <span className="chat-list-meta">
                  {MODE_LABELS[conversation.mode]} · {conversation.messageCount}{" "}
                  {conversation.messageCount === 1 ? "message" : "messages"}
                </span>
              </button>
              <span className="chat-list-actions">
                <button
                  type="button"
                  onClick={() => {
                    setRenamingId(conversation.id);
                    setRenameDraft(conversation.title);
                  }}
                  disabled={chat.pending !== null}
                >
                  Rename
                </button>
                <button
                  type="button"
                  onClick={() => setDeleteId(conversation.id)}
                  disabled={chat.pending !== null}
                >
                  Delete
                </button>
              </span>
            </div>
          )}
          {deleteId === conversation.id && (
            <div className="chat-delete-confirm" role="group" aria-label={`Confirm deleting ${conversation.title}`}>
              <p>
                Delete “{conversation.title}” and its{" "}
                {conversation.messageCount}{" "}
                {conversation.messageCount === 1 ? "message" : "messages"}? This
                cannot be undone.
              </p>
              <button
                type="button"
                className="danger"
                autoFocus
                onClick={() => {
                  void store.deleteConversation(conversation.id).then(() => setDeleteId(null));
                }}
                disabled={chat.pending !== null}
              >
                Delete conversation
              </button>
              <button type="button" onClick={() => setDeleteId(null)}>
                Cancel
              </button>
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}

function NewConversationForm() {
  const chat = useChat();
  const store = useChatStore();
  const [title, setTitle] = useState("");
  const [mode, setMode] = useState<ChatMode>("discussion");
  const [error, setError] = useState<string | null>(null);

  function submit(event: React.FormEvent) {
    event.preventDefault();
    void store.createConversation(title, mode).then((ok) => {
      if (ok) {
        setTitle("");
        setError(null);
      }
    });
  }

  return (
    <form className="chat-new-form" onSubmit={submit} data-testid="chat-new-form">
      <label htmlFor="chat-new-title">New conversation title</label>
      <input
        id="chat-new-title"
        type="text"
        value={title}
        maxLength={MAX_TITLE_LENGTH}
        onChange={(event) => {
          setTitle(event.target.value);
          setError(null);
          store.clearActionError();
        }}
        placeholder="e.g. Requirements interview, round 1"
      />
      <label htmlFor="chat-new-mode">Starting mode</label>
      <select
        id="chat-new-mode"
        value={mode}
        onChange={(event) => setMode(event.target.value as ChatMode)}
      >
        {chat.modes.map((modeInfo) => (
          <option key={modeInfo.id} value={modeInfo.id}>
            {MODE_LABELS[modeInfo.id]} — {modeInfo.purpose}
          </option>
        ))}
      </select>
      {error && (
        <p className="chat-error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" disabled={chat.pending !== null || title.trim().length === 0}>
        Start conversation
      </button>
    </form>
  );
}

function Thread() {
  const chat = useChat();
  const store = useChatStore();
  const threadEnd = useRef<HTMLDivElement | null>(null);
  const [draft, setDraft] = useState("");
  const [draftError, setDraftError] = useState<string | null>(null);

  const conversation = chat.conversations.find(
    (c) => c.id === chat.openConversationId,
  );

  useEffect(() => {
    // jsdom (tests) has no scrollIntoView — guard the optional call.
    threadEnd.current?.scrollIntoView?.({ block: "nearest" });
  }, [chat.openMessages.length]);

  if (!conversation) {
    return (
      <div className="chat-thread chat-thread-empty" data-testid="chat-thread-empty">
        <p>Select a conversation on the left, or start a new one.</p>
      </div>
    );
  }

  function send(event: React.FormEvent) {
    event.preventDefault();
    void store.sendMessage(draft).then((ok) => {
      if (ok) {
        setDraft("");
        setDraftError(null);
      } else {
        setDraftError(chat.actionError?.message ?? null);
      }
    });
  }

  return (
    <div className="chat-thread" aria-label={`Conversation: ${conversation.title}`}>
      <header className="chat-thread-header">
        <div>
          <h2>{conversation.title}</h2>
          <p className="chat-thread-meta">
            {conversation.messageCount}{" "}
            {conversation.messageCount === 1 ? "message" : "messages"} · created{" "}
            {formatTimestamp(conversation.createdAt)}
          </p>
        </div>
        <div className="chat-thread-actions">
          <label className="option-note" htmlFor={`chat-mode-${conversation.id}`}>
            Mode (shapes framing only — never execution permissions)
          </label>
          <select
            id={`chat-mode-${conversation.id}`}
            value={conversation.mode}
            disabled={chat.pending !== null}
            onChange={(event) =>
              void store.setMode(conversation.id, event.target.value as ChatMode)
            }
          >
            {chat.modes.map((modeInfo) => (
              <option key={modeInfo.id} value={modeInfo.id}>
                {MODE_LABELS[modeInfo.id]} — {modeInfo.purpose}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={() => void store.showExport()}
            disabled={chat.pending !== null || conversation.messageCount === 0}
          >
            Export transcript
          </button>
        </div>
      </header>

      <ol className="chat-messages" aria-label="Messages">
        {chat.openMessages.map((message) => (
          <li
            key={message.id}
            className={`chat-message chat-message-${message.role}`}
          >
            <span className="chat-message-meta">
              <strong>{roleLabel(message.role)}</strong> · {MODE_LABELS[message.mode]} ·{" "}
              {formatTimestamp(message.createdAt)}
            </span>
            <p className="chat-message-content">{message.content}</p>
          </li>
        ))}
        {chat.openMessages.length === 0 && (
          <li className="chat-message chat-message-empty">
            No messages yet — write the first one below.
          </li>
        )}
      </ol>
      <div ref={threadEnd} />

      <form className="chat-composer" onSubmit={send}>
        <label className="option-note" htmlFor="chat-composer-input">
          Message (recorded as a user turn; agents do not respond in chat yet —
          execution arrives with orchestration)
        </label>
        <textarea
          id="chat-composer-input"
          value={draft}
          maxLength={MAX_MESSAGE_LENGTH}
          rows={3}
          onChange={(event) => {
            setDraft(event.target.value);
            setDraftError(null);
            store.clearActionError();
          }}
          placeholder="Type a message…"
        />
        {draftError && (
          <p className="chat-error" role="alert">
            {draftError}
          </p>
        )}
        <div className="chat-composer-actions">
          <span className="option-note" aria-live="polite">
            {[...draft].length}/{MAX_MESSAGE_LENGTH} characters
          </span>
          <button
            type="submit"
            disabled={chat.pending !== null || draft.trim().length === 0}
          >
            {chat.pending === "append" ? "Saving…" : "Send"}
          </button>
        </div>
      </form>
    </div>
  );
}

function ExportPanel() {
  const chat = useChat();
  const store = useChatStore();
  const [copied, setCopied] = useState(false);
  const panelRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!chat.export) return;
    // Modal overlay (Phase 1 overlay rule): move focus in, and let
    // Escape close it.
    panelRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        store.dismissExport();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chat.export !== null]);

  if (!chat.export) return null;

  return (
    <div
      className="chat-export"
      role="dialog"
      aria-modal="true"
      aria-label="Transcript export"
      tabIndex={-1}
      ref={panelRef}
      data-testid="chat-export"
    >
      <h2>
        Transcript export — <code>{chat.export.filename}</code>
      </h2>
      <p className="option-note">
        A Markdown transcript (D-026). Copy it wherever you need it; Orqen does
        not write files outside its data directory.
      </p>
      <pre className="chat-export-content" tabIndex={0}>
        {chat.export.content}
      </pre>
      <div className="chat-export-actions">
        <button
          type="button"
          onClick={() => {
            void navigator.clipboard
              ?.writeText(chat.export?.content ?? "")
              .then(() => setCopied(true))
              .catch(() => setCopied(false));
          }}
        >
          {copied ? "Copied!" : "Copy to clipboard"}
        </button>
        <button type="button" onClick={() => store.dismissExport()}>
          Close
        </button>
      </div>
    </div>
  );
}

function Chat() {
  const chat = useChat();
  const store = useChatStore();
  const workspace = useWorkspace();
  const selectedProjectId = workspace.data.selectedProjectId;

  useEffect(() => {
    void store.loadModes();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    void store.loadConversations(selectedProjectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedProjectId]);

  return (
    <section aria-labelledby="chat-heading" className="chat">
      <h1 id="chat-heading" tabIndex={-1}>
        Chat
      </h1>
      <p className="chat-intro">
        Mode-aware conversations per project. Modes (GOVERNANCE.md §3) shape
        framing and recordkeeping — <strong>they never grant execution
        privileges</strong>, and nothing in chat invokes an agent yet.
      </p>

      {!selectedProjectId && (
        <div className="notice-card" data-testid="chat-no-project">
          <p>
            Chat conversations belong to a project. Create or select a project
            first.
          </p>
        </div>
      )}

      {selectedProjectId && chat.listPhase === "loading" && (
        <p className="option-note" data-testid="chat-loading">
          Loading conversations…
        </p>
      )}
      {selectedProjectId && chat.listPhase === "error" && (
        <div className="error-card" role="alert" data-testid="chat-error">
          <p>{chat.loadError?.message ?? "Conversations are unavailable."}</p>
          <button
            type="button"
            onClick={() => void store.loadConversations(selectedProjectId)}
          >
            Retry
          </button>
        </div>
      )}

      {selectedProjectId && chat.listPhase === "ready" && (
        <div className="chat-layout">
          <div className="chat-side">
            <NewConversationForm />
            <ConversationList />
          </div>
          <Thread />
        </div>
      )}

      {chat.actionError && (
        <div className="error-card" role="alert" data-testid="chat-action-error">
          <p>{chat.actionError.message}</p>
          <button type="button" onClick={() => store.clearActionError()}>
            Dismiss
          </button>
        </div>
      )}

      <ExportPanel />
    </section>
  );
}

export default Chat;

//! Chat infrastructure: mode-aware conversations persisted per project.
//!
//! Phase 4 scope (ROADMAP.md): a mode-aware chat surface with per-project
//! conversation persistence and message export. Chat modes shape how
//! conversations are framed (GOVERNANCE.md §3) and NEVER grant execution
//! privileges — nothing in this module invokes an agent; execution arrives
//! with orchestration (Phase 6) behind its own explicit trust decision.
//!
//! Validation follows the shared policies:
//! - string lengths count Unicode scalar values (code points) on both IPC
//!   sides (docs/DECISIONS.md D-012);
//! - ids/modes/roles are validated against allowlists before any SQL;
//! - every mutating command returns the fresh state so the frontend
//!   converges on backend truth.

use crate::db::DbState;
use crate::error::{CommandError, CommandResult};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// Maximum conversation title length (code points, D-012).
pub const MAX_TITLE_LENGTH: usize = 120;
/// Maximum message content length (code points, D-012). Generous: chat
/// messages can carry pasted plans and logs, but must stay bounded.
pub const MAX_MESSAGE_LENGTH: usize = 20000;

/// The seven chat modes of GOVERNANCE.md §3, with their authoritative
/// purpose strings. Modes shape conversation framing; execution control
/// is separate and is never derived from a mode.
pub const CHAT_MODES: &[(&str, &str)] = &[
    (
        "interview",
        "Requirements gathering toward a Project Blueprint.",
    ),
    (
        "research",
        "Investigate docs, code, and options; no code changes.",
    ),
    (
        "planning",
        "Turn researched understanding into a chunked plan with acceptance criteria.",
    ),
    (
        "build",
        "Frame work on an approved plan chunk (execution itself stays gated).",
    ),
    (
        "debug",
        "Reproduce, isolate, and fix a defect; update lessons when validated.",
    ),
    (
        "review",
        "Audit a change set or proposal produced by another agent.",
    ),
    (
        "discussion",
        "Open exploration and proposals among user and agents; no commits.",
    ),
];

/// Validates a mode id against the GOVERNANCE.md §3 allowlist.
pub fn validated_mode(raw: &str) -> Result<String, CommandError> {
    if CHAT_MODES.iter().any(|(id, _)| *id == raw) {
        Ok(raw.to_string())
    } else {
        let known = CHAT_MODES
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
            .join(", ");
        Err(CommandError::InvalidInput(format!(
            "\"{raw}\" is not a chat mode. Supported modes: {known}."
        )))
    }
}

/// Validates a message role. The chat infrastructure accepts all three
/// roles so later phases can record assistant/system turns; the Phase 4
/// UI only ever sends `user`.
pub fn validated_role(raw: &str) -> Result<String, CommandError> {
    match raw {
        "user" | "assistant" | "system" => Ok(raw.to_string()),
        other => Err(CommandError::InvalidInput(format!(
            "\"{other}\" is not a message role."
        ))),
    }
}

/// Validates and normalizes a conversation title (trim, non-empty,
/// code-point limit — the D-012 policy).
pub fn validated_title(raw: &str) -> Result<String, CommandError> {
    let title = raw.trim();
    let length = title.chars().count();
    if length == 0 {
        return Err(CommandError::InvalidInput(
            "Conversation title must not be empty.".to_string(),
        ));
    }
    if length > MAX_TITLE_LENGTH {
        return Err(CommandError::InvalidInput(format!(
            "Conversation title must not exceed {MAX_TITLE_LENGTH} characters."
        )));
    }
    Ok(title.to_string())
}

/// Validates message content (non-empty after trim, code-point limit).
pub fn validated_content(raw: &str) -> Result<String, CommandError> {
    let content = raw.trim();
    let length = content.chars().count();
    if length == 0 {
        return Err(CommandError::InvalidInput(
            "Message must not be empty.".to_string(),
        ));
    }
    if length > MAX_MESSAGE_LENGTH {
        return Err(CommandError::InvalidInput(format!(
            "Message must not exceed {MAX_MESSAGE_LENGTH} characters."
        )));
    }
    Ok(content.to_string())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// One persisted conversation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDto {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub mode: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
}

/// One persisted message.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub mode: String,
    pub content: String,
    pub created_at: String,
}

/// A conversation with its full message history.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetailDto {
    pub conversation: ConversationDto,
    pub messages: Vec<MessageDto>,
}

/// A Markdown transcript export (docs/DECISIONS.md D-026).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationExportDto {
    pub conversation_id: String,
    pub title: String,
    /// Suggested file name (slug + `.md`).
    pub filename: String,
    pub content: String,
}

/// The static chat-mode catalog for the UI (GOVERNANCE.md §3).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatModeDto {
    pub id: String,
    pub purpose: String,
}

#[tauri::command]
pub fn list_chat_modes() -> CommandResult<Vec<ChatModeDto>> {
    Ok(CHAT_MODES
        .iter()
        .map(|(id, purpose)| ChatModeDto {
            id: (*id).to_string(),
            purpose: (*purpose).to_string(),
        })
        .collect())
}

/// Lists a project's conversations (most recently updated first). The
/// project must exist — an unknown id is an input error, not an empty
/// list (truthful states).
pub fn list_conversations_conn(
    conn: &Connection,
    project_id: &str,
) -> Result<Vec<ConversationDto>, String> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT id FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if exists.is_none() {
        return Err(format!("Unknown project \"{project_id}\"."));
    }
    let mut statement = conn
        .prepare(
            "SELECT c.id, c.project_id, c.title, c.mode, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS message_count
             FROM conversations c
             WHERE c.project_id = ?1
             ORDER BY c.updated_at DESC, c.created_at DESC, c.id",
        )
        .map_err(|e| e.to_string())?;
    let conversations = statement
        .query_map(params![project_id], |row| {
            Ok(ConversationDto {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                mode: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                message_count: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(conversations)
}

fn load_conversation(conn: &Connection, id: &str) -> Result<ConversationDto, String> {
    conn.query_row(
        "SELECT c.id, c.project_id, c.title, c.mode, c.created_at, c.updated_at,
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS message_count
         FROM conversations c
         WHERE c.id = ?1",
        params![id],
        |row| {
            Ok(ConversationDto {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                mode: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                message_count: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
    .and_then(|found| found.ok_or_else(|| format!("Unknown conversation \"{id}\".")))
}

fn load_messages(conn: &Connection, conversation_id: &str) -> Result<Vec<MessageDto>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, conversation_id, role, mode, content, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at, id",
        )
        .map_err(|e| e.to_string())?;
    let messages = statement
        .query_map(params![conversation_id], |row| {
            Ok(MessageDto {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                mode: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(messages)
}

/// Loads a conversation with its messages. Unknown ids are input errors.
pub fn open_conversation_conn(
    conn: &Connection,
    id: &str,
) -> Result<ConversationDetailDto, String> {
    let conversation = load_conversation(conn, id)?;
    let messages = load_messages(conn, id)?;
    Ok(ConversationDetailDto {
        conversation,
        messages,
    })
}

/// Creates a conversation in a project and returns it. The project must
/// exist (the foreign key would also reject it; the explicit check gives
/// a friendlier error).
pub fn create_conversation_conn(
    conn: &mut Connection,
    project_id: &str,
    raw_title: &str,
    raw_mode: &str,
) -> Result<ConversationDto, CommandError> {
    let title = validated_title(raw_title)?;
    let mode = validated_mode(raw_mode)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let transaction = conn.transaction().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("saving the conversation", &e))
    })?;
    let project_exists: Option<String> = transaction
        .query_row(
            "SELECT id FROM projects WHERE id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error("saving the conversation", &e))
        })?;
    if project_exists.is_none() {
        return Err(CommandError::InvalidInput(format!(
            "Unknown project \"{project_id}\" — select or create a project first."
        )));
    }
    transaction
        .execute(
            "INSERT INTO conversations (id, project_id, title, mode, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, project_id, title, mode, now],
        )
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error("saving the conversation", &e))
        })?;
    let conversation = load_conversation(&transaction, &id).map_err(CommandError::Storage)?;
    transaction.commit().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("saving the conversation", &e))
    })?;
    Ok(conversation)
}

fn touch_conversation(transaction: &rusqlite::Transaction, id: &str) -> Result<(), CommandError> {
    let updated = transaction
        .execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now_rfc3339(), id],
        )
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error("updating the conversation", &e))
        })?;
    if updated == 0 {
        return Err(CommandError::InvalidInput(format!(
            "Unknown conversation \"{id}\"."
        )));
    }
    Ok(())
}

/// Renames a conversation and returns the fresh detail.
pub fn rename_conversation_conn(
    conn: &mut Connection,
    id: &str,
    raw_title: &str,
) -> Result<ConversationDetailDto, CommandError> {
    let title = validated_title(raw_title)?;
    let transaction = conn.transaction().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("renaming the conversation", &e))
    })?;
    let updated = transaction
        .execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now_rfc3339(), id],
        )
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error("renaming the conversation", &e))
        })?;
    if updated == 0 {
        return Err(CommandError::InvalidInput(format!(
            "Unknown conversation \"{id}\"."
        )));
    }
    let conversation = load_conversation(&transaction, id).map_err(CommandError::Storage)?;
    let messages = load_messages(&transaction, id).map_err(CommandError::Storage)?;
    transaction.commit().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("renaming the conversation", &e))
    })?;
    Ok(ConversationDetailDto {
        conversation,
        messages,
    })
}

/// Switches a conversation's mode. Modes shape framing only — switching
/// never changes or grants execution permissions (GOVERNANCE.md §3).
pub fn set_conversation_mode_conn(
    conn: &mut Connection,
    id: &str,
    raw_mode: &str,
) -> Result<ConversationDetailDto, CommandError> {
    let mode = validated_mode(raw_mode)?;
    let transaction = conn
        .transaction()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("updating the mode", &e)))?;
    touch_conversation(&transaction, id)?;
    transaction
        .execute(
            "UPDATE conversations SET mode = ?1 WHERE id = ?2",
            params![mode, id],
        )
        .map_err(|e| CommandError::Storage(crate::db::describe_error("updating the mode", &e)))?;
    let conversation = load_conversation(&transaction, id).map_err(CommandError::Storage)?;
    let messages = load_messages(&transaction, id).map_err(CommandError::Storage)?;
    transaction
        .commit()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("updating the mode", &e)))?;
    Ok(ConversationDetailDto {
        conversation,
        messages,
    })
}

/// Deletes a conversation (messages cascade via the foreign key).
pub fn delete_conversation_conn(conn: &mut Connection, id: &str) -> Result<(), CommandError> {
    let transaction = conn.transaction().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("deleting the conversation", &e))
    })?;
    let deleted = transaction
        .execute("DELETE FROM conversations WHERE id = ?1", params![id])
        .map_err(|e| {
            CommandError::Storage(crate::db::describe_error("deleting the conversation", &e))
        })?;
    if deleted == 0 {
        return Err(CommandError::InvalidInput(format!(
            "Unknown conversation \"{id}\"."
        )));
    }
    transaction.commit().map_err(|e| {
        CommandError::Storage(crate::db::describe_error("deleting the conversation", &e))
    })?;
    Ok(())
}

/// Appends a message and returns the fresh conversation detail. The
/// message's `mode` records the conversation's mode at send time, so a
/// later mode switch leaves the history honestly annotated.
pub fn append_message_conn(
    conn: &mut Connection,
    conversation_id: &str,
    raw_role: &str,
    raw_content: &str,
) -> Result<ConversationDetailDto, CommandError> {
    let role = validated_role(raw_role)?;
    let content = validated_content(raw_content)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let transaction = conn
        .transaction()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the message", &e)))?;
    let conversation_mode: String = transaction
        .query_row(
            "SELECT mode FROM conversations WHERE id = ?1",
            params![conversation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the message", &e)))?
        .ok_or_else(|| {
            CommandError::InvalidInput(format!("Unknown conversation \"{conversation_id}\"."))
        })?;
    transaction
        .execute(
            "INSERT INTO messages (id, conversation_id, role, mode, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, conversation_id, role, conversation_mode, content, now],
        )
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the message", &e)))?;
    touch_conversation(&transaction, conversation_id)?;
    let conversation =
        load_conversation(&transaction, conversation_id).map_err(CommandError::Storage)?;
    let messages = load_messages(&transaction, conversation_id).map_err(CommandError::Storage)?;
    transaction
        .commit()
        .map_err(|e| CommandError::Storage(crate::db::describe_error("saving the message", &e)))?;
    Ok(ConversationDetailDto {
        conversation,
        messages,
    })
}

/// Slugifies a conversation title for the export file name: lowercase,
/// ASCII word characters and dashes only, collapsed; falls back to
/// `conversation` when nothing survives. Export file names are derived
/// from user content, so the allowlist keeps them safe for filesystems.
fn slugify(raw: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "conversation".to_string()
    } else {
        slug
    }
}

/// Renders a conversation as a Markdown transcript (docs/DECISIONS.md
/// D-026). Structured messages the user pasted (COMMUNICATION.md §2
/// blocks such as `Q-ID:` questions) are preserved VERBATIM in message
/// content — the transcript never rewrites what a participant said. The
/// header carries subject/scope metadata per COMMUNICATION.md §1, and
/// the footer states the honesty rule: transcripts are records, not
/// authorizations.
pub fn export_conversation_conn(
    conn: &Connection,
    id: &str,
) -> Result<ConversationExportDto, String> {
    let conversation = load_conversation(conn, id)?;
    let messages = load_messages(conn, id)?;
    let project_name: String = conn
        .query_row(
            "SELECT name FROM projects WHERE id = ?1",
            params![conversation.project_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let mut content = String::new();
    content.push_str(&format!(
        "# Conversation export: {}\n\n",
        conversation.title
    ));
    content.push_str(&format!("- Project: {project_name}\n"));
    content.push_str(&format!("- Current mode: {}\n", conversation.mode));
    content.push_str(&format!("- Messages: {}\n", messages.len()));
    content.push_str(&format!("- Conversation id: {}\n", conversation.id));
    content.push_str(&format!("- Exported: {}\n\n", now_rfc3339()));
    content.push_str("## Transcript\n\n");
    if messages.is_empty() {
        content.push_str("_(This conversation has no messages.)_\n\n");
    }
    for (index, message) in messages.iter().enumerate() {
        content.push_str(&format!(
            "### [{}] {} — {} — {}\n\n",
            index + 1,
            message.role,
            message.mode,
            message.created_at
        ));
        content.push_str(message.content.trim());
        content.push_str("\n\n");
    }
    content.push_str(
        "---\n\n_Exported from Orqen. Chat transcripts are records, not \
         authorizations — chat modes never grant execution privileges \
         (GOVERNANCE.md §3)._\n",
    );

    Ok(ConversationExportDto {
        conversation_id: conversation.id,
        title: conversation.title.clone(),
        filename: format!("{}.md", slugify(&conversation.title)),
        content,
    })
}

/// Lists a project's conversations.
#[tauri::command]
pub fn list_conversations(
    db: tauri::State<DbState>,
    project_id: String,
) -> CommandResult<Vec<ConversationDto>> {
    db.with_conn(|conn| {
        list_conversations_conn(conn, &project_id).map_err(CommandError::InvalidInput)
    })
}

/// Loads a conversation with its messages.
#[tauri::command]
pub fn open_conversation(
    db: tauri::State<DbState>,
    id: String,
) -> CommandResult<ConversationDetailDto> {
    db.with_conn(|conn| open_conversation_conn(conn, &id).map_err(CommandError::InvalidInput))
}

/// Creates a conversation and returns it.
#[tauri::command]
pub fn create_conversation(
    db: tauri::State<DbState>,
    project_id: String,
    title: String,
    mode: String,
) -> CommandResult<ConversationDto> {
    db.with_conn(|conn| create_conversation_conn(conn, &project_id, &title, &mode))
}

/// Renames a conversation and returns the fresh detail.
#[tauri::command]
pub fn rename_conversation(
    db: tauri::State<DbState>,
    id: String,
    title: String,
) -> CommandResult<ConversationDetailDto> {
    db.with_conn(|conn| rename_conversation_conn(conn, &id, &title))
}

/// Switches a conversation's mode and returns the fresh detail.
#[tauri::command]
pub fn set_conversation_mode(
    db: tauri::State<DbState>,
    id: String,
    mode: String,
) -> CommandResult<ConversationDetailDto> {
    db.with_conn(|conn| set_conversation_mode_conn(conn, &id, &mode))
}

/// Deletes a conversation (messages cascade).
#[tauri::command]
pub fn delete_conversation(db: tauri::State<DbState>, id: String) -> CommandResult<()> {
    db.with_conn(|conn| delete_conversation_conn(conn, &id))
}

/// Appends a message and returns the fresh conversation detail.
#[tauri::command]
pub fn append_message(
    db: tauri::State<DbState>,
    conversation_id: String,
    role: String,
    content: String,
) -> CommandResult<ConversationDetailDto> {
    db.with_conn(|conn| append_message_conn(conn, &conversation_id, &role, &content))
}

/// Renders a conversation's Markdown transcript export.
#[tauri::command]
pub fn export_conversation(
    db: tauri::State<DbState>,
    id: String,
) -> CommandResult<ConversationExportDto> {
    db.with_conn(|conn| export_conversation_conn(conn, &id).map_err(CommandError::InvalidInput))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod temp_dir_guard {
        pub struct TempDir(pub std::path::PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn open_test_db() -> (temp_dir_guard::TempDir, Connection) {
        let dir = crate::db::temp_db_dir();
        let conn = crate::db::open_database(&dir).expect("open test db");
        (temp_dir_guard::TempDir(dir), conn)
    }

    fn seed_project(conn: &Connection, id: &str, name: &str) {
        conn.execute(
            "INSERT INTO projects (id, name, created_at) VALUES (?1, ?2, '2026-01-01T00:00:00Z')",
            params![id, name],
        )
        .expect("seed project");
    }

    #[test]
    fn mode_validation_accepts_the_seven_governance_modes_only() {
        for &(id, purpose) in CHAT_MODES {
            assert_eq!(validated_mode(id).expect(id), id);
            assert!(!purpose.is_empty());
        }
        assert!(validated_mode("yolo").is_err());
        assert!(validated_mode("").is_err());
        assert!(validated_mode("INTERVIEW").is_err(), "ids are lowercase");
    }

    #[test]
    fn role_validation_accepts_only_the_three_roles() {
        for role in ["user", "assistant", "system"] {
            assert_eq!(validated_role(role).expect(role), role);
        }
        assert!(validated_role("agent").is_err());
        assert!(validated_role("").is_err());
    }

    #[test]
    fn title_and_content_validation_follow_the_code_point_policy() {
        assert!(validated_title("  Plan the thing  ").is_ok());
        assert!(validated_title("   ").is_err());
        assert!(validated_title("").is_err());
        // 121 code points: rejected; the emoji counts as ONE code point.
        let long = "a".repeat(MAX_TITLE_LENGTH + 1);
        assert!(validated_title(&long).is_err());
        let emoji = "🦀".repeat(MAX_TITLE_LENGTH);
        assert_eq!(
            validated_title(&emoji)
                .expect("emoji title")
                .chars()
                .count(),
            MAX_TITLE_LENGTH
        );

        assert!(validated_content("hello").is_ok());
        assert!(validated_content("   ").is_err());
        let long_message = "x".repeat(MAX_MESSAGE_LENGTH + 1);
        assert!(validated_content(&long_message).is_err());
    }

    #[test]
    fn conversation_crud_round_trip_persists_per_project() {
        let (_guard, mut conn) = open_test_db();
        seed_project(&conn, "p1", "Demo");
        let created =
            create_conversation_conn(&mut conn, "p1", " Interview the owner ", "interview")
                .expect("create");
        assert_eq!(created.title, "Interview the owner");
        assert_eq!(created.mode, "interview");
        assert_eq!(created.message_count, 0);

        let list = list_conversations_conn(&conn, "p1").expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, created.id);

        // Unknown project: an input error, not an empty list.
        assert!(list_conversations_conn(&conn, "ghost").is_err());

        // Rename + mode switch.
        let renamed =
            rename_conversation_conn(&mut conn, &created.id, "Owner interview").expect("rename");
        assert_eq!(renamed.conversation.title, "Owner interview");
        let switched =
            set_conversation_mode_conn(&mut conn, &created.id, "planning").expect("mode switch");
        assert_eq!(switched.conversation.mode, "planning");

        // Delete cascades.
        delete_conversation_conn(&mut conn, &created.id).expect("delete");
        assert_eq!(list_conversations_conn(&conn, "p1").expect("list").len(), 0);
        assert!(open_conversation_conn(&conn, &created.id).is_err());
    }

    #[test]
    fn unknown_project_and_conversation_are_input_errors() {
        let (_guard, mut conn) = open_test_db();
        let error = create_conversation_conn(&mut conn, "ghost", "t", "research")
            .expect_err("unknown project");
        assert!(error.to_string().contains("Unknown project"));
        assert!(open_conversation_conn(&conn, "ghost").is_err());
        assert!(rename_conversation_conn(&mut conn, "ghost", "t").is_err());
        assert!(set_conversation_mode_conn(&mut conn, "ghost", "research").is_err());
        assert!(delete_conversation_conn(&mut conn, "ghost").is_err());
    }

    #[test]
    fn messages_append_in_order_and_record_the_mode_at_send_time() {
        let (_guard, mut conn) = open_test_db();
        seed_project(&conn, "p1", "Demo");
        let conversation =
            create_conversation_conn(&mut conn, "p1", "Thread", "research").expect("create");

        let after_first =
            append_message_conn(&mut conn, &conversation.id, "user", "What do we know?")
                .expect("append");
        assert_eq!(after_first.messages.len(), 1);
        assert_eq!(after_first.messages[0].role, "user");
        assert_eq!(after_first.messages[0].mode, "research");

        // Switch the mode, then append: the NEW message records the new
        // mode; the old one keeps the mode in effect when it was sent.
        set_conversation_mode_conn(&mut conn, &conversation.id, "planning").expect("switch");
        let after_second =
            append_message_conn(&mut conn, &conversation.id, "user", "Plan it.").expect("append");
        assert_eq!(after_second.messages.len(), 2);
        assert_eq!(after_second.messages[0].mode, "research");
        assert_eq!(after_second.messages[1].mode, "planning");
        assert_eq!(after_second.conversation.message_count, 2);
        assert_eq!(after_second.conversation.mode, "planning");
    }

    #[test]
    fn deleting_a_project_cascades_to_conversations_and_messages() {
        let (_guard, mut conn) = open_test_db();
        seed_project(&conn, "p1", "Doomed");
        let conversation =
            create_conversation_conn(&mut conn, "p1", "Thread", "review").expect("create");
        append_message_conn(&mut conn, &conversation.id, "user", "hello").expect("append");
        conn.execute("DELETE FROM projects WHERE id = 'p1'", [])
            .expect("delete project");
        let conversations: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .expect("conversation count");
        let messages: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .expect("message count");
        assert_eq!(conversations, 0, "conversation must cascade");
        assert_eq!(messages, 0, "messages must cascade");
    }

    #[test]
    fn message_constraints_reject_unknown_roles_and_modes() {
        let (_guard, mut conn) = open_test_db();
        seed_project(&conn, "p1", "Demo");
        let conversation =
            create_conversation_conn(&mut conn, "p1", "Thread", "review").expect("create");
        assert!(
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, mode, content, created_at)
                 VALUES ('m1', ?1, 'ghost', 'review', 'x', '2026-01-01T00:00:00Z')",
                params![conversation.id],
            )
            .is_err(),
            "unknown role must be rejected by the CHECK constraint"
        );
        assert!(
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, mode, content, created_at)
                 VALUES ('m2', ?1, 'user', 'yolo', 'x', '2026-01-01T00:00:00Z')",
                params![conversation.id],
            )
            .is_err(),
            "unknown mode must be rejected by the CHECK constraint"
        );
        assert!(
            conn.execute(
                "INSERT INTO messages (id, conversation_id, role, mode, content, created_at)
                 VALUES ('m3', 'ghost-conversation', 'user', 'review', 'x', '2026-01-01T00:00:00Z')",
                [],
            )
            .is_err(),
            "unknown conversation must be rejected by the foreign key"
        );
    }

    #[test]
    fn export_renders_an_honest_markdown_transcript() {
        let (_guard, mut conn) = open_test_db();
        seed_project(&conn, "p1", "Blueprint Demo");
        let conversation =
            create_conversation_conn(&mut conn, "p1", "Owner Interview: Scope", "interview")
                .expect("create");
        append_message_conn(
            &mut conn,
            &conversation.id,
            "user",
            "Q-ID: Q-1 | Scope: blueprint\nQuestion: Which platforms are in scope?",
        )
        .expect("append");
        set_conversation_mode_conn(&mut conn, &conversation.id, "planning").expect("switch");
        append_message_conn(&mut conn, &conversation.id, "user", "Desktop only for now.")
            .expect("append");

        let export = export_conversation_conn(&conn, &conversation.id).expect("export");
        assert_eq!(export.filename, "owner-interview-scope.md");
        assert!(
            export
                .content
                .contains("# Conversation export: Owner Interview: Scope")
        );
        assert!(export.content.contains("- Project: Blueprint Demo"));
        assert!(export.content.contains("- Current mode: planning"));
        assert!(export.content.contains("- Messages: 2"));
        // Structured messages are preserved VERBATIM (COMMUNICATION.md §2).
        assert!(export.content.contains("Q-ID: Q-1 | Scope: blueprint"));
        assert!(
            export
                .content
                .contains("Question: Which platforms are in scope?")
        );
        // Mode-per-message honesty: the first message records interview.
        assert!(export.content.contains("### [1] user — interview —"));
        assert!(export.content.contains("### [2] user — planning —"));
        // Honesty footer.
        assert!(export.content.contains("not authorizations"));
    }

    #[test]
    fn export_of_an_empty_conversation_is_truthful() {
        let (_guard, mut conn) = open_test_db();
        seed_project(&conn, "p1", "Demo");
        let conversation =
            create_conversation_conn(&mut conn, "p1", "一緒 about", "discussion").expect("create");
        let export = export_conversation_conn(&conn, &conversation.id).expect("export");
        assert!(export.content.contains("no messages"));
        // ASCII fragments survive; a purely non-ASCII title slugs to the
        // filesystem-safe fallback.
        assert_eq!(export.filename, "about.md");
    }

    #[test]
    fn slugify_is_filesystem_safe() {
        assert_eq!(slugify("Plan the thing"), "plan-the-thing");
        assert_eq!(slugify("  --  "), "conversation");
        assert_eq!(slugify("a/b\\c:d*e?f"), "a-b-c-d-e-f");
        assert_eq!(slugify("DEMO"), "demo");
    }

    #[test]
    fn list_chat_modes_returns_the_catalog() {
        let modes = list_chat_modes().expect("modes");
        assert_eq!(modes.len(), CHAT_MODES.len());
        assert_eq!(modes[0].id, "interview");
    }
}

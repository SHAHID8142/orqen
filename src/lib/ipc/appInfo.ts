/**
 * Typed IPC bridge for application information commands.
 *
 * All frontend access to Tauri commands goes through this layer so that
 * command names, payloads and error normalization stay in one place.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  CommandErrorKind,
  CommandErrorPayload,
} from "../../types/app";

/** Maximum accepted length for an echo message (mirrors the Rust limit). */
export const ECHO_PING_MAX_LENGTH = 200;

/**
 * Shared length policy (docs/DECISIONS.md D-012): strings are measured in
 * Unicode scalar values (code points) — not UTF-16 code units or UTF-8
 * bytes. `[...value]` iterates code points, so Bangla combining marks and
 * multi-unit emoji are counted identically to Rust's `chars().count()`.
 * Note: composed emoji such as "👍🏽" (thumb + skin-tone modifier) are two
 * code points by this policy.
 */
function codePointLength(value: string): number {
  return [...value].length;
}

/** Fetches application metadata from the Rust core. */
export async function fetchAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

/**
 * Sends a message to the Rust core and receives it back.
 *
 * Verifies the full IPC round trip with input validation on both sides.
 * Rejects before invoking for input that cannot be valid.
 */
export async function echoPing(message: string): Promise<string> {
  const trimmed = message.trim();
  if (codePointLength(trimmed) === 0) {
    throw new Error("Echo message must not be empty.");
  }
  if (codePointLength(trimmed) > ECHO_PING_MAX_LENGTH) {
    throw new Error(
      `Echo message must not exceed ${ECHO_PING_MAX_LENGTH} characters.`,
    );
  }
  return invoke<string>("echo_ping", { message: trimmed });
}

function isCommandErrorPayload(value: unknown): value is CommandErrorPayload {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  const kind = candidate["kind"];
  const message = candidate["message"];
  return (
    (kind === "invalid_input" || kind === "internal") &&
    typeof message === "string"
  );
}

/**
 * Normalizes any thrown value into a user-presentable error payload.
 *
 * Tauri command errors arrive as the serialized `CommandErrorPayload` from
 * the Rust core; anything else becomes a generic internal error.
 */
export function normalizeCommandError(error: unknown): CommandErrorPayload {
  if (isCommandErrorPayload(error)) {
    return { kind: error.kind as CommandErrorKind, message: error.message };
  }
  return {
    kind: "internal",
    message: error instanceof Error ? error.message : String(error),
  };
}

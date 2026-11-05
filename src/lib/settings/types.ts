/**
 * Settings domain model — persisted by the Rust core (SQLite).
 *
 * These are saved preferences, not operational configuration: Telegram
 * notifications and autonomous execution are NOT functional yet (no
 * Telegram integration, no orchestration engine — see docs/ROADMAP.md).
 */
import type { CommandErrorPayload } from "../../types/app";
import type { Settings, SettingsInput } from "../../types/workspace";

export type { Settings, SettingsInput };

/** Allowed bounds for the retry preference. */
export const MIN_RETRY = 1;
export const MAX_RETRY_LIMIT = 10;

/** Overall persistence phase. `error` means no valid settings exist yet. */
export type SettingsPhase = "loading" | "ready" | "error";

/** Full observable settings state. */
export interface SettingsState {
  phase: SettingsPhase;
  data: Settings;
  /** Load failure when no valid settings exist yet (drives the retry UI). */
  loadError: CommandErrorPayload | null;
  /** Failure of the latest update (shown near the controls). */
  actionError: CommandErrorPayload | null;
  saving: boolean;
}

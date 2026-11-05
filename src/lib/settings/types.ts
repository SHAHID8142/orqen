/**
 * Settings domain model — in-memory only (Phase 1).
 *
 * These are user preferences, not operational configuration: Telegram
 * notifications and autonomous execution are NOT functional yet (no
 * Telegram integration, no orchestration engine — see docs/ROADMAP.md).
 * Persistence arrives in Phase 2.
 */

/** How execution approvals behave once orchestration exists (Phase 6+). */
export type ApprovalMode = "manual" | "autonomous";

export interface SettingsState {
  approvalMode: ApprovalMode;
  /** Retry cap for failed micro-tasks, clamped to 1..MAX_RETRY_LIMIT. */
  maxRetry: number;
  telegramNotifications: boolean;
}

/** Bounds for the retry preference. */
export const MIN_RETRY = 1;
export const MAX_RETRY_LIMIT = 10;

/** Defaults per docs/GOVERNANCE.md §Execution controls. */
export const DEFAULT_SETTINGS: SettingsState = {
  approvalMode: "manual",
  maxRetry: 3,
  telegramNotifications: true,
};

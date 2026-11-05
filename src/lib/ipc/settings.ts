/**
 * Typed IPC bridge for persistent settings.
 */
import { invoke } from "@tauri-apps/api/core";
import type { Settings, SettingsInput } from "../../types/workspace";

/** Loads the persisted settings. */
export async function fetchSettings(): Promise<Settings> {
  return invoke<Settings>("load_settings");
}

/** Updates settings; resolves with the stored result. */
export async function updateSettingsRequest(input: SettingsInput): Promise<Settings> {
  return invoke<Settings>("update_settings", { settings: input });
}

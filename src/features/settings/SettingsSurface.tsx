import { useState } from "react";
import type { KeyboardEvent } from "react";
import { useSettings, useSettingsStore } from "../../lib/settings/SettingsContext";
import { MAX_RETRY_LIMIT, MIN_RETRY } from "../../lib/settings/types";
import "./SettingsSurface.css";

/**
 * Settings surface: in-memory preference controls.
 *
 * Truthful by construction: Telegram notifications and autonomous
 * execution are preferences only — neither integration is operational
 * yet (see docs/GOVERNANCE.md §Execution controls and docs/ROADMAP.md).
 */
function SettingsSurface() {
  const settings = useSettings();
  const store = useSettingsStore();
  // Draft for the retry field: while editing, the input shows exactly what
  // the user typed (cleared or intermediate values are never snapped back);
  // the value is validated and committed on blur or Enter (R3).
  const [retryDraft, setRetryDraft] = useState<string | null>(null);

  function commitRetryDraft() {
    if (retryDraft === null) return;
    const parsed = Number(retryDraft);
    if (retryDraft.trim() !== "" && Number.isFinite(parsed)) {
      store.setMaxRetry(parsed);
    }
    setRetryDraft(null);
  }

  function handleRetryKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      commitRetryDraft();
    } else if (event.key === "Escape") {
      setRetryDraft(null);
    }
  }

  return (
    <section aria-labelledby="settings-heading" className="settings">
      <h1 id="settings-heading" tabIndex={-1}>
        Settings
      </h1>
      <p className="surface-lead">
        Session preferences with safe defaults. They apply in memory for this
        run only — persistence arrives in Phase 2.
      </p>

      <fieldset className="settings-group">
        <legend>Approval mode</legend>
        <div className="radio-option">
          <input
            id="approval-manual"
            type="radio"
            name="approval-mode"
            value="manual"
            checked={settings.approvalMode === "manual"}
            onChange={() => store.setApprovalMode("manual")}
          />
          <label htmlFor="approval-manual">
            Manual <span className="option-note">— approve each feature chunk (default)</span>
          </label>
        </div>
        <div className="radio-option">
          <input
            id="approval-autonomous"
            type="radio"
            name="approval-mode"
            value="autonomous"
            checked={settings.approvalMode === "autonomous"}
            onChange={() => store.setApprovalMode("autonomous")}
          />
          <label htmlFor="approval-autonomous">
            Autonomous{" "}
            <span className="option-note">
              — preference only; no orchestration engine is operational yet
            </span>
          </label>
        </div>
      </fieldset>

      <div className="settings-group">
        <label htmlFor="max-retry">Maximum retry (per failed micro-task)</label>
        <div className="retry-row">
          <input
            id="max-retry"
            type="number"
            min={MIN_RETRY}
            max={MAX_RETRY_LIMIT}
            value={retryDraft ?? String(settings.maxRetry)}
            onChange={(event) => setRetryDraft(event.target.value)}
            onBlur={commitRetryDraft}
            onKeyDown={handleRetryKeyDown}
          />
          <span className="option-note">
            Allowed range {MIN_RETRY}–{MAX_RETRY_LIMIT}; committed on leaving
            the field, out-of-range values are clamped.
          </span>
        </div>
      </div>

      <div className="settings-group">
        <div className="checkbox-option">
          <input
            id="telegram-notifications"
            type="checkbox"
            checked={settings.telegramNotifications}
            onChange={(event) => store.setTelegramNotifications(event.target.checked)}
          />
          <label htmlFor="telegram-notifications">
            Telegram notifications{" "}
            <span className="option-note">
              — preference only; Telegram is not connected yet
            </span>
          </label>
        </div>
      </div>

      <div className="settings-actions">
        <button type="button" onClick={() => store.resetToDefaults()}>
          Restore defaults
        </button>
      </div>

      <p className="settings-note" data-testid="settings-state">
        Current session values — approval mode: {settings.approvalMode}, maximum
        retry: {settings.maxRetry}, Telegram notifications:{" "}
        {settings.telegramNotifications ? "on" : "off"}.
      </p>
    </section>
  );
}

export default SettingsSurface;

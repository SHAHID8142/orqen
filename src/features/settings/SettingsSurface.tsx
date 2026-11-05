import { useState } from "react";
import type { KeyboardEvent } from "react";
import { useSettings, useSettingsStore } from "../../lib/settings/SettingsContext";
import { MAX_RETRY_LIMIT, MIN_RETRY } from "../../lib/settings/types";
import "./SettingsSurface.css";

/**
 * Settings surface: persisted preference controls.
 *
 * Truthful by construction: Telegram notifications and autonomous
 * execution are saved preferences only — neither integration is
 * operational yet (see docs/GOVERNANCE.md §Execution controls).
 */
function SettingsSurface() {
  const settings = useSettings();
  const store = useSettingsStore();
  // Draft for the retry field: while editing, the input shows exactly what
  // the user typed (cleared or intermediate values are never snapped back);
  // the value is validated and committed on blur or Enter.
  const [retryDraft, setRetryDraft] = useState<string | null>(null);

  const busy = settings.saving;

  function commitRetryDraft() {
    if (retryDraft === null) return;
    const parsed = Number(retryDraft);
    if (retryDraft.trim() !== "" && Number.isFinite(parsed)) {
      // The field hint promises clamping; the store re-validates strictly.
      const clamped = Math.min(
        MAX_RETRY_LIMIT,
        Math.max(MIN_RETRY, Math.round(parsed)),
      );
      void store.update({
        approvalMode: settings.data.approvalMode,
        maxRetry: clamped,
        telegramNotifications: settings.data.telegramNotifications,
      });
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

  if (settings.phase === "loading") {
    return (
      <section aria-labelledby="settings-heading" aria-busy="true" className="settings">
        <h1 id="settings-heading" tabIndex={-1}>
          Settings
        </h1>
        <p className="surface-lead" data-testid="settings-loading">
          Loading settings…
        </p>
      </section>
    );
  }

  if (settings.phase === "error") {
    return (
      <section aria-labelledby="settings-heading" className="settings">
        <h1 id="settings-heading" tabIndex={-1}>
          Settings
        </h1>
        <div className="error-card" role="alert" data-testid="settings-error">
          <h2>Could not load settings</h2>
          <p>
            {settings.loadError?.message ??
              "The local database is unavailable. Your data is untouched."}
          </p>
          <button type="button" onClick={() => void store.load()}>
            Retry
          </button>
        </div>
      </section>
    );
  }

  return (
    <section aria-labelledby="settings-heading" className="settings">
      <h1 id="settings-heading" tabIndex={-1}>
        Settings
      </h1>
      <p className="surface-lead">
        Saved locally via the Rust core — changes survive application
        restarts.
      </p>

      <fieldset className="settings-group" disabled={busy}>
        <legend>Approval mode</legend>
        <div className="radio-option">
          <input
            id="approval-manual"
            type="radio"
            name="approval-mode"
            value="manual"
            checked={settings.data.approvalMode === "manual"}
            onChange={() => {
              void store.update({
                approvalMode: "manual",
                maxRetry: settings.data.maxRetry,
                telegramNotifications: settings.data.telegramNotifications,
              });
            }}
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
            checked={settings.data.approvalMode === "autonomous"}
            onChange={() => {
              void store.update({
                approvalMode: "autonomous",
                maxRetry: settings.data.maxRetry,
                telegramNotifications: settings.data.telegramNotifications,
              });
            }}
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
            disabled={busy}
            value={retryDraft ?? String(settings.data.maxRetry)}
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
            disabled={busy}
            checked={settings.data.telegramNotifications}
            onChange={(event) => {
              void store.update({
                approvalMode: settings.data.approvalMode,
                maxRetry: settings.data.maxRetry,
                telegramNotifications: event.target.checked,
              });
            }}
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
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void store.update({
              approvalMode: "manual",
              maxRetry: 3,
              telegramNotifications: true,
            });
          }}
        >
          Restore defaults
        </button>
      </div>

      {settings.actionError && (
        <p className="field-error" role="alert" data-testid="settings-action-error">
          {settings.actionError.message}
        </p>
      )}

      <p className="settings-note" data-testid="settings-state">
        Current saved values — approval mode: {settings.data.approvalMode},
        maximum retry: {settings.data.maxRetry}, Telegram notifications:{" "}
        {settings.data.telegramNotifications ? "on" : "off"}.
      </p>
    </section>
  );
}

export default SettingsSurface;

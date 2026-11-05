/**
 * Settings surface tests: preference controls work against a fresh
 * in-memory store and are labeled truthfully as preferences only.
 */
import { afterEach, describe, expect, it } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SettingsProvider } from "../../lib/settings/SettingsContext";
import { createSettingsStore, type SettingsStore } from "../../lib/settings/store";
import SettingsSurface from "./SettingsSurface";

function renderSettings(store: SettingsStore = createSettingsStore()) {
  return render(
    <SettingsProvider store={store}>
      <SettingsSurface />
    </SettingsProvider>,
  );
}

afterEach(() => {
  cleanup();
});

describe("Settings surface", () => {
  it("starts with the governance defaults and labels non-operational features", () => {
    renderSettings();

    expect(screen.getByLabelText(/^Manual/)).toBeChecked();
    expect(screen.getByLabelText(/^Autonomous/)).not.toBeChecked();
    expect(screen.getByText(/preference only; no orchestration/)).toBeInTheDocument();
    expect(screen.getByLabelText(/Telegram notifications/)).toBeChecked();
    expect(
      screen.getByText(/Telegram is not connected yet/),
    ).toBeInTheDocument();
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "approval mode: manual, maximum retry: 3, Telegram notifications: on",
    );
  });

  it("switches the approval mode preference", () => {
    renderSettings();
    fireEvent.click(screen.getByLabelText(/^Autonomous/));
    expect(screen.getByLabelText(/^Autonomous/)).toBeChecked();
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "approval mode: autonomous",
    );
  });

  it("clamps out-of-range retry values", () => {
    renderSettings();
    const retryInput = screen.getByLabelText(/Maximum retry/);

    fireEvent.change(retryInput, { target: { value: "0" } });
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 1",
    );

    fireEvent.change(retryInput, { target: { value: "999" } });
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 10",
    );

    fireEvent.change(retryInput, { target: { value: "5" } });
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 5",
    );
  });

  it("toggles Telegram notifications and restores defaults", () => {
    renderSettings();
    const telegram = screen.getByLabelText(/Telegram notifications/);

    fireEvent.click(telegram);
    expect(telegram).not.toBeChecked();
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "Telegram notifications: off",
    );

    fireEvent.click(screen.getByRole("button", { name: "Restore defaults" }));
    expect(telegram).toBeChecked();
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "approval mode: manual, maximum retry: 3, Telegram notifications: on",
    );
  });
});

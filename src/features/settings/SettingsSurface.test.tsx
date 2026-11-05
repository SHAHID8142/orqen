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

  it("lets the user type freely and commits the clamped value on blur", () => {
    renderSettings();
    const retryInput = screen.getByLabelText(/Maximum retry/);

    // Intermediate typing is shown as typed — never snapped back mid-edit.
    fireEvent.change(retryInput, { target: { value: "1" } });
    expect(retryInput).toHaveValue(1);
    fireEvent.change(retryInput, { target: { value: "12" } });
    expect(retryInput).toHaveValue(12);

    fireEvent.blur(retryInput);
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 10",
    );
    expect(retryInput).toHaveValue(10);
  });

  it("can clear the field; an empty draft reverts instead of committing", () => {
    renderSettings();
    const retryInput = screen.getByLabelText(/Maximum retry/);

    fireEvent.change(retryInput, { target: { value: "" } });
    expect(retryInput).toHaveValue(null); // stays cleared while editing

    fireEvent.blur(retryInput);
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 3",
    );
    expect(retryInput).toHaveValue(3);
  });

  it("commits on Enter and reverts the draft on Escape", () => {
    renderSettings();
    const retryInput = screen.getByLabelText(/Maximum retry/);

    fireEvent.change(retryInput, { target: { value: "0" } });
    fireEvent.keyDown(retryInput, { key: "Enter" });
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 1",
    );

    fireEvent.change(retryInput, { target: { value: "9" } });
    fireEvent.keyDown(retryInput, { key: "Escape" });
    fireEvent.blur(retryInput);
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "maximum retry: 1",
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

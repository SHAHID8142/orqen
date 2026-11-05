/**
 * Settings surface tests against fake IPC: loading/error/retry states,
 * persisted updates with the draft retry field, busy guards, and the
 * truthful "preference only" labeling.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsProvider } from "../../lib/settings/SettingsContext";
import { createSettingsStore, type SettingsIpc } from "../../lib/settings/store";
import SettingsSurface from "./SettingsSurface";
import type { Settings } from "../../types/workspace";

const DEFAULTS: Settings = {
  approvalMode: "manual",
  maxRetry: 3,
  telegramNotifications: true,
  mainAgentId: null,
};

function makeIpc(overrides: Partial<SettingsIpc> = {}): SettingsIpc & {
  load: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
} {
  return {
    load: vi.fn().mockResolvedValue(DEFAULTS),
    update: vi.fn(),
    ...overrides,
  } as never;
}

function renderSettings(ipc: SettingsIpc) {
  const store = createSettingsStore(ipc);
  render(
    <SettingsProvider store={store}>
      <SettingsSurface />
    </SettingsProvider>,
  );
  return { store, ipc };
}

afterEach(() => {
  cleanup();
});

describe("Settings surface — persistence states", () => {
  it("shows a loading state before settings arrive", () => {
    renderSettings(makeIpc());
    expect(screen.getByTestId("settings-loading")).toBeInTheDocument();
  });

  it("shows an error card with retry when loading fails", async () => {
    const ipc = makeIpc();
    ipc.load.mockRejectedValueOnce({ kind: "storage", message: "database unavailable" });
    const { store } = renderSettings(ipc);
    void store.load();

    await waitFor(() => {
      expect(screen.getByTestId("settings-error")).toBeInTheDocument();
    });
    expect(screen.getByRole("alert")).toHaveTextContent("database unavailable");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.getByTestId("settings-state")).toBeInTheDocument();
    });
  });
});

describe("Settings surface — persisted updates", () => {
  it("shows persisted defaults with truthful preference labeling", async () => {
    const { store } = renderSettings(makeIpc());
    await store.load();
    await waitFor(() => {
      expect(screen.getByTestId("settings-state")).toHaveTextContent(
        "approval mode: manual, maximum retry: 3, Telegram notifications: on",
      );
    });
    expect(screen.getByLabelText(/^Manual/)).toBeChecked();
    expect(screen.getByText(/preference only; no orchestration/)).toBeInTheDocument();
    expect(screen.getByText(/Telegram is not connected yet/)).toBeInTheDocument();
  });

  it("saves an approval-mode change and applies the stored result", async () => {
    const stored: Settings = {
      approvalMode: "autonomous",
      maxRetry: 3,
      telegramNotifications: true,
      mainAgentId: null,
    };
    const ipc = makeIpc();
    ipc.update.mockResolvedValue(stored);
    const { store } = renderSettings(ipc);
    await store.load();

    fireEvent.click(screen.getByLabelText(/^Autonomous/));
    await waitFor(() => {
      expect(screen.getByTestId("settings-state")).toHaveTextContent(
        "approval mode: autonomous",
      );
    });
    expect(ipc.update).toHaveBeenCalledWith({
      approvalMode: "autonomous",
      maxRetry: 3,
      telegramNotifications: true,
    });
  });

  it("lets the user type freely and commits the clamped value on blur", async () => {
    const ipc = makeIpc();
    ipc.update.mockImplementation(async (input: Settings) => Promise.resolve(input));
    const { store } = renderSettings(ipc);
    await store.load();
    await waitFor(() => expect(screen.getByLabelText(/Maximum retry/)).toBeEnabled());

    const retryInput = screen.getByLabelText(/Maximum retry/);
    fireEvent.change(retryInput, { target: { value: "1" } });
    expect(retryInput).toHaveValue(1);
    fireEvent.change(retryInput, { target: { value: "12" } });
    expect(retryInput).toHaveValue(12);

    fireEvent.blur(retryInput);
    await waitFor(() => {
      expect(ipc.update).toHaveBeenCalledWith({
        approvalMode: "manual",
        maxRetry: 10,
        telegramNotifications: true,
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId("settings-state")).toHaveTextContent("maximum retry: 10");
    });
  });

  it("can clear the field; an empty draft reverts instead of committing", async () => {
    const ipc = makeIpc();
    const { store } = renderSettings(ipc);
    await store.load();
    await waitFor(() => expect(screen.getByLabelText(/Maximum retry/)).toBeEnabled());

    const retryInput = screen.getByLabelText(/Maximum retry/);
    fireEvent.change(retryInput, { target: { value: "" } });
    expect(retryInput).toHaveValue(null);
    fireEvent.blur(retryInput);
    expect(ipc.update).not.toHaveBeenCalled();
    expect(screen.getByTestId("settings-state")).toHaveTextContent("maximum retry: 3");
  });

  it("commits on Enter and reverts the draft on Escape", async () => {
    const ipc = makeIpc();
    ipc.update.mockImplementation(async (input: Settings) => Promise.resolve(input));
    const { store } = renderSettings(ipc);
    await store.load();
    await waitFor(() => expect(screen.getByLabelText(/Maximum retry/)).toBeEnabled());

    const retryInput = screen.getByLabelText(/Maximum retry/);
    fireEvent.change(retryInput, { target: { value: "0" } });
    fireEvent.keyDown(retryInput, { key: "Enter" });
    await waitFor(() => {
      expect(ipc.update).toHaveBeenCalledWith({
        approvalMode: "manual",
        maxRetry: 1,
        telegramNotifications: true,
      });
    });

    fireEvent.change(retryInput, { target: { value: "9" } });
    fireEvent.keyDown(retryInput, { key: "Escape" });
    fireEvent.blur(retryInput);
    expect(ipc.update).toHaveBeenCalledTimes(1);
  });

  it("shows a backend failure without losing previous values", async () => {
    const ipc = makeIpc();
    ipc.update.mockRejectedValue({ kind: "storage", message: "write failed" });
    const { store } = renderSettings(ipc);
    await store.load();
    await waitFor(() => expect(screen.getByLabelText(/Maximum retry/)).toBeEnabled());

    fireEvent.click(screen.getByLabelText(/Telegram notifications/));
    await waitFor(() => {
      expect(screen.getByTestId("settings-action-error")).toHaveTextContent("write failed");
    });
    expect(screen.getByTestId("settings-state")).toHaveTextContent(
      "Telegram notifications: on",
    );
    expect(screen.getByLabelText(/Telegram notifications/)).toBeChecked();
  });

  it("restores defaults persistently", async () => {
    const current: Settings = {
      approvalMode: "autonomous",
      maxRetry: 9,
      telegramNotifications: false,
      mainAgentId: null,
    };
    const ipc = makeIpc({ load: vi.fn().mockResolvedValue(current) });
    const defaults: Settings = { ...DEFAULTS };
    ipc.update.mockResolvedValue(defaults);
    const { store } = renderSettings(ipc);
    await store.load();
    await waitFor(() => expect(screen.getByRole("button", { name: "Restore defaults" })).toBeEnabled());

    fireEvent.click(screen.getByRole("button", { name: "Restore defaults" }));
    await waitFor(() => {
      // The update payload covers the three execution preferences; the
      // backend leaves the Main Agent preference untouched.
      expect(ipc.update).toHaveBeenCalledWith({
        approvalMode: "manual",
        maxRetry: 3,
        telegramNotifications: true,
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId("settings-state")).toHaveTextContent(
        "approval mode: manual, maximum retry: 3, Telegram notifications: on",
      );
    });
  });
});

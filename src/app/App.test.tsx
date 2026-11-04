/**
 * Application smoke test: renders the real app shell and verifies the
 * foundation screen against a mocked Tauri IPC layer.
 */
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

// @testing-library auto-cleanup only activates with global hooks,
// which Vitest does not enable by default — register it explicitly.
afterEach(() => {
  cleanup();
});

describe("App (Phase 0 smoke test)", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders the Orqen application shell", () => {
    invokeMock.mockImplementation(() => new Promise(() => undefined));
    render(<App />);

    expect(screen.getByText("Orqen")).toBeInTheDocument();
    expect(screen.getByText("Phase 0 · Foundation")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /Orqen is initialized/i }),
    ).toBeInTheDocument();
  });

  it("displays real runtime information returned by the Rust core", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_app_info") {
        return { name: "Orqen", version: "0.1.0", os: "macos" };
      }
      if (command === "echo_ping") {
        return "ping";
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId("runtime-app")).toHaveTextContent(
        "Orqen v0.1.0",
      );
    });
    expect(screen.getByTestId("runtime-platform")).toHaveTextContent("macos");
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });

  it("shows an error state when the backend is unreachable", async () => {
    invokeMock.mockRejectedValue("backend unavailable");

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        /Could not load runtime information/,
      );
    });
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });
});

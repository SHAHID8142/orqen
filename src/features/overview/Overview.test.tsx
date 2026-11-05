/**
 * Overview surface tests: preserves the Phase 0 runtime diagnostics
 * contract (real IPC-driven states, no simulation).
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
import Overview from "./Overview";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

afterEach(() => {
  cleanup();
});

describe("Overview", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders the heading and section structure", () => {
    invokeMock.mockImplementation(() => new Promise(() => undefined));
    render(<Overview />);

    expect(
      screen.getByRole("heading", { level: 1, name: "Overview" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Runtime status")).toBeInTheDocument();
    expect(screen.getByText("Implemented in this build")).toBeInTheDocument();
    expect(screen.getByText("Planned — not implemented yet")).toBeInTheDocument();
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

    render(<Overview />);

    await waitFor(() => {
      expect(screen.getByTestId("runtime-app")).toHaveTextContent("Orqen v0.1.0");
    });
    expect(screen.getByTestId("runtime-platform")).toHaveTextContent("macos");
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });

  it("shows an error state when the backend is unreachable", async () => {
    invokeMock.mockRejectedValue("backend unavailable");

    render(<Overview />);

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        /Could not load runtime information/,
      );
    });
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });
});

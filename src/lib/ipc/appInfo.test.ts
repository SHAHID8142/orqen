/**
 * Unit tests for the typed IPC bridge (frontend side).
 * The Tauri `invoke` function is mocked; validation and error
 * normalization run for real.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  ECHO_PING_MAX_LENGTH,
  echoPing,
  fetchAppInfo,
  normalizeCommandError,
} from "./appInfo";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

describe("fetchAppInfo", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("passes through the AppInfo payload from the Rust core", async () => {
    const payload = { name: "Orqen", version: "0.1.0", os: "macos" };
    invokeMock.mockResolvedValue(payload);

    await expect(fetchAppInfo()).resolves.toEqual(payload);
    expect(invokeMock).toHaveBeenCalledWith("get_app_info");
  });
});

describe("echoPing", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue("ping");
  });

  it("trims the message and forwards it to the command", async () => {
    await expect(echoPing("  ping  ")).resolves.toBe("ping");
    expect(invokeMock).toHaveBeenCalledWith("echo_ping", { message: "ping" });
  });

  it("rejects empty input without invoking the backend", async () => {
    await expect(echoPing("   ")).rejects.toThrow(/must not be empty/);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("rejects messages beyond the shared length limit", async () => {
    const tooLong = "a".repeat(ECHO_PING_MAX_LENGTH + 1);
    await expect(echoPing(tooLong)).rejects.toThrow(/exceed/i);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("accepts a message exactly at the limit", async () => {
    const atLimit = "a".repeat(ECHO_PING_MAX_LENGTH);
    await expect(echoPing(atLimit)).resolves.toBe("ping");
    expect(invokeMock).toHaveBeenCalledWith("echo_ping", {
      message: atLimit,
    });
  });
});

describe("normalizeCommandError", () => {
  it("passes through a serialized CommandErrorPayload", () => {
    expect(
      normalizeCommandError({
        kind: "invalid_input",
        message: "echo message must not be empty",
      }),
    ).toEqual({
      kind: "invalid_input",
      message: "echo message must not be empty",
    });
  });

  it("wraps an Error as an internal error", () => {
    expect(normalizeCommandError(new Error("boom"))).toEqual({
      kind: "internal",
      message: "boom",
    });
  });

  it("stringifies non-Error throw values", () => {
    expect(normalizeCommandError("backend unavailable")).toEqual({
      kind: "internal",
      message: "backend unavailable",
    });
  });
});

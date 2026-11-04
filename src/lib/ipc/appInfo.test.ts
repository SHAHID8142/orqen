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

  it("accepts Bangla text (code-point policy)", async () => {
    invokeMock.mockResolvedValueOnce("বাংলা ঠিক আছে");
    await expect(echoPing("  বাংলা ঠিক আছে  ")).resolves.toBe("বাংলা ঠিক আছে");
    expect(invokeMock).toHaveBeenCalledWith("echo_ping", {
      message: "বাংলা ঠিক আছে",
    });
  });

  it("accepts emoji text (code-point policy)", async () => {
    // "👍🏽" is two code points (thumb + skin-tone modifier) — still valid.
    invokeMock.mockResolvedValueOnce("🙂👍🏽 হ্যালো");
    await expect(echoPing("🙂👍🏽 হ্যালো")).resolves.toBe("🙂👍🏽 হ্যালো");
    expect(invokeMock).toHaveBeenCalledWith("echo_ping", {
      message: "🙂👍🏽 হ্যালো",
    });
  });

  it("accepts 150 emoji — regression: 300 UTF-16 units / 600 UTF-8 bytes but 150 code points", async () => {
    // Under the old policies this input was rejected by BOTH sides at
    // different thresholds (UTF-16 units: 300 > 200; UTF-8 bytes: 600 > 200),
    // while other scripts diverged in the opposite direction. D-012 counts
    // code points on both sides: 150 code points -> accepted.
    const message = "🙂".repeat(150);
    expect(message.length).toBe(300); // UTF-16 units (surrogate pairs)
    expect([...message].length).toBe(150); // code points — the shared policy
    await expect(echoPing(message)).resolves.toBe("ping");
    expect(invokeMock).toHaveBeenCalledWith("echo_ping", { message });
  });

  it("rejects 201 emoji — 201 code points, over the shared limit", async () => {
    const message = "🙂".repeat(ECHO_PING_MAX_LENGTH + 1);
    await expect(echoPing(message)).rejects.toThrow(/exceed/i);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("rejects 201 Bangla characters — over the shared limit", async () => {
    const message = "ক".repeat(ECHO_PING_MAX_LENGTH + 1);
    await expect(echoPing(message)).rejects.toThrow(/exceed/i);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("accepts a mixed-script message exactly at the limit", async () => {
    const message = "🙂".repeat(100) + "ক".repeat(100); // 200 code points
    await expect(echoPing(message)).resolves.toBe("ping");
    expect(invokeMock).toHaveBeenCalledWith("echo_ping", { message });
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

// @vitest-environment happy-dom
import { act, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  loadLaunchChoice,
  buildFirstRunSave,
  continueIsBlocked,
  saveChoiceThenOpenApp,
  showScreenForChoiceRead,
  useFirstRunScreen,
} from "./useFirstRun";

afterEach(() => {
  clearMocks();
});

describe("showScreenForChoiceRead", () => {
  it("an unreadable registry opens the app instead of trapping the user on the first-run screen", () => {
    expect(showScreenForChoiceRead({ ok: false })).toBe(false);
  });

  it("no saved choice shows the screen and a saved choice skips it", () => {
    expect(showScreenForChoiceRead({ ok: true, choice: null })).toBe(true);
    expect(
      showScreenForChoiceRead({
        ok: true,
        choice: { kept: [], search_project_folders: true, saved_at: "2026-01-01T00:00:00.000Z" },
      }),
    ).toBe(false);
  });
});

describe("continueIsBlocked", () => {
  it("a detection error still lets the user continue with an empty choice or names the screen that traps them", () => {
    expect(continueIsBlocked({ rows: null, error: "probe failed", saving: false })).toBe(false);
  });

  it("detection in flight blocks continue until rows or an error arrive", () => {
    expect(continueIsBlocked({ rows: null, error: null, saving: false })).toBe(true);
    expect(continueIsBlocked({ rows: [], error: null, saving: false })).toBe(false);
  });

  it("a save in progress blocks a second continue", () => {
    expect(continueIsBlocked({ rows: [], error: null, saving: true })).toBe(true);
  });
});

describe("useFirstRunScreen telemetry", () => {
  it("the welcome screen starts the telemetry switch on and saves it on, or the registry's off default silently opts users out", async () => {
    const saved: unknown[] = [];
    mockIPC((command, args) => {
      if (command === "detect_harnesses") throw "fixture: no harness probe";
      if (command === "save_harnesses_choice") saved.push(args);
      return null;
    });
    const { result } = renderHook(() => useFirstRunScreen(() => {}));
    expect(result.current.telemetryEnabled).toBe(true);

    await waitFor(() => expect(result.current.error).not.toBeNull());
    act(() => result.current.continue());
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ telemetryEnabled: true });
  });
});

describe("saveChoiceThenOpenApp", () => {
  it("a failed welcome save keeps the screen and names the reason instead of opening the app", async () => {
    const onSaved = vi.fn();
    const onSaveFailed = vi.fn();
    await saveChoiceThenOpenApp(() => Promise.reject(new Error("registry is read-only")), {
      onSaved,
      onSaveFailed,
    });
    expect(onSaved).not.toHaveBeenCalled();
    expect(onSaveFailed).toHaveBeenCalledOnce();
    expect(onSaveFailed.mock.calls[0]?.[0]).toContain("registry is read-only");
  });

  it("a successful welcome save opens the app exactly once", async () => {
    const onSaved = vi.fn();
    const onSaveFailed = vi.fn();
    await saveChoiceThenOpenApp(() => Promise.resolve(), { onSaved, onSaveFailed });
    expect(onSaved).toHaveBeenCalledOnce();
    expect(onSaveFailed).not.toHaveBeenCalled();
  });
});

describe("buildFirstRunSave", () => {
  it("carries a turned-off telemetry switch into the save call or silently re-enables it", () => {
    const { choice, telemetryEnabled } = buildFirstRunSave(
      new Set(["claude-code"]),
      false,
      false,
      "2026-01-01T00:00:00.000Z",
    );
    expect(telemetryEnabled).toBe(false);
    expect(choice).toEqual({
      kept: ["claude-code"],
      search_project_folders: false,
      saved_at: "2026-01-01T00:00:00.000Z",
    });
  });
});

it("returning launch detects in the background and opens before probes finish or blocks startup", async () => {
  let finish: ((report: { harnesses: [] }) => void) | undefined;
  const detect = vi.fn(
    () =>
      new Promise<{ harnesses: [] }>((resolve) => {
        finish = resolve;
      }),
  );
  const onGate = vi.fn();
  const onReport = vi.fn();
  const choice = {
    kept: ["claude-code"],
    search_project_folders: true,
    saved_at: "2026-01-01T00:00:00Z",
  };
  const onChoice = vi.fn();
  await loadLaunchChoice({
    readChoice: async () => choice,
    detect,
    onChoice,
    onGate,
    onReport,
    onError: vi.fn(),
  });
  expect(onGate).toHaveBeenCalledWith(false);
  expect(detect).toHaveBeenCalledOnce();
  expect(onReport).not.toHaveBeenCalled();
  expect(onChoice).toHaveBeenCalledWith(choice);
  finish?.({ harnesses: [] });
  await Promise.resolve();
  expect(onReport).toHaveBeenCalledWith({ harnesses: [] });
  expect(choice.kept).toEqual(["claude-code"]);
});

it("an unreadable launch choice still detects and opens with every new agent off or leaves a permanent spinner", async () => {
  const onGate = vi.fn();
  const onChoice = vi.fn();
  const onReport = vi.fn();
  const onError = vi.fn();
  const detect = vi.fn(async () => ({ harnesses: [] }));
  await loadLaunchChoice({
    readChoice: async () => {
      throw new Error("saved choice is unreadable");
    },
    detect,
    onGate,
    onChoice,
    onReport,
    onError,
  });
  expect(onGate).toHaveBeenCalledWith(false);
  expect(onChoice).toHaveBeenCalledWith(null);
  expect(detect).toHaveBeenCalledOnce();
  expect(onError).toHaveBeenCalledWith("saved choice is unreadable");
  await Promise.resolve();
  expect(onReport).toHaveBeenCalledWith({ harnesses: [] });
});

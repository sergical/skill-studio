import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { getInstalledSkills } from "./skill-api";
import {
  BUSY_MESSAGE,
  plainBusyMessage,
  setUpdateAllRunning,
  UPDATE_ALL_BUSY_MESSAGE,
} from "./skill-busy-message";

// Same stand-in for `window` as `skill-api-error.test.ts`; `recordIpcCall` also needs a frame scheduler.
Object.assign(globalThis, {
  window: globalThis,
  requestAnimationFrame: (callback: () => void) => setTimeout(callback, 0),
});

describe("plainBusyMessage", () => {
  it("a_core_lease_refusal_reads_as_plain_words_or_shows_the_lease_path", () => {
    const raw = "another process holds the lease on /Users/me/.agents/skills";
    expect(plainBusyMessage(raw, false)).toBe(BUSY_MESSAGE);
  });

  it("the_desktop_write_lease_refusal_reads_as_plain_words_or_shows_the_pid", () => {
    const raw = "Another write is in progress (pid 123, held for 4.2s)";
    expect(plainBusyMessage(raw, false)).toBe(BUSY_MESSAGE);
  });

  it("a_refusal_while_update_all_runs_names_update_all_or_blames_nothing", () => {
    const raw = "another process holds the lease on /Users/me/.agents/skills";
    expect(plainBusyMessage(raw, true)).toBe(UPDATE_ALL_BUSY_MESSAGE);
  });

  it("any_other_error_passes_through_unchanged_or_hides_its_cause", () => {
    expect(plainBusyMessage("disk full", true)).toBe("disk full");
  });
});

describe("a write refused by the lease, through the IPC wrapper", () => {
  afterEach(() => {
    setUpdateAllRunning(false);
    clearMocks();
  });

  it("the_error_a_caller_catches_names_update_all_while_it_runs_or_shows_the_raw_lease_text", async () => {
    mockIPC(() => {
      throw "Another write is in progress (pid 9, held for 3s)";
    });
    await expect(getInstalledSkills()).rejects.toThrow(BUSY_MESSAGE);

    setUpdateAllRunning(true);
    await expect(getInstalledSkills()).rejects.toThrow(UPDATE_ALL_BUSY_MESSAGE);
  });
});

// ============================================================================
// Skill Studio - Sidebar tests
// ============================================================================

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackgroundTaskStatus } from "@skill-studio/lib";
import {
  backgroundRefreshError,
  hasNewerSkillSnapshotEmission,
  relativeScanTime,
  rescanTooltip,
} from "../../lib/sidebar-nav";

const NOW = Date.parse("2026-01-01T12:00:00.000Z");

/** `NOW` minus `ms`, read by a clock pinned at `NOW`, so a slow runner cannot cross a boundary. */
function scannedAgo(ms: number): string {
  return new Date(NOW - ms).toISOString();
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("relativeScanTime", () => {
  it("reads 'Never' when there is no timestamp", () => {
    expect(relativeScanTime(undefined)).toBe("Never");
  });

  it("reads 'Just now' just under the one-minute boundary", () => {
    const scannedAt = scannedAgo(59_999);
    expect(relativeScanTime(scannedAt)).toBe("Just now");
  });

  it("reads '1m ago' at the one-minute boundary", () => {
    const scannedAt = scannedAgo(60_000);
    expect(relativeScanTime(scannedAt)).toBe("1m ago");
  });
});

describe("rescanTooltip", () => {
  it("shows 'Last sync: Never' when timestamp is missing", () => {
    expect(rescanTooltip(undefined)).toBe("Refresh skills from disk\nLast sync: Never");
  });

  it("shows 'Last sync: Just now' for recent scans", () => {
    const scannedAt = scannedAgo(10_000);
    expect(rescanTooltip(scannedAt)).toBe("Refresh skills from disk\nLast sync: Just now");
  });
});

describe("hasNewerSkillSnapshotEmission", () => {
  it("does not complete from an initial snapshot read", () => {
    expect(hasNewerSkillSnapshotEmission(0, undefined)).toBe(false);
  });

  it("completes when a listener delivers a newer backend revision", () => {
    expect(hasNewerSkillSnapshotEmission(4, 5)).toBe(true);
  });
});

describe("backgroundRefreshError", () => {
  const task = (
    worker: BackgroundTaskStatus["worker"],
    running: boolean,
    lastError: string | null,
  ): BackgroundTaskStatus => ({
    worker,
    running,
    last_started_at: null,
    last_finished_at: null,
    last_error: lastError,
  });

  it("reports nothing for a refresh that is only running", () => {
    expect(
      backgroundRefreshError({
        revision: 1,
        tasks: [task("scan", true, null), task("invocations", true, null)],
      }),
    ).toBeNull();
  });

  it("reports a failed scan or usage refresh", () => {
    expect(
      backgroundRefreshError({ revision: 1, tasks: [task("invocations", false, "index locked")] }),
    ).toBe("index locked");
  });

  it("leaves a failed update check to Home", () => {
    expect(
      backgroundRefreshError({ revision: 1, tasks: [task("update_check", false, "offline")] }),
    ).toBeNull();
  });
});

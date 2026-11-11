import { describe, expect, it } from "vitest";
import {
  requestRescanWithFeedback,
  selectNewerBackgroundStatus,
  selectNewerSkillSnapshot,
  startSkillSnapshotSubscription,
} from "./useSkillSnapshot";
import type { SkillSnapshot } from "@skill-studio/lib";

function snapshot(revision: number, scannedAt: string): SkillSnapshot {
  return {
    revision,
    skills: [],
    projects: [],
    invocations: [],
    heatmap: { days: {} },
    scanned_at: scannedAt,
    update_check: {
      checked_at: null,
      gh_status: "ok",
      message: null,
      updates_available: 0,
      upstream_ahead: [],
    },
    opencode_config_kind: null,
    scan_partial: false,
    scan_observations: [],
    unread_roots: [],
  };
}

describe("selectNewerSkillSnapshot", () => {
  it("keeps an event that arrives before an older initial read completes", () => {
    const eventSnapshot = snapshot(4, "event");
    const afterEvent = selectNewerSkillSnapshot(undefined, eventSnapshot);

    expect(selectNewerSkillSnapshot(afterEvent, snapshot(3, "initial"))).toBe(eventSnapshot);
  });

  it("accepts revision zero only as the initial legacy bootstrap", () => {
    const legacy = snapshot(0, "legacy");

    expect(selectNewerSkillSnapshot(undefined, legacy)).toBe(legacy);
    expect(selectNewerSkillSnapshot(legacy, snapshot(0, "second legacy"))).toBe(legacy);
    expect(selectNewerSkillSnapshot(legacy, snapshot(1, "current"))?.revision).toBe(1);
  });
});

describe("startSkillSnapshotSubscription", () => {
  it("registers before reading and rejects a stale initial read", async () => {
    let current: SkillSnapshot | undefined;
    const received: Array<{ revision: number; source: string }> = [];
    let listener: ((snapshot: SkillSnapshot) => void) | undefined;
    let finishRead: ((snapshot: SkillSnapshot) => void) | undefined;
    const read = new Promise<SkillSnapshot>((resolve) => {
      finishRead = resolve;
    });
    const subscription = startSkillSnapshotSubscription({
      isCancelled: () => false,
      listen: async (registeredListener) => {
        listener = registeredListener;
        return () => undefined;
      },
      read: () => read,
      onSnapshot: (candidate, source) => {
        received.push({ revision: candidate.revision, source });
        current = selectNewerSkillSnapshot(current, candidate);
      },
      onError: () => undefined,
      onSettled: () => undefined,
    });
    await Promise.resolve();
    listener?.(snapshot(2, "event"));
    finishRead?.(snapshot(1, "initial"));
    await subscription;

    expect(current?.scanned_at).toBe("event");
    expect(received).toEqual([
      { revision: 2, source: "event" },
      { revision: 1, source: "initial" },
    ]);
  });

  it("disposes a listener that finishes registering after unmount", async () => {
    let cancelled = false;
    let finishListen: ((unlisten: () => void) => void) | undefined;
    let disposed = false;
    let readCalled = false;
    const listen = new Promise<() => void>((resolve) => {
      finishListen = resolve;
    });
    const subscription = startSkillSnapshotSubscription({
      isCancelled: () => cancelled,
      listen: () => listen,
      read: async () => {
        readCalled = true;
        return snapshot(1, "initial");
      },
      onSnapshot: () => undefined,
      onError: () => undefined,
      onSettled: () => undefined,
    });

    cancelled = true;
    finishListen?.(() => {
      disposed = true;
    });
    await subscription;

    expect(disposed).toBe(true);
    expect(readCalled).toBe(false);
  });
});

describe("background status publication", () => {
  it("keeps_a_running_event_when_an_older_initial_read_arrives_or_hides_the_scan", () => {
    const running = { revision: 2, tasks: [] };
    expect(selectNewerBackgroundStatus(running, { revision: 1, tasks: [] })).toBe(running);
    expect(selectNewerBackgroundStatus(undefined, running)).toBe(running);
  });
});

describe("request rescan result", () => {
  it("a_failed_rebuild_reaches_the_caller_and_stops_loading_or_silently_reports_success", async () => {
    const loading: boolean[] = [];
    const errors: string[] = [];
    await expect(
      requestRescanWithFeedback({
        request: async () => {
          throw new Error("fixture rebuild failed");
        },
        isMounted: () => true,
        setLoading: (value) => loading.push(value),
        reportError: (message) => errors.push(message),
      }),
    ).rejects.toThrow("fixture rebuild failed");
    expect(loading).toEqual([true, false]);
    expect(errors).toEqual(["fixture rebuild failed"]);
  });

  it("an_unmounted_view_receives_no_late_loading_or_error_updates_or_changes_disposed_state", async () => {
    const loading: boolean[] = [];
    const errors: string[] = [];
    let mounted = true;
    const result = requestRescanWithFeedback({
      request: async () => {
        mounted = false;
        throw new Error("fixture rebuild failed");
      },
      isMounted: () => mounted,
      setLoading: (value) => loading.push(value),
      reportError: (message) => errors.push(message),
    });
    await expect(result).rejects.toThrow("fixture rebuild failed");
    expect(loading).toEqual([true]);
    expect(errors).toEqual([]);
  });
});

import { describe, expect, it } from "vitest";
import { applyAddSkillOperationEvent, listenForAddSkillOperation } from "./useAddSkillOperation";
import type { AddSkillOperationEvent } from "@skill-studio/lib";

function event(
  operationId: string,
  sequence: number,
  phase: AddSkillOperationEvent["phase"],
): AddSkillOperationEvent {
  return {
    operation_id: operationId,
    sequence,
    phase,
    message: phase,
  };
}

describe("applyAddSkillOperationEvent", () => {
  it("rejects a stale sequence after a newer event", () => {
    const current = event("op-1", 4, "installing");
    expect(applyAddSkillOperationEvent(current, event("op-1", 2, "queued"), "op-1")).toBe(current);
  });

  it("permits a fresh submit after decline and ignores a late parent event", () => {
    let trackedOperationId: string | undefined;
    expect(
      applyAddSkillOperationEvent(undefined, event("op-1", 5, "cancelled"), trackedOperationId),
    ).toBeUndefined();

    trackedOperationId = "op-2";
    const fresh = event("op-2", 1, "queued");
    const lateParent = event("op-1", 5, "cancelled");
    expect(applyAddSkillOperationEvent(fresh, lateParent, trackedOperationId)).toBe(fresh);
  });
});

describe("listenForAddSkillOperation", () => {
  it("registers before any start or status read", async () => {
    const calls: string[] = [];
    let listener: ((event: AddSkillOperationEvent) => void) | undefined;
    await listenForAddSkillOperation({
      isCancelled: () => false,
      listen: async (registeredListener) => {
        calls.push("listen");
        listener = registeredListener;
        return () => undefined;
      },
      onEvent: () => {
        calls.push("event");
      },
    });
    expect(calls).toEqual(["listen"]);
    listener?.(event("op-1", 1, "queued"));
    expect(calls).toEqual(["listen", "event"]);
  });
});

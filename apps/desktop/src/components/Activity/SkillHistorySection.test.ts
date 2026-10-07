// ============================================================================
// Skill Studio - Activity restore safety tests
// ============================================================================

import { describe, expect, it } from "vitest";
import type { SkillEvent } from "@skill-studio/lib";
import {
  canRestoreSkillEvent,
  eventLabel,
  kindLabel,
  shouldOfferForceRestore,
} from "./skill-history-restore-policy";

function event(forceRestorable: boolean): SkillEvent {
  return {
    id: "event",
    ts: "2026-09-05T00:00:00Z",
    kind: "explode_shared_dir",
    skill: "find-bugs",
    status: "done",
    restorable: true,
    force_restorable: forceRestorable,
    harness: null,
    project_path: null,
    reverted_by: null,
    scope: null,
  };
}

describe("shouldOfferForceRestore", () => {
  it("does not offer force when an independent copy boundary makes it unsafe", () => {
    expect(shouldOfferForceRestore(event(false), "/root has changed since the event")).toBe(false);
  });

  it("offers force for an ordinary drift refusal when the backend allows it", () => {
    expect(shouldOfferForceRestore(event(true), "/root has changed since the event")).toBe(true);
  });
});

describe("canRestoreSkillEvent", () => {
  it("hides restore for a non-restorable independent-copy undo event", () => {
    expect(canRestoreSkillEvent({ ...event(false), kind: "restore", restorable: false })).toBe(
      false,
    );
  });
});

describe("eventLabel", () => {
  it("a_split_row_that_names_an_agent_reads_turned_off_for_that_agent_not_split", () => {
    expect(eventLabel({ ...event(false), kind: "split", harness: "codex" }, "Codex")).toBe(
      "Turned off for Codex",
    );
  });

  it("a_plain_split_row_keeps_the_split_label_or_every_split_would_read_as_a_turn_off", () => {
    expect(eventLabel({ ...event(false), kind: "split", harness: null }, null)).toBe("split");
  });
});

describe("kindLabel", () => {
  it("the_quarantine_prune_event_reads_quarantine_pruned_not_quarantine_prune", () => {
    expect(kindLabel("quarantine_prune")).toBe("Quarantine pruned");
  });
});

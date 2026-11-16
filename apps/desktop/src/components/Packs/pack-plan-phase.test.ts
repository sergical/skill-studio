import { describe, expect, it } from "vitest";
import type { PackActionReport } from "@skill-studio/lib";
import { advancePackPlan } from "./pack-plan-phase";
import type { PackPlanEvent, PackPlanState } from "./pack-plan-phase";

function report(states: PackActionReport["steps"][number]["state"][]): PackActionReport {
  return {
    dry_run: true,
    summary: "",
    steps: states.map((state, index) => ({
      source: "owner/repo",
      skill: `skill-${index}`,
      scope: { scope: "global" },
      action: "update",
      state,
    })),
  };
}

const plan = report([{ state: "planned" }, { state: "skipped", reason: "has local edits" }]);
const allSkipped = report([{ state: "skipped", reason: "a plugin" }]);
const result = { ...report([{ state: "done", event_id: "evt" }]), dry_run: false };

const STATES: Record<string, PackPlanState> = {
  planning: { phase: "planning" },
  planned: { phase: "planned", plan },
  "planned (nothing to run)": { phase: "planned", plan: allSkipped },
  running: { phase: "running", plan },
  done: { phase: "done", result },
  failed: { phase: "failed", message: "boom" },
};

const EVENTS: Record<PackPlanEvent["type"], PackPlanEvent> = {
  planned: { type: "planned", plan },
  confirm: { type: "confirm" },
  ran: { type: "ran", result },
  error: { type: "error", message: "boom" },
};

/** Every legal move; any `(state, event)` pair not listed must be refused. */
const LEGAL: Record<string, Partial<Record<PackPlanEvent["type"], PackPlanState["phase"]>>> = {
  planning: { planned: "planned", error: "failed" },
  planned: { confirm: "running" },
  "planned (nothing to run)": {},
  running: { ran: "done", error: "failed" },
  done: {},
  failed: {},
};

describe("advancePackPlan", () => {
  for (const [from, state] of Object.entries(STATES)) {
    for (const [type, event] of Object.entries(EVENTS)) {
      const want = LEGAL[from][type as PackPlanEvent["type"]];
      it(`${from} --${type}--> ${want ?? "refused"}`, () => {
        const move = advancePackPlan(state, event);
        if (want === undefined) {
          expect(move, `illegal move ${from} --${type}--> was allowed`).toMatchObject({ ok: false });
        } else {
          expect(move, `legal move ${from} --${type}--> was refused`).toMatchObject({
            ok: true,
            state: { phase: want },
          });
        }
      });
    }
  }
});

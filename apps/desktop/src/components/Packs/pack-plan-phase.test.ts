import { describe, expect, it } from "vitest";
import type { PackActionReport } from "@skill-studio/lib";
import { advancePackPlan, advancePlanPhase } from "./pack-plan-phase";
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

interface MoveCase {
  from: string;
  state: PackPlanState;
  /** Every legal move from `state`; each event not listed must be refused. */
  legal: Partial<Record<PackPlanEvent["type"], PackPlanState["phase"]>>;
}

const CASES: MoveCase[] = [
  {
    from: "planning",
    state: { phase: "planning" },
    legal: { planned: "planned", error: "failed" },
  },
  { from: "planned", state: { phase: "planned", plan }, legal: { confirm: "running" } },
  { from: "planned (nothing to run)", state: { phase: "planned", plan: allSkipped }, legal: {} },
  { from: "running", state: { phase: "running", plan }, legal: { ran: "done", error: "failed" } },
  { from: "done", state: { phase: "done", result }, legal: {} },
  { from: "failed", state: { phase: "failed", message: "boom" }, legal: {} },
];

const EVENTS: PackPlanEvent[] = [
  { type: "planned", plan },
  { type: "confirm" },
  { type: "ran", result },
  { type: "error", message: "boom" },
];

describe("advancePackPlan", () => {
  for (const { from, state, legal } of CASES) {
    for (const event of EVENTS) {
      const type = event.type;
      const want = legal[type];
      it(`${from} --${type}--> ${want ?? "refused"}`, () => {
        const move = advancePackPlan(state, event);
        if (want === undefined) {
          expect(move, `illegal move ${from} --${type}--> was allowed`).toMatchObject({
            ok: false,
          });
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

describe("advancePlanPhase with a pack export's rule", () => {
  // An export plan runs only when it has files to write.
  const canRun = (plan: { files: string[] }) => plan.files.length > 0;
  const withFiles = { files: ["skills/a/SKILL.md", "README.md"] };
  const empty = { files: [] };

  it.each([
    { name: "a plan with files confirms", plan: withFiles, phase: "running" },
    { name: "a plan with no file stays planned", plan: empty, phase: null },
  ])("$name", ({ plan, phase }) => {
    const move = advancePlanPhase({ phase: "planned", plan }, { type: "confirm" }, canRun);
    expect(move.ok ? move.state.phase : null).toBe(phase);
  });

  it("finishes with the written result or fails with the message", () => {
    const running = { phase: "running" as const, plan: withFiles };
    const done = advancePlanPhase(running, { type: "ran", result: withFiles }, canRun);
    expect(done.ok && done.state.phase).toBe("done");
    const failed = advancePlanPhase(running, { type: "error", message: "not empty" }, canRun);
    expect(failed.ok && failed.state).toEqual({ phase: "failed", message: "not empty" });
  });
});

// ============================================================================
// pack-plan-phase - The pack action dialog's one state machine:
// planning → planned → running → done, and planning | running → failed.
// The dialog asks `advancePackPlan` for each next state and never sets it.
// ============================================================================

import type { PackActionReport } from "@skill-studio/lib";

export type PackPlanState =
  | { phase: "planning" }
  | { phase: "planned"; plan: PackActionReport }
  | { phase: "running"; plan: PackActionReport }
  | { phase: "done"; result: PackActionReport }
  | { phase: "failed"; message: string };

export type PackPlanEvent =
  | { type: "planned"; plan: PackActionReport }
  | { type: "confirm" }
  | { type: "ran"; result: PackActionReport }
  | { type: "error"; message: string };

export type PackPlanMove = { ok: true; state: PackPlanState } | { ok: false; illegal: string };

/** Steps the plan would run: a plan with none has nothing to confirm. */
export function plannedCount(plan: PackActionReport): number {
  return plan.steps.filter((step) => step.state.state === "planned").length;
}

/** The next state for `event`, or the name of the illegal move. */
export function advancePackPlan(state: PackPlanState, event: PackPlanEvent): PackPlanMove {
  const to = (next: PackPlanState): PackPlanMove => ({ ok: true, state: next });
  switch (state.phase) {
    case "planning":
      if (event.type === "planned") return to({ phase: "planned", plan: event.plan });
      if (event.type === "error") return to({ phase: "failed", message: event.message });
      break;
    case "planned":
      if (event.type === "confirm" && plannedCount(state.plan) > 0) {
        return to({ phase: "running", plan: state.plan });
      }
      break;
    case "running":
      if (event.type === "ran") return to({ phase: "done", result: event.result });
      if (event.type === "error") return to({ phase: "failed", message: event.message });
      break;
    case "done":
    case "failed":
      break;
  }
  return { ok: false, illegal: `${state.phase} --${event.type}-->` };
}

// ============================================================================
// pack-plan-phase - The pack dialogs' one state machine (a pack action, and
// a pack export): planning → planned → running → done, and
// planning | running → failed. A dialog asks `advancePackPlan` (or, for
// another report type, `advancePlanPhase`) for each next state and never
// sets it.
// ============================================================================

import type { PackActionReport } from "@skill-studio/lib";

export type PackPlanState<R = PackActionReport> =
  | { phase: "planning" }
  | { phase: "planned"; plan: R }
  | { phase: "running"; plan: R }
  | { phase: "done"; result: R }
  | { phase: "failed"; message: string };

export type PackPlanEvent<R = PackActionReport> =
  | { type: "planned"; plan: R }
  | { type: "confirm" }
  | { type: "ran"; result: R }
  | { type: "error"; message: string };

export type PackPlanMove<R = PackActionReport> =
  | { ok: true; state: PackPlanState<R> }
  | { ok: false; illegal: string };

/** Steps the plan would run: a plan with none has nothing to confirm. */
export function plannedCount(plan: PackActionReport): number {
  return plan.steps.filter((step) => step.state.state === "planned").length;
}

/** The next state for `event`, or the name of the illegal move. A plan
 * moves to running only when `canRun(plan)`: one with nothing to do has
 * nothing to confirm. */
export function advancePlanPhase<R>(
  state: PackPlanState<R>,
  event: PackPlanEvent<R>,
  canRun: (plan: R) => boolean,
): PackPlanMove<R> {
  const to = (next: PackPlanState<R>): PackPlanMove<R> => ({ ok: true, state: next });
  switch (state.phase) {
    case "planning":
      if (event.type === "planned") return to({ phase: "planned", plan: event.plan });
      if (event.type === "error") return to({ phase: "failed", message: event.message });
      break;
    case "planned":
      if (event.type === "confirm" && canRun(state.plan)) {
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

/** `advancePlanPhase` for a pack action: it runs only with planned steps. */
export function advancePackPlan(state: PackPlanState, event: PackPlanEvent): PackPlanMove {
  return advancePlanPhase(state, event, (plan) => plannedCount(plan) > 0);
}

// ============================================================================
// Skill Studio - skill-location-switch-view
// What a Locations row switch shows while its action runs
// ============================================================================

export type RowSwitchPhase =
  | { kind: "idle" }
  | { kind: "pending"; target: boolean }
  | { kind: "held"; target: boolean };

export interface RowSwitchView {
  shown: boolean;
  busy: boolean;
}

export function rowSwitchView({
  checked,
  changesAtOnce,
  phase,
}: {
  checked: boolean;
  changesAtOnce: boolean;
  phase: RowSwitchPhase;
}): RowSwitchView {
  if (phase.kind === "idle") return { shown: checked, busy: false };
  if (phase.kind === "held") return { shown: phase.target, busy: checked !== phase.target };
  return { shown: changesAtOnce ? phase.target : checked, busy: true };
}

export function phaseAfter(ok: boolean, target: boolean): RowSwitchPhase {
  return ok ? { kind: "held", target } : { kind: "idle" };
}

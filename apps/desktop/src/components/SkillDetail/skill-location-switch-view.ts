// ============================================================================
// Skill Studio - skill-location-switch-view
// What a Locations row switch shows while its action runs
// ============================================================================

/** How long a switch holds its new state while it waits for the refreshed row to remount it. */
export const SWITCH_REFRESH_HOLD_MS = 5000;

export type RowSwitchPhase = "idle" | "pending" | "held";

export interface RowSwitchView {
  shown: boolean;
  busy: boolean;
}

/**
 * What a row switch shows. Only an action that changes the copy at once shows the target state
 * while pending; a confirm-first one stays at `checked` so it does not flip and flip back as the
 * confirm opens.
 */
export function rowSwitchView({
  checked,
  changesAtOnce,
  phase,
}: {
  checked: boolean;
  changesAtOnce: boolean;
  phase: RowSwitchPhase;
}): RowSwitchView {
  if (phase === "idle") return { shown: checked, busy: false };
  if (phase === "held") return { shown: !checked, busy: true };
  return { shown: changesAtOnce ? !checked : checked, busy: true };
}

/** The phase after the action settles: a direct change that worked holds until the row remounts. */
export function phaseAfter(ok: boolean, changesAtOnce: boolean): RowSwitchPhase {
  return ok && changesAtOnce ? "held" : "idle";
}

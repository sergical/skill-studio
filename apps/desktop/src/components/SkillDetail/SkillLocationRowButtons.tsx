// ============================================================================
// SkillLocationRowButtons - the control a Locations row shows: a switch (on =
// live, off = parked) for a copy that can be parked or turned back on, or the
// two fix buttons "Keep live" / "Keep parked" on a parked copy a live one came
// back beside.
// ============================================================================

import { useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { SwitchControl } from "../ui/SwitchControl";
import { TooltipControl } from "../ui/TooltipControl";
import { parkActionFor } from "./skill-location-status";
import type { LocationAction, LocationRow } from "./skill-location-status";

/** One ghost text button that shows a spinner while its action runs. */
export function RowActionButton({
  label,
  ariaLabel,
  action,
  onAction,
}: {
  label: string;
  ariaLabel: string;
  action: LocationAction;
  onAction: (action: LocationAction) => Promise<boolean>;
}) {
  const [isPending, setIsPending] = useState(false);
  return (
    <Button
      variant="ghost"
      size="xs"
      aria-label={ariaLabel}
      disabled={isPending}
      onClick={() => {
        setIsPending(true);
        void onAction(action).finally(() => setIsPending(false));
      }}
    >
      {label}
      {isPending && (
        <Loader2
          size={12}
          className="animate-spin motion-reduce:animate-none"
          aria-label="Working"
        />
      )}
    </Button>
  );
}

/**
 * Fixed-width control slot (switch plus spinner space) so switches line up
 * across rows and the row does not shift while the spinner shows.
 */
export const ROW_SWITCH_SLOT = "inline-flex w-10 shrink-0 items-center gap-1";

/**
 * A switch that runs a row action; it shows the target state and a spinner until the action
 * settles. `waitsForRefresh` is for an action that changes the copy at once (no confirm): the
 * backend call returns before the `skills://snapshot` event brings the new row, so the switch
 * holds the new state until `checked` catches up instead of bouncing back for a moment.
 */
export function RowActionSwitch({
  checked,
  ariaLabel,
  action,
  onAction,
  waitsForRefresh = false,
}: {
  checked: boolean;
  ariaLabel: string;
  action: LocationAction;
  onAction: (action: LocationAction) => Promise<boolean>;
  waitsForRefresh?: boolean;
}) {
  const [isPending, setIsPending] = useState(false);
  const [heldTarget, setHeldTarget] = useState<boolean | null>(null);
  if (heldTarget !== null && heldTarget === checked) setHeldTarget(null);
  const isBusy = isPending || heldTarget !== null;
  return (
    <span className={ROW_SWITCH_SLOT}>
      <SwitchControl
        checked={isPending ? !checked : (heldTarget ?? checked)}
        disabled={isBusy}
        ariaLabel={ariaLabel}
        onCheckedChange={() => {
          setIsPending(true);
          void onAction(action)
            .then((ok) => {
              if (ok && waitsForRefresh) setHeldTarget(!checked);
            })
            .catch(() => undefined)
            .finally(() => setIsPending(false));
        }}
      />
      <span className="inline-flex w-3 justify-center">
        {isBusy && (
          <Loader2
            size={12}
            className="animate-spin motion-reduce:animate-none"
            aria-label="Working"
          />
        )}
      </span>
    </span>
  );
}

/** The row's control(s), or nothing when the row has no park action (a link, plugin or reader). */
export function SkillLocationRowButtons({
  row,
  scopeLabel,
  projectPath,
  onAction,
}: {
  row: LocationRow;
  scopeLabel: string;
  projectPath: string | null;
  onAction: (action: LocationAction) => Promise<boolean>;
}) {
  if (row.kind === "parked" && row.deployment) {
    if (row.leftBehind) {
      return (
        <>
          <RowActionButton
            label="Keep live"
            ariaLabel={`Keep the live ${row.harnessLabel} copy, delete the parked one`}
            action={{ kind: "keep-live", pair: row.leftBehind }}
            onAction={onAction}
          />
          <RowActionButton
            label="Keep parked"
            ariaLabel={`Keep the parked ${row.harnessLabel} copy, delete the live one`}
            action={{ kind: "keep-parked", pair: row.leftBehind }}
            onAction={onAction}
          />
        </>
      );
    }
    return (
      <RowActionSwitch
        checked={false}
        ariaLabel={`Turn on the parked ${row.harnessLabel} copy`}
        action={{ kind: "unpark", deployment: row.deployment }}
        onAction={onAction}
        waitsForRefresh
      />
    );
  }
  const park = parkActionFor(row, scopeLabel, projectPath);
  if (!park) return null;
  const isShared = row.kind === "shared";
  return (
    <TooltipControl
      content={`On. Turn off to park ${isShared ? "it for every agent" : "this copy"}: agents stop seeing it until you turn it back on.`}
    >
      <span className="inline-flex">
        <RowActionSwitch
          checked
          ariaLabel={
            isShared
              ? `Park the ${row.harnessLabel} for every agent`
              : `Park the ${row.harnessLabel} copy`
          }
          action={park}
          onAction={onAction}
          // A project copy confirms first (the dialog owns that state); a global one parks at once.
          waitsForRefresh={projectPath === null}
        />
      </span>
    </TooltipControl>
  );
}

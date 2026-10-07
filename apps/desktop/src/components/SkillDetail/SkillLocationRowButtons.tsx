// ============================================================================
// SkillLocationRowButtons - the control a Locations row shows: a switch (on =
// live, off = parked) for a copy that can be parked or turned back on, or the
// two fix buttons "Keep live" / "Keep parked" on a parked copy a live one came
// back beside.
// ============================================================================

import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { SwitchControl } from "../ui/SwitchControl";
import { SWITCH_REFRESH_HOLD_MS, rowSwitchView } from "./skill-location-switch-view";
import type { RowSwitchPhase } from "./skill-location-switch-view";
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
 * A switch that runs a row action and shows a spinner until the action settles. `changesAtOnce`
 * is for an action that changes the copy with no confirm: the backend call returns before the
 * `skills://snapshot` event brings the new row, so the switch holds the new state until the
 * refreshed row remounts it (or `SWITCH_REFRESH_HOLD_MS` passes) instead of bouncing back.
 */
export function RowActionSwitch({
  checked,
  ariaLabel,
  title,
  action,
  onAction,
  changesAtOnce = false,
}: {
  checked: boolean;
  ariaLabel: string;
  title?: string;
  action: LocationAction;
  onAction: (action: LocationAction) => Promise<boolean>;
  changesAtOnce?: boolean;
}) {
  const [phase, setPhase] = useState<RowSwitchPhase>("idle");
  useEffect(() => {
    if (phase !== "held") return;
    const timer = setTimeout(() => setPhase("idle"), SWITCH_REFRESH_HOLD_MS);
    return () => clearTimeout(timer);
  }, [phase]);
  const { shown, busy } = rowSwitchView({ checked, changesAtOnce, phase });
  return (
    <span className={ROW_SWITCH_SLOT}>
      <SwitchControl
        checked={shown}
        disabled={busy}
        ariaLabel={ariaLabel}
        title={title}
        onCheckedChange={() => {
          setPhase("pending");
          void onAction(action)
            .then((ok) => setPhase(ok && changesAtOnce ? "held" : "idle"))
            .catch(() => setPhase("idle"));
        }}
      />
      <span className="inline-flex w-3 justify-center">
        {busy && (
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
        key={row.deployment.id}
        checked={false}
        ariaLabel={`${row.harnessLabel} copy`}
        title="Parked. Turn on to put this copy back where agents see it."
        action={{ kind: "unpark", deployment: row.deployment }}
        onAction={onAction}
        changesAtOnce
      />
    );
  }
  const park = parkActionFor(row, scopeLabel, projectPath);
  if (!park) return null;
  const isShared = row.kind === "shared";
  const parkWhat = isShared ? "park it for every agent" : "park this copy";
  return (
    <RowActionSwitch
      key={row.deployment?.id ?? row.path}
      checked
      ariaLabel={isShared ? `${row.harnessLabel} for every agent` : `${row.harnessLabel} copy`}
      title={`On. Turn off to ${parkWhat}: agents stop seeing it until you turn it back on.`}
      action={park}
      onAction={onAction}
      // A project copy confirms first (the dialog owns that state); a global one parks at once.
      changesAtOnce={projectPath === null}
    />
  );
}

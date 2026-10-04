// ============================================================================
// SkillLocationRowButtons - the one text button a Locations row shows where
// the old switch was: "Park" on a live copy, "Turn on" on a parked copy, or
// the two fixes "Keep live" / "Keep parked" on a parked copy a live one came
// back beside. Each is a Button, not a switch: parking moves a folder.
// ============================================================================

import { useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { parkActionFor } from "./skill-location-status";
import type { LocationAction, LocationRow } from "./skill-location-status";

/** One ghost text button that shows a spinner while its action runs. */
function RowActionButton({
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

/** The row's button(s), or nothing when the row has no park action (a link, plugin or reader). */
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
      <RowActionButton
        label="Turn on"
        ariaLabel={`Turn on the parked ${row.harnessLabel} copy`}
        action={{ kind: "unpark", deployment: row.deployment }}
        onAction={onAction}
      />
    );
  }
  const park = parkActionFor(row, scopeLabel, projectPath);
  if (!park) return null;
  return (
    <RowActionButton
      label="Park"
      ariaLabel={
        row.kind === "shared"
          ? `Park the ${row.harnessLabel} for every agent`
          : `Park the ${row.harnessLabel} copy`
      }
      action={park}
      onAction={onAction}
    />
  );
}

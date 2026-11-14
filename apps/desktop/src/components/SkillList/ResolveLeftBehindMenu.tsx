// ============================================================================
// ResolveLeftBehindMenu - the "Resolve" menu on any row of a skill with a
// parked copy left behind: "Keep live" and "Keep parked". Each opens the same
// confirm the Locations card uses, so nothing is deleted on one click.
// ============================================================================

import { useState } from "react";
import { ChevronDown } from "lucide-react";
import type { LeftBehindPair } from "@skill-studio/lib";
import { LeftBehindDialog } from "../SkillDetail/LeftBehindDialog";
import type { LeftBehindChoice } from "../SkillDetail/LeftBehindDialog";
import { MenuControl, MenuItem } from "../ui/MenuControl";
import { FIX_CLASS } from "./skill-row-format";

export function ResolveLeftBehindMenu({
  skillName,
  pair,
}: {
  skillName: string;
  pair: LeftBehindPair;
}) {
  const [choice, setChoice] = useState<LeftBehindChoice | null>(null);
  return (
    <>
      <MenuControl
        align="end"
        triggerClassName={FIX_CLASS}
        trigger={
          <>
            Resolve
            <ChevronDown aria-hidden />
          </>
        }
      >
        <MenuItem onClick={() => setChoice("keep-live")}>Keep live</MenuItem>
        <MenuItem onClick={() => setChoice("keep-parked")}>Keep parked</MenuItem>
      </MenuControl>
      {choice && (
        <LeftBehindDialog
          skillName={skillName}
          choice={choice}
          pair={pair}
          onClose={() => setChoice(null)}
        />
      )}
    </>
  );
}

// ============================================================================
// HomeLeftBehindActions - the "Resolve" menu on a "parked copy left behind"
// row in Needs attention: "Keep live" and "Keep parked". Each opens the same
// confirm the Locations card uses, so nothing is deleted on one click.
// ============================================================================

import { useState } from "react";
import { ChevronDown } from "lucide-react";
import type { HealthIssue } from "@skill-studio/lib";
import { LeftBehindDialog } from "../SkillDetail/LeftBehindDialog";
import type { LeftBehindChoice } from "../SkillDetail/LeftBehindDialog";
import { MenuControl, MenuItem } from "../ui/MenuControl";
import { ROW_ACTION_CLASS } from "./home-row-click";

export function HomeLeftBehindActions({
  issue,
  live,
  parked,
}: {
  issue: HealthIssue;
  live: NonNullable<HealthIssue["live"]>;
  parked: NonNullable<HealthIssue["parked"]>;
}) {
  const [choice, setChoice] = useState<LeftBehindChoice | null>(null);
  return (
    <>
      <MenuControl
        align="end"
        triggerClassName={ROW_ACTION_CLASS}
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
          skillName={issue.skill.name}
          choice={choice}
          pair={{ live, parked }}
          onClose={() => setChoice(null)}
        />
      )}
    </>
  );
}

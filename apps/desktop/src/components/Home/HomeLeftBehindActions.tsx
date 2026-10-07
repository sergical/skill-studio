// ============================================================================
// HomeLeftBehindActions - the two fixes on a "parked copy left behind" row in
// Needs attention: "Keep live" and "Keep parked". Each opens the same confirm
// the Locations card uses, so nothing is deleted on one click.
// ============================================================================

import { useState } from "react";
import { Button } from "@skill-studio/ui";
import type { HealthIssue } from "@skill-studio/lib";
import { LeftBehindDialog } from "../SkillDetail/LeftBehindDialog";
import type { LeftBehindChoice } from "../SkillDetail/LeftBehindDialog";

const FIX_CLASS =
  "h-9 max-w-full truncate p-0 text-small text-text-tertiary hover:bg-transparent hover:text-accent";

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
      <span className="flex items-center gap-3">
        <Button variant="ghost" className={FIX_CLASS} onClick={() => setChoice("keep-live")}>
          Keep live
        </Button>
        <Button variant="ghost" className={FIX_CLASS} onClick={() => setChoice("keep-parked")}>
          Keep parked
        </Button>
      </span>
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

// ============================================================================
// useGitWarning - asks the core once whether git tracks the copy a confirm is
// about to park or delete, and hands back the sentence to show. The check is
// read-only and never blocks the action: a failed check reads as "couldn't
// check".
// ============================================================================

import { useEffect, useState } from "react";
import type { LifecycleTarget } from "@skill-studio/lib";
import { gitWarningText } from "../components/SkillDetail/skill-location-helpers";
import { parkCheck } from "../lib/skill-api";

/** The warning sentence for `target`, or `null` while checking, when git does not track it, or when `enabled` is false. `doing` is the gerund, e.g. "removing". */
export function useGitWarning(
  target: LifecycleTarget | null,
  doing: string,
  enabled: boolean,
): string | null {
  const [gitTracked, setGitTracked] = useState<boolean | null | undefined>(undefined);
  const deploymentId = target?.deployment_id ?? null;

  useEffect(() => {
    if (!enabled || !deploymentId) return;
    let cancelled = false;
    parkCheck({ deployment_id: deploymentId }).then(
      (check) => {
        if (!cancelled) setGitTracked(check.git_tracked);
      },
      () => {
        if (!cancelled) setGitTracked(null);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [deploymentId, enabled]);

  if (!enabled || gitTracked === undefined) return null;
  return gitWarningText(gitTracked, doing);
}

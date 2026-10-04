// ============================================================================
// useGitWarning - asks the core once whether git tracks the copy a confirm is
// about to park or delete, and hands back the sentence to show. The check is
// read-only and a failed check reads as "couldn't check". While it runs,
// `isChecking` is true so the confirm can wait for the answer.
// ============================================================================

import { useEffect, useState } from "react";
import type { LifecycleTarget } from "@skill-studio/lib";
import { gitWarningText } from "../components/SkillDetail/skill-location-helpers";
import { parkCheck } from "../lib/skill-api";

interface GitWarning {
  /** The warning sentence, or `null` while checking, when git does not track the folder, or when `enabled` is false. */
  warning: string | null;
  /** True from the first render until the check answers; false when nothing is checked. */
  isChecking: boolean;
}

/** The warning for `target`. `doing` is the gerund, e.g. "removing". */
export function useGitWarning(
  target: LifecycleTarget | null,
  doing: string,
  enabled: boolean,
): GitWarning {
  const [answer, setAnswer] = useState<{ id: string; gitTracked: boolean | null } | null>(null);
  const deploymentId = target?.deployment_id ?? null;

  useEffect(() => {
    if (!enabled || !deploymentId) return;
    let cancelled = false;
    parkCheck({ deployment_id: deploymentId }).then(
      (check) => {
        if (!cancelled) setAnswer({ id: deploymentId, gitTracked: check.git_tracked });
      },
      () => {
        if (!cancelled) setAnswer({ id: deploymentId, gitTracked: null });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [deploymentId, enabled]);

  if (!enabled || !deploymentId) return { warning: null, isChecking: false };
  if (answer?.id !== deploymentId) return { warning: null, isChecking: true };
  return { warning: gitWarningText(answer.gitTracked, doing), isChecking: false };
}

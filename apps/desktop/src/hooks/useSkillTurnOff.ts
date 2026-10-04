// ============================================================================
// Skill Studio - useSkillTurnOff
// IPC side of the "Turn off for <Agent>" dialog: asks the backend whether the
// turn-off would be refused, and runs it followed by a rescan.
// ============================================================================

import { useEffect, useState } from "react";
import type { AgentId, AgentOffCheck, LifecycleTarget } from "@skill-studio/lib";
import { requestSkillRescan, turnOffCheck, turnOffForAgent } from "../lib/skill-api";

/** `check` is `null` until the backend answers. A failed check reads as a refusal with no way out, so nothing is written blind. */
export function useSkillTurnOff(target: LifecycleTarget, agent: AgentId) {
  const [check, setCheck] = useState<AgentOffCheck | null>(null);

  useEffect(() => {
    let ignore = false;
    turnOffCheck(target, agent).then(
      (answer) => {
        if (!ignore) setCheck(answer);
      },
      (err) => {
        if (ignore) return;
        const reason = err instanceof Error ? err.message : String(err);
        setCheck({ refusal: { reason, off_everywhere: false }, git_tracked: null, project: null });
      },
    );
    return () => {
      ignore = true;
    };
  }, [target, agent]);

  const turnOff = async () => {
    await turnOffForAgent(target, agent);
    await requestSkillRescan();
  };

  return { check, turnOff };
}

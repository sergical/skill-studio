// ============================================================================
// Skill Studio - useSkillInstalls
// The cached skills.sh install count for one installed skills-sh skill.
// ============================================================================

import { useEffect, useState } from "react";
import type { InstalledSkill } from "@skill-studio/lib";
import { getInstallCounts } from "../lib/skill-api";

/** The install count, or null until it loads and when it is unknown or offline. */
export function useSkillInstalls(skill: InstalledSkill): number | null {
  const { name, source, source_kind: kind } = skill;
  const id = `${source}/${name}`;
  const [count, setCount] = useState<{ id: string; installs: number } | null>(null);
  useEffect(() => {
    if (kind !== "skills-sh") return;
    let cancelled = false;
    getInstallCounts([{ source, name }])
      .then(([result]) => {
        if (!cancelled && result?.installs != null) setCount({ id, installs: result.installs });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [kind, source, name, id]);
  return count?.id === id ? count.installs : null;
}

// ============================================================================
// AgentUses - Which skills one agent used in the last 30 days, from the same
// use history as Home and Activity, so every agent that history covers gets
// an answer, not only the ones whose sessions Skill Studio reads.
// ============================================================================

import { useState } from "react";
import { Button } from "@skill-studio/ui";
import type { SkillSnapshot } from "@skill-studio/lib";
import { PageSection } from "../Shell/PageSection";
import { useAppStore } from "../../store/appStore";

const SHOWN = 8;

/** Uses per skill by `harness` in the last 30 days, most first; null when no history is readable. */
export function agentUses(snapshot: SkillSnapshot, harness: string) {
  if (!snapshot.invocations.length) return null;
  return snapshot.invocations
    .map((stats) => ({ skill: stats.skill, uses: stats.by_harness_30_days[harness] ?? 0 }))
    .filter((row) => row.uses > 0)
    .sort((a, b) => b.uses - a.uses || a.skill.localeCompare(b.skill));
}

export function AgentUses({ snapshot, harness }: { snapshot: SkillSnapshot; harness: string }) {
  const [all, setAll] = useState(false);
  const openSkill = useAppStore((s) => s.openSkill);
  const rows = agentUses(snapshot, harness);
  const installed = new Set(snapshot.skills.map((skill) => skill.name));
  const total = rows?.reduce((sum, row) => sum + row.uses, 0) ?? 0;
  const most = rows?.[0]?.uses ?? 0;
  const shown = all ? rows : rows?.slice(0, SHOWN);
  return (
    <PageSection
      title="Uses in 30 days"
      meta={rows?.length ? `${total.toLocaleString()} across ${rows.length} skills` : undefined}
    >
      {!rows ? (
        <p className="m-0 text-small text-text-secondary">No readable history.</p>
      ) : !rows.length ? (
        <p className="m-0 text-small text-text-secondary">No recorded uses in 30 days.</p>
      ) : (
        <>
          <ul aria-label="Uses by skill" className="m-0 flex list-none flex-col p-0">
            {shown?.map((row) => (
              <li
                key={row.skill}
                aria-label={`${row.skill}, ${row.uses} ${row.uses === 1 ? "use" : "uses"}`}
                className="grid min-h-8 grid-cols-[minmax(0,14rem)_minmax(0,1fr)_3rem] items-center gap-3"
              >
                {installed.has(row.skill) ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    className="-ml-2.5 max-w-[calc(100%+0.625rem)] justify-start truncate text-body font-normal"
                    title={row.skill}
                    onClick={() => openSkill(row.skill)}
                  >
                    {row.skill}
                  </Button>
                ) : (
                  <span className="truncate text-body" title={row.skill}>
                    {row.skill}
                  </span>
                )}
                <span aria-hidden className="h-1.5 rounded-full bg-bg-tertiary">
                  <span
                    className="block h-full rounded-full bg-data"
                    style={{ width: `${(row.uses / most) * 100}%` }}
                  />
                </span>
                <span className="text-right text-small text-text-secondary tabular-nums">
                  {row.uses.toLocaleString()}
                </span>
              </li>
            ))}
          </ul>
          {rows.length > SHOWN && (
            <Button
              size="sm"
              variant="ghost"
              className="mt-1 -ml-2.5"
              aria-expanded={all}
              onClick={() => setAll(!all)}
            >
              {all ? "Show fewer" : `Show all ${rows.length}`}
            </Button>
          )}
        </>
      )}
    </PageSection>
  );
}

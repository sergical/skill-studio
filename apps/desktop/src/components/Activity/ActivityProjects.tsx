// ============================================================================
// ActivityProjects - "Projects" section: every project with a use in the
// calendar's 30 days, under the page's agent and trigger filters. Left out
// when no use carries a project, since some agents don't record one.
// ============================================================================

import { basename, formatCount, projectRows } from "@skill-studio/lib";
import type { ActivityFilter, SkillInvocationStats } from "@skill-studio/lib";
import { PageSection } from "../Shell/PageSection";
import { TooltipControl } from "../ui/TooltipControl";
import { CountBar } from "./ActivityBySkill";
import { CappedList } from "./ActivityParts";

export function ActivityProjects({
  stats,
  filter,
  now,
}: {
  stats: SkillInvocationStats[];
  filter: ActivityFilter;
  now: Date;
}) {
  const rows = projectRows(stats, filter, now);
  if (rows.length === 0) return null;
  return (
    <PageSection title="Projects">
      <CappedList
        items={rows}
        render={({ project, count }) => (
          <TooltipControl key={project} content={[{ text: project, mono: true }]}>
            <div className="grid h-8 grid-cols-[minmax(0,1fr)_2.5rem_3rem] items-center gap-3">
              <span className="truncate text-body text-text-primary">{basename(project)}</span>
              <span className="text-right text-body text-text-secondary tabular-nums">
                {formatCount(count)}
              </span>
              <CountBar value={count} max={rows[0].count} />
            </div>
          </TooltipControl>
        )}
      />
    </PageSection>
  );
}

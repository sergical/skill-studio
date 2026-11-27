// ============================================================================
// ActivityDayDetails - One picked day in place of the 30-day skills list:
// its total against a usual day, the uses by hour, every skill, and every
// project. Arrows step to the day before or after; the close button goes
// back to the 30 days.
// ============================================================================

import type { ReactNode } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import {
  basename,
  dayDetail,
  deploymentLabelFromAgentId,
  formatCount,
  formatDay,
  ranked,
  shiftDay,
  usualDay,
  uses,
} from "@skill-studio/lib";
import type { ActivityFilter, AgentId, SkillInvocationStats } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { PageSection } from "../Shell/PageSection";
import { HarnessIcon } from "../ui/HarnessIcon";
import { TooltipControl } from "../ui/TooltipControl";
import { CountBar, manyAgents, SkillUseList } from "./ActivityBySkill";
import type { SkillUse } from "./ActivityBySkill";
import { CappedList, Muted } from "./ActivityParts";
import { HourStrip } from "./ActivityHourStrip";
import { PanelLabel } from "./ActivitySplits";

function compared(total: number, usual: number): string | null {
  if (total === 0 || usual === 0) return null;
  const ratio = total / usual;
  if (ratio >= 1.5) return `${Number(ratio.toFixed(1))}× a usual day`;
  if (ratio <= 0.6) return "quieter than a usual day";
  return "about a usual day";
}

function IconButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <TooltipControl content={label}>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={label}
        disabled={disabled}
        className="text-text-tertiary"
        onClick={onClick}
      >
        {children}
      </Button>
    </TooltipControl>
  );
}

interface ActivityDayDetailsProps {
  stats: SkillInvocationStats[];
  dayKey: string;
  filter: ActivityFilter;
  /** Per-day totals under the same filter, for the "usual day" comparison. */
  days: Record<string, number>;
  dates: string[];
  now: Date;
  onChangeDay: (dayKey: string | null) => void;
  onOpenSkill: (name: string) => void;
}

export function ActivityDayDetails({
  stats,
  dayKey,
  filter,
  days,
  dates,
  now,
  onChangeDay,
  onOpenSkill,
}: ActivityDayDetailsProps) {
  const day = dayDetail(stats, dayKey, filter);
  const prev = shiftDay(dates, dayKey, -1);
  const next = shiftDay(dates, dayKey, 1);
  const comparison = compared(day.total, usualDay(days));
  const rows: SkillUse[] = ranked(day.bySkill).map(([skill, count]) => {
    const harnesses = day.skillHarnesses[skill] ?? [];
    return {
      skill,
      count,
      harnesses,
      tooltip: [harnesses.map(deploymentLabelFromAgentId).join(" · ")],
    };
  });
  // SAFETY: harness ids come off the wire as plain strings; every one this page can show is a
  // known AgentId.
  const agents = ranked(day.byHarness) as [AgentId, number][];
  const projects = ranked(day.byProject);

  return (
    <PageSection
      title={formatDay(dayKey)}
      meta={
        <span className="-mr-1.5 flex items-center">
          <IconButton label="Previous day" disabled={!prev} onClick={() => onChangeDay(prev)}>
            <ChevronLeft size={14} aria-hidden="true" />
          </IconButton>
          <IconButton label="Next day" disabled={!next} onClick={() => onChangeDay(next)}>
            <ChevronRight size={14} aria-hidden="true" />
          </IconButton>
          <IconButton label="Back to 30 days" onClick={() => onChangeDay(null)}>
            <X size={14} aria-hidden="true" />
          </IconButton>
        </span>
      }
    >
      <div className="flex flex-col gap-6">
        <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-small text-text-secondary tabular-nums">
          <span>
            {day.total === 0
              ? "No uses"
              : `${uses(day.total)} · ${rows.length} ${rows.length === 1 ? "skill" : "skills"}`}
            {comparison && <span className="text-text-tertiary"> · {comparison}</span>}
          </span>
          {filter.harness === "all" && agents.length > 1 && (
            <span className="flex items-center gap-3">
              {agents.map(([id, n]) => (
                <span
                  key={id}
                  role="img"
                  className="inline-flex items-center gap-1.5"
                  aria-label={`${deploymentLabelFromAgentId(id)} ${formatCount(n)}`}
                >
                  <HarnessIcon harness={id} size={12} />
                  <Muted>{formatCount(n)}</Muted>
                </span>
              ))}
            </span>
          )}
        </p>

        {day.total > 0 && (
          <>
            <HourStrip byHour={day.byHour} />
            <div className="flex flex-col gap-1.5">
              <PanelLabel>Skills</PanelLabel>
              <SkillUseList
                rows={rows}
                now={now}
                showAgents={manyAgents(rows)}
                onOpenSkill={onOpenSkill}
              />
            </div>
            {projects.length > 0 && (
              <div className="flex flex-col gap-1.5">
                <PanelLabel>Projects</PanelLabel>
                <CappedList
                  items={projects}
                  moreClassName="-ml-2 px-2"
                  render={([project, n]) => (
                    <TooltipControl key={project} content={[{ text: project, mono: true }]}>
                      <div className="grid h-8 grid-cols-[minmax(0,1fr)_2.5rem_3rem] items-center gap-3">
                        <span className="truncate text-body text-text-primary">
                          {basename(project)}
                        </span>
                        <span className="text-right text-body text-text-secondary tabular-nums">
                          {formatCount(n)}
                        </span>
                        <CountBar value={n} max={projects[0][1]} />
                      </div>
                    </TooltipControl>
                  )}
                />
              </div>
            )}
          </>
        )}
      </div>
    </PageSection>
  );
}

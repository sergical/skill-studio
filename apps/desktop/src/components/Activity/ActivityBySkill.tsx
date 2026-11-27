// ============================================================================
// ActivityBySkill - The skills list for the calendar's 30 days, under the
// page's agent and trigger filters, and the row list it shares with a picked
// day: each row names the skill, the agents that used it, when it was last
// used, and its count drawn as a bar against the busiest skill.
// ============================================================================

import {
  deploymentLabelFromAgentId,
  formatCount,
  formatRelativeTime,
  ranked,
  skillRows,
  triggerLabel,
} from "@skill-studio/lib";
import type { ActivityFilter, AgentId, SkillInvocationStats, SkillRow } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { PageSection } from "../Shell/PageSection";
import { HarnessIcon } from "../ui/HarnessIcon";
import { TooltipControl } from "../ui/TooltipControl";
import { CappedList } from "./ActivityParts";

export interface SkillUse {
  skill: string;
  count: number;
  /** Agents behind the uses, busiest first. */
  harnesses: string[];
  /** Absent on a day's list, where the day already says when. */
  lastUsed?: string | null;
  tooltip: string[];
}

export function rowTooltip(row: Pick<SkillRow, "byHarness" | "byTrigger">): string[] {
  const harnesses = ranked(row.byHarness)
    .map(([id, n]) => `${deploymentLabelFromAgentId(id)} ${formatCount(n)}`)
    .join(" · ");
  const triggers = ranked(row.byTrigger)
    .map(([id, n]) => `${triggerLabel(id)} ${formatCount(n)}`)
    .join(" · ");
  return [harnesses, triggers];
}

/** True when the rows' uses come from two or more agents, so per-row agent marks tell them apart. */
export function manyAgents(rows: SkillUse[]): boolean {
  return new Set(rows.flatMap((row) => row.harnesses)).size > 1;
}

/** A row's count drawn against the list's largest, in the data color. */
export function CountBar({ value, max }: { value: number; max: number }) {
  return (
    <span className="h-1.5 overflow-hidden rounded-full bg-bg-tertiary" aria-hidden="true">
      <span
        className="block h-full rounded-full bg-data"
        style={{ width: `${max > 0 ? Math.max(4, (value / max) * 100) : 0}%` }}
      />
    </span>
  );
}

export function SkillUseList({
  rows,
  now,
  showAgents,
  onOpenSkill,
}: {
  rows: SkillUse[];
  now: Date;
  showAgents: boolean;
  onOpenSkill: (name: string) => void;
}) {
  const max = rows[0]?.count ?? 0;
  const withLastUsed = rows.some((row) => row.lastUsed !== undefined);
  const columns = [
    "minmax(0,1fr)",
    showAgents ? "auto" : null,
    withLastUsed ? "4.5rem" : null,
    "2.5rem",
    "3rem",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <CappedList
      items={rows}
      moreClassName="-ml-2 px-2"
      render={(row) => (
        <TooltipControl key={row.skill} content={row.tooltip}>
          <Button
            variant="ghost"
            className="-mx-2 grid h-8 justify-stretch gap-3 rounded-sm px-2 text-left font-normal"
            style={{ gridTemplateColumns: columns }}
            onClick={() => onOpenSkill(row.skill)}
          >
            <span className="truncate text-body text-text-primary">{row.skill}</span>
            {showAgents && (
              <span
                role="img"
                aria-label={row.harnesses.map(deploymentLabelFromAgentId).join(", ")}
                className="flex items-center justify-end gap-1"
              >
                {row.harnesses.map((id) => (
                  // SAFETY: harness ids come off the wire as plain strings; every one this page
                  // can show is a known AgentId.
                  <HarnessIcon key={id} harness={id as AgentId} size={12} muted />
                ))}
              </span>
            )}
            {withLastUsed && (
              <span className="text-right text-small text-nowrap text-text-tertiary tabular-nums">
                {row.lastUsed ? formatRelativeTime(row.lastUsed, now) : ""}
              </span>
            )}
            <span className="text-right text-body text-text-secondary tabular-nums">
              {formatCount(row.count)}
            </span>
            <CountBar value={row.count} max={max} />
          </Button>
        </TooltipControl>
      )}
    />
  );
}

export function ActivityBySkill({
  stats,
  filter,
  now,
  onOpenSkill,
}: {
  stats: SkillInvocationStats[];
  filter: ActivityFilter;
  now: Date;
  onOpenSkill: (name: string) => void;
}) {
  const rows: SkillUse[] = skillRows(stats, "30d", filter, now).map((row) => ({
    skill: row.skill,
    count: row.count,
    harnesses: ranked(row.byHarness).map(([id]) => id),
    lastUsed: row.lastUsed,
    tooltip: rowTooltip(row),
  }));
  const filtered = filter.harness !== "all" || filter.trigger !== "all";
  return (
    <PageSection
      title="Skills"
      meta={
        rows.length > 0 && `${formatCount(rows.length)} ${rows.length === 1 ? "skill" : "skills"}`
      }
    >
      {rows.length === 0 ? (
        <p className="text-body text-text-tertiary">
          {filtered ? "No uses with these filters." : "No uses in the last 30 days."}
        </p>
      ) : (
        <SkillUseList
          rows={rows}
          now={now}
          showAgents={manyAgents(rows)}
          onOpenSkill={onOpenSkill}
        />
      )}
    </PageSection>
  );
}

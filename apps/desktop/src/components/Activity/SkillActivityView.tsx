// ============================================================================
// SkillActivityView - Two columns: the 30-day calendar with the agents and
// triggers behind its uses on the left; on the right the skills and projects
// those uses went to, or one picked day's hours, skills and projects. The
// History event list sits below. One column when the page is narrow, with
// the skills or the day right under the calendar.
// ============================================================================

import { deploymentLabelFromAgentId, heatmapDateRangeLocal, statsInRange } from "@skill-studio/lib";
import type { SkillInvocationStats, SkillSnapshot } from "@skill-studio/lib";
import { FirstScanState, type FirstScanStateProps } from "../SkillList/FirstScanState";
import { ActivityBySkill } from "./ActivityBySkill";
import { ActivityDayDetails } from "./ActivityDayDetails";
import { ActivityFilters, useActivityLens } from "./ActivityFilters";
import { ActivityHistory, ActivitySources } from "./ActivityHistory";
import { ActivityProjects } from "./ActivityProjects";
import { SkillHistorySection } from "./SkillHistorySection";
import { useActivityEscape } from "./useActivityEscape";
import { joinNames } from "../Home/home-todos";
import { PageShell } from "../Shell/PageShell";
import { useAppStore } from "../../store/appStore";

interface SkillActivityViewProps extends FirstScanStateProps {
  snapshot: SkillSnapshot | undefined;
  usageRefreshing?: boolean;
  onSelectSkill: (name: string) => void;
}

const NO_STATS: SkillInvocationStats[] = [];

function activityNow(scannedAt: string | undefined) {
  return scannedAt ? new Date(scannedAt) : new Date();
}

/**
 * Activity history for own and plugin skills, from every agent with a usage reader that's turned
 * on in Settings. Every count on the page covers the calendar's 30 days, or the day picked on it.
 */
export function SkillActivityView({
  snapshot,
  onSelectSkill,
  isLoading,
  usageRefreshing = isLoading,
  error,
  onRetry,
}: SkillActivityViewProps) {
  const day = useAppStore((state) => state.activityDay);
  const setDay = useAppStore((state) => state.setActivityDay);
  useActivityEscape(() => setDay(null));

  // The snapshot's scan time, not the render time, so a stale snapshot doesn't make the window
  // and the heatmap disagree about "now".
  const now = activityNow(snapshot?.scanned_at);
  // Every section reads the calendar's 30 days only: not the start of the day before them, and
  // not a use dated after the scan.
  const stats = statsInRange(snapshot?.invocations ?? NO_STATS, heatmapDateRangeLocal(now));
  const lens = useActivityLens(stats, now);
  const hasAnyInvocations = stats.some((s) => s.by_hour.length > 0);

  if (!snapshot) {
    return (
      <PageShell title="Activity">
        <FirstScanState isLoading={isLoading} error={error} onRetry={onRetry} />
      </PageShell>
    );
  }

  if (!hasAnyInvocations) {
    return (
      <PageShell title="Activity" refreshing={usageRefreshing}>
        <div className="flex flex-col gap-8">
          {!usageRefreshing && (
            <p className="text-body text-pretty text-text-secondary">
              {lens.agents.length > 0
                ? `No readable skill history from ${joinNames(lens.agents.map(deploymentLabelFromAgentId))} in the last 30 days.`
                : "No readable skill history in the last 30 days."}
            </p>
          )}
          <SkillHistorySection />
        </div>
      </PageShell>
    );
  }

  return (
    <PageShell
      title="Activity"
      refreshing={usageRefreshing}
      actions={
        <ActivityFilters
          stats={stats}
          harness={lens.harness}
          setHarness={lens.setHarness}
          trigger={lens.trigger}
          setTrigger={lens.setTrigger}
          clearFilter={lens.clearFilter}
          agents={lens.agents}
        />
      }
    >
      <div
        className={`@container flex flex-col gap-8 ${usageRefreshing ? "opacity-50" : ""}`}
        aria-busy={usageRefreshing}
      >
        {/* One column: calendar, skills, then sources, so a picked day opens right under the
            calendar. Two columns: calendar over sources on the left; the skills or the day span
            both rows on the right, and the `1fr` row takes any extra height. */}
        <div className="grid grid-cols-1 gap-x-10 gap-y-8 @[52rem]:grid-cols-[minmax(0,26rem)_minmax(0,1fr)] @[52rem]:grid-rows-[auto_1fr]">
          <ActivityHistory stats={stats} lens={lens} selected={day} onSelect={setDay} />
          <div className="flex min-w-0 flex-col gap-8 @[52rem]:col-start-2 @[52rem]:row-span-2 @[52rem]:row-start-1">
            {day ? (
              <ActivityDayDetails
                stats={stats}
                dayKey={day}
                filter={lens.filter}
                days={lens.days}
                dates={lens.dates}
                now={now}
                onChangeDay={setDay}
                onOpenSkill={onSelectSkill}
              />
            ) : (
              <>
                <ActivityBySkill
                  stats={stats}
                  filter={lens.filter}
                  now={now}
                  onOpenSkill={onSelectSkill}
                />
                <ActivityProjects stats={stats} filter={lens.filter} now={now} />
              </>
            )}
          </div>
          <ActivitySources
            stats={stats}
            lens={lens}
            className="self-start @[52rem]:col-start-1 @[52rem]:row-start-2"
          />
        </div>
        <SkillHistorySection />
      </div>
    </PageShell>
  );
}

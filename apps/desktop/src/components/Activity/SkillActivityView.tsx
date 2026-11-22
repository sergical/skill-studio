import { FirstScanState, type FirstScanStateProps } from "../SkillList/FirstScanState";
// ============================================================================
// SkillActivityView - Docked layout: a 30-day heatmap, by-skill and by-project
// lists on the left; a right-hand panel that is always there, showing the
// usage window's overview until a day is picked, then that day's details.
// ============================================================================

import { heatmapDateRangeLocal, statsInRange } from "@skill-studio/lib";
import type { SkillInvocationStats, SkillSnapshot } from "@skill-studio/lib";
import { ActivityBySkill } from "./ActivityBySkill";
import { ActivityDayDetails } from "./ActivityDayDetails";
import { ActivityFilters, useActivityLens } from "./ActivityFilters";
import { ActivityOverview } from "./ActivityOverview";
import { ActivityProjects } from "./ActivityProjects";
import { ActivityHistory } from "./ActivityHistory";
import { SkillHistorySection } from "./SkillHistorySection";
import { useActivityEscape } from "./useActivityEscape";
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
 * Full activity history for own and plugin skills, from every harness with a
 * usage reader that's turned on in Settings: a 30-day heatmap, a per-skill
 * table filtered to the shared usage window, a 30-day per-project breakdown,
 * and a docked details panel for the day currently picked.
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
      <PageShell title="Activity" refreshing={usageRefreshing}>
        <FirstScanState isLoading={isLoading} error={error} onRetry={onRetry} />
      </PageShell>
    );
  }

  return (
    <PageShell
      title="Activity"
      refreshing={usageRefreshing}
      subtitle="From every agent turned on in Settings"
      actions={
        <ActivityFilters
          stats={stats}
          harness={lens.harness}
          setHarness={lens.setHarness}
          trigger={lens.trigger}
          setTrigger={lens.setTrigger}
          clearFilter={lens.clearFilter}
          enabledHarnesses={lens.enabledHarnesses}
        />
      }
    >
      {!hasAnyInvocations ? (
        <>
          {!usageRefreshing && (
            <p className="text-wrap-pretty text-body text-text-tertiary">
              No skill uses recorded yet.
            </p>
          )}
          <SkillHistorySection />
        </>
      ) : (
        <div
          className={`@container ${usageRefreshing ? "opacity-50" : ""}`}
          aria-busy={usageRefreshing}
        >
          {/* Narrow: one column, the details panel between the heatmap and the lists. Wide: the
              panel spans both rows of the right column, so its grid area (the sticky containing
              block) reaches the page bottom. The `1fr` row takes any extra height the panel
              needs, so the heatmap row never stretches. */}
          <div className="grid grid-cols-1 gap-x-6 gap-y-5 @[52rem]:grid-cols-[minmax(0,1fr)_18rem] @[52rem]:grid-rows-[auto_1fr]">
            <ActivityHistory
              stats={stats}
              dates={lens.dates}
              days={lens.days}
              historyTotal={lens.historyTotal}
              filter={lens.filter}
              selected={day}
              onSelect={setDay}
            />
            <aside
              aria-label="Details"
              // 7rem covers the window's 8px top and bottom insets, the page header row (the filters sit in it),
              // the sticky offset, and the page's bottom padding. Any taller and the grid's bottom
              // edge pushes the aside up past the sticky offset when scrolled to the end.
              className="flex flex-col gap-5 self-start rounded-md border border-border-subtle bg-bg-secondary p-4 @[52rem]:sticky @[52rem]:top-5 @[52rem]:col-start-2 @[52rem]:row-span-2 @[52rem]:row-start-1 @[52rem]:max-h-[calc(100dvh-7rem)] @[52rem]:overflow-y-auto"
            >
              {day ? (
                <ActivityDayDetails
                  stats={stats}
                  dayKey={day}
                  filter={lens.filter}
                  days={lens.days}
                  dates={lens.dates}
                  onChangeDay={setDay}
                  onOpenSkill={onSelectSkill}
                />
              ) : (
                <ActivityOverview stats={stats} filter={lens.filter} now={now} onOpenDay={setDay} />
              )}
            </aside>
            <div className="flex min-w-0 flex-col gap-5 @[52rem]:col-start-1 @[52rem]:row-start-2">
              <ActivityBySkill
                stats={stats}
                filter={lens.filter}
                now={now}
                onOpenSkill={onSelectSkill}
              />
              <ActivityProjects stats={stats} filter={lens.filter} now={now} />
              <SkillHistorySection />
            </div>
          </div>
        </div>
      )}
    </PageShell>
  );
}

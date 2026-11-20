// ============================================================================
// ActivityHistory - "Last 30 days" section: the calendar heatmap plus its
// total, wired to the shared day-card hover content.
// ============================================================================

import { uses } from "@skill-studio/lib";
import type { ActivityFilter, SkillInvocationStats } from "@skill-studio/lib";
import { ActivityDayCard } from "./ActivityDayCard";
import { ActivityHeatmap } from "./ActivityHeatmap";
import { Muted, SectionHeader } from "./ActivityParts";

export function ActivityHistory({
  stats,
  dates,
  days,
  historyTotal,
  filter,
  selected,
  onSelect,
}: {
  stats: SkillInvocationStats[];
  dates: string[];
  days: Record<string, number>;
  historyTotal: number;
  filter: ActivityFilter;
  selected: string | null;
  onSelect: (dayKey: string | null) => void;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-3">
      <SectionHeader title="Last 30 days">
        <Muted>{uses(historyTotal)}</Muted>
      </SectionHeader>
      <ActivityHeatmap
        dates={dates}
        days={days}
        label="Uses per day. Enter opens the day's details"
        cardWidth={248}
        selected={selected}
        onSelect={onSelect}
        renderCard={(key) => (
          <ActivityDayCard
            stats={stats}
            dayKey={key}
            filter={filter}
            hint={selected === key ? "Click to close" : "Click for all details"}
          />
        )}
      />
    </section>
  );
}

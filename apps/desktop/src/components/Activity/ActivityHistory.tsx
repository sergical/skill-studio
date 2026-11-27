// ============================================================================
// ActivityHistory - "Last 30 days" section: the calendar heatmap and its
// total. `ActivitySources` below it names which agents and triggers the uses
// came from; the page places the two apart, so one column can put the
// skills between them.
// ============================================================================

import { historyByHarness, historyByTrigger, uses } from "@skill-studio/lib";
import type { SkillInvocationStats } from "@skill-studio/lib";
import { PageSection } from "../Shell/PageSection";
import { ActivityDayCard } from "./ActivityDayCard";
import type { ActivityLens } from "./ActivityFilters";
import { ActivityHeatmap } from "./ActivityHeatmap";
import { AgentSplit, TriggerSplit } from "./ActivitySplits";

export function ActivityHistory({
  stats,
  lens,
  selected,
  onSelect,
}: {
  stats: SkillInvocationStats[];
  lens: ActivityLens;
  selected: string | null;
  onSelect: (dayKey: string | null) => void;
}) {
  return (
    <PageSection title="Last 30 days" meta={uses(lens.historyTotal)}>
      <ActivityHeatmap
        dates={lens.dates}
        days={lens.days}
        label="Uses per day. Enter opens the day's skills"
        cardWidth={248}
        selected={selected}
        onSelect={onSelect}
        renderCard={(key) => <ActivityDayCard stats={stats} dayKey={key} filter={lens.filter} />}
      />
    </PageSection>
  );
}

export function ActivitySources({
  stats,
  lens,
  className,
}: {
  stats: SkillInvocationStats[];
  lens: ActivityLens;
  className?: string;
}) {
  return (
    <div className={`flex flex-col gap-6 ${className ?? ""}`}>
      <AgentSplit
        agents={lens.agents}
        counts={historyByHarness(stats, lens.trigger)}
        readable={historyByHarness(stats, "all")}
        harness={lens.harness}
        setHarness={lens.setHarness}
      />
      <TriggerSplit
        counts={historyByTrigger(stats, lens.harness)}
        trigger={lens.trigger}
        setTrigger={lens.setTrigger}
      />
    </div>
  );
}

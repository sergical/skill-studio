import { useMemo, useState } from "react";
import type { KeyboardEvent } from "react";
import { ArrowRight } from "lucide-react";
import {
  ALL_ACTIVITY,
  dayTotals,
  formatCount,
  formatDay,
  HARNESS_CATALOG,
  heatmapDateRangeLocal,
  skillRows,
  statsInRange,
} from "@skill-studio/lib";
import type { SkillSnapshot } from "@skill-studio/lib";
import { Button, ToggleGroup, ToggleGroupItem } from "@skill-studio/ui";
import { singleSelectToggleValue } from "../../lib/single-select-toggle-group";
import { useAppStore } from "../../store/appStore";

type ActivityWindow = "7d" | "30d";

interface DashboardActivityProps {
  snapshot: SkillSnapshot;
  usageRefreshing?: boolean;
  open: (name: string) => void;
  clients: string[] | undefined;
}

interface Day {
  date: string;
  count: number;
}

function dayLabel(day: Day) {
  return `${formatDay(day.date)} · ${formatCount(day.count)} ${day.count === 1 ? "load" : "loads"}`;
}

function DailyLoads({ days, openDay }: { days: Day[]; openDay: (date: string) => void }) {
  const [focusedDate, setFocusedDate] = useState<string>();
  const [hovered, setHovered] = useState<string>();
  const [focusWithin, setFocusWithin] = useState(false);
  const active = days.find((day) => day.date === focusedDate) ?? days[days.length - 1];
  const readout = days.find((day) => day.date === hovered) ?? (focusWithin ? active : undefined);
  const peak = Math.max(...days.map((day) => day.count));

  function moveFocus(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    const positions = {
      ArrowLeft: Math.max(0, index - 1),
      ArrowRight: Math.min(days.length - 1, index + 1),
      Home: 0,
      End: days.length - 1,
    };
    if (!(event.key in positions)) return;
    // SAFETY: the key lookup above restricts keyboard navigation to the four supported keys.
    const next = positions[event.key as keyof typeof positions];
    event.preventDefault();
    event.currentTarget.parentElement?.querySelectorAll("button")[next]?.focus();
  }

  return (
    <figure className="mt-3.5">
      <div
        role="toolbar"
        onMouseLeave={() => setHovered(undefined)}
        onFocus={() => setFocusWithin(true)}
        onBlur={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget)) setFocusWithin(false);
        }}
        aria-label="Recorded loads by day, open day in Activity"
        className="flex h-16 items-end gap-px border-b border-border"
      >
        {days.map(({ date, count }, index) => {
          return (
            <Button
              key={date}
              variant="ghost"
              aria-label={dayLabel({ date, count })}
              onMouseEnter={() => setHovered(date)}
              tabIndex={date === active.date ? 0 : -1}
              className="group/day relative focus-visible:z-10 h-full min-w-0 flex-1 items-end rounded-xs border-0 p-0 hover:bg-bg-hover"
              onFocus={() => {
                setFocusedDate(date);
                setHovered(undefined);
              }}
              onKeyDown={(event) => moveFocus(event, index)}
              onClick={() => openDay(date)}
            >
              {count > 0 && (
                <span
                  aria-hidden="true"
                  className="w-4/5 max-w-9 rounded-t-xs bg-accent/75 group-hover/day:bg-accent group-focus-visible/day:bg-accent"
                  style={{ height: `max(2px, ${(count / peak) * 100}%)` }}
                />
              )}
            </Button>
          );
        })}
      </div>
      <figcaption className="mt-1.5 flex justify-between text-caption text-text-tertiary tabular-nums">
        <span>{formatDay(days[0].date)}</span>
        <span className="font-medium text-text-primary">{readout && dayLabel(readout)}</span>
        <span>{formatDay(days[days.length - 1].date)}</span>
      </figcaption>
    </figure>
  );
}

export function DashboardActivity({
  snapshot,
  usageRefreshing,
  open,
  clients,
}: DashboardActivityProps) {
  const [window, setWindow] = useState<ActivityWindow>("30d");
  const clientStats = useMemo(() => {
    // Match the other client-filtered surfaces while detection is unavailable.
    if (!clients) return snapshot.invocations;
    const enabled = new Set(clients);
    return snapshot.invocations.map((stat) => ({
      ...stat,
      by_hour: stat.by_hour.filter((hour) => enabled.has(hour.harness)),
    }));
  }, [snapshot.invocations, clients]);
  const installed = useMemo(
    () => new Set(snapshot.skills.map((skill) => skill.name)),
    [snapshot.skills],
  );
  const dayCount = window === "7d" ? 7 : 30;
  const { days, top, total } = useMemo(() => {
    const now = new Date(snapshot.scanned_at);
    const dates = heatmapDateRangeLocal(now).dates.slice(-dayCount);
    const stats = statsInRange(clientStats, { start: dates[0], end: dates[dates.length - 1] });
    const counts = dayTotals(stats, ALL_ACTIVITY);
    // The calendar already clipped the first day. Keep its whole local day across DST.
    const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    return {
      days: dates.map((date) => ({ date, count: counts[date] ?? 0 })),
      top: skillRows(stats, window, ALL_ACTIVITY, midnight).slice(0, 4),
      total: Object.values(counts).reduce((sum, count) => sum + count, 0),
    };
  }, [snapshot.scanned_at, clientStats, window, dayCount]);

  function openActivity(date: string | null) {
    const store = useAppStore.getState();
    store.setUsageWindow(window);
    store.setActivityFilter(
      clients?.length === 1 ? { harness: clients[0], trigger: "all" } : ALL_ACTIVITY,
    );
    store.setActivityDay(date);
    store.setActiveView({ kind: "activity" });
  }

  function status() {
    if (clients?.length === 0) return "Choose a client in Settings to see recorded loads.";
    if (clients && !HARNESS_CATALOG.some((row) => row.usageReader && clients.includes(row.id))) {
      return "The selected clients don’t report skill loads.";
    }
    return usageRefreshing
      ? "Reading session history…"
      : `No skill loads recorded in the last ${dayCount} days.`;
  }

  return (
    <section
      aria-label="Skill activity"
      className="@container rounded-lg border border-border bg-bg-secondary"
    >
      <header className="flex flex-wrap items-center justify-between gap-2 px-4 pt-3.5">
        <h2 className="text-heading font-semibold text-text-primary">Skill activity</h2>
        <div className="flex items-center gap-2">
          <ToggleGroup
            variant="segmented"
            aria-label="Activity window"
            value={[window]}
            onValueChange={(next) => singleSelectToggleValue<ActivityWindow>(next, setWindow)}
          >
            <ToggleGroupItem value="7d">7 days</ToggleGroupItem>
            <ToggleGroupItem value="30d">30 days</ToggleGroupItem>
          </ToggleGroup>
          <Button variant="ghost" size="sm" onClick={() => openActivity(null)}>
            View activity <ArrowRight aria-hidden="true" />
          </Button>
        </div>
      </header>
      {total > 0 ? (
        <div className="grid gap-4 px-4 pt-4 pb-4 @[34rem]:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] @[34rem]:gap-6">
          <div className="min-w-0">
            <div className="flex flex-wrap items-baseline gap-x-2">
              <strong className="text-[2rem] leading-none font-medium tracking-tight text-text-primary tabular-nums">
                {formatCount(total)}
              </strong>
              <span className="text-body text-text-secondary">
                recorded skill {total === 1 ? "load" : "loads"}
              </span>
              {usageRefreshing && (
                <span role="status" className="text-caption text-text-tertiary">
                  Updating…
                </span>
              )}
            </div>
            <DailyLoads days={days} openDay={openActivity} />
          </div>
          <div className="min-w-0 border-t border-border pt-3 @[34rem]:border-t-0 @[34rem]:border-l @[34rem]:pt-0 @[34rem]:pl-6">
            <h3 className="mb-1.5 text-caption font-medium text-text-secondary">
              Most used skills
            </h3>
            <ul>
              {top.map(({ skill, count }) => {
                const contents = (
                  <>
                    <span className="truncate font-mono text-caption" title={skill}>
                      {skill}
                    </span>
                    <span className="pl-2 text-caption text-text-secondary tabular-nums">
                      {formatCount(count)}
                    </span>
                  </>
                );
                return (
                  <li key={skill}>
                    {installed.has(skill) ? (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="-mx-1.5 flex w-[calc(100%+0.75rem)] justify-between px-1.5 font-normal"
                        onClick={() => open(skill)}
                      >
                        {contents}
                      </Button>
                    ) : (
                      <div className="flex h-7 items-center justify-between">{contents}</div>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        </div>
      ) : (
        <div
          role="status"
          className="flex min-h-32 items-center gap-2 px-4 pb-2 text-body text-text-secondary"
        >
          {status()}
          {clients?.length === 0 && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => useAppStore.getState().setActiveView({ kind: "settings" })}
            >
              Open Settings
            </Button>
          )}
        </div>
      )}
    </section>
  );
}

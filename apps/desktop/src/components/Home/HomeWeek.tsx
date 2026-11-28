// ============================================================================
// HomeWeek - Uses this week as one number and a bar per day, opening
// Activity; without readable history, says so and what fills it.
// ============================================================================

import { formatCount } from "@skill-studio/lib";
import { Button, cn } from "@skill-studio/ui";

function WeekBars({ values, labels }: { values: number[]; labels: string[] }) {
  const max = Math.max(1, ...values);
  return (
    <span className="flex flex-col gap-1" aria-hidden>
      <span className="flex h-8 items-end gap-1">
        {values.map((v, i) => (
          <span
            key={i}
            className={cn("w-3 rounded-[2px]", i === values.length - 1 ? "bg-data" : "bg-data/40")}
            style={{ height: `${v ? Math.max(12, (v / max) * 100) : 6}%` }}
          />
        ))}
      </span>
      <span className="flex gap-1 text-[10px] leading-none text-text-tertiary">
        {labels.map((label, i) => (
          <span
            key={i}
            className={cn("w-3 text-center", i === labels.length - 1 && "text-text-secondary")}
          >
            {label}
          </span>
        ))}
      </span>
    </span>
  );
}

const orList = (items: string[]) =>
  items.length > 1
    ? `${items.slice(0, -1).join(", ")} or ${items[items.length - 1]}`
    : items.join("");

interface HomeWeekProps {
  /** Null when no kept agent has readable history. */
  days: { total: number[]; labels: string[] } | null;
  agentLabels: string[];
  openActivity: () => void;
}

export function HomeWeek({ days, agentLabels, openActivity }: HomeWeekProps) {
  if (!days)
    return (
      <div className="max-w-xs">
        <p className="text-caption text-text-tertiary">Uses this week</p>
        <p className="mt-1 text-body">No readable history yet</p>
        <p className="mt-0.5 text-small text-text-secondary">
          Uses show here once {orList(agentLabels) || "an agent"} runs a skill.
        </p>
      </div>
    );
  const total = days.total.reduce((a, b) => a + b, 0);
  return (
    <Button
      variant="ghost"
      className="-mx-2 -my-1.5 h-auto items-end gap-4 px-2 py-1.5 font-normal"
      onClick={openActivity}
    >
      <span className="flex flex-col items-start">
        <span className="text-caption text-text-tertiary">Uses this week</span>
        <span className="text-display leading-none font-semibold tabular-nums">
          {formatCount(total)}
        </span>
      </span>
      <WeekBars values={days.total} labels={days.labels} />
      <span className="sr-only">
        {days.labels.map((label, i) => `${label} ${days.total[i]}`).join(", ")}. Open Activity
      </span>
    </Button>
  );
}

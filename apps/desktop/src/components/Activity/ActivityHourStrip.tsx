// ============================================================================
// ActivityHourStrip - A day's use counts by local hour, as a bar strip with
// its peak hour called out above and the hours marked below.
// ============================================================================

import { formatHour } from "@skill-studio/lib";
import { Muted } from "./ActivityParts";
import { PanelLabel } from "./ActivitySplits";

export function HourStrip({ byHour }: { byHour: number[] }) {
  const max = Math.max(...byHour);
  const peak = formatHour(byHour.indexOf(max));
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-3">
        <PanelLabel>Uses by hour</PanelLabel>
        <Muted>Most around {peak}</Muted>
      </div>
      <div
        role="img"
        aria-label={`Uses by hour, most around ${peak}`}
        className="flex h-8 items-end gap-px"
      >
        {byHour.map((n, hour) => (
          <span
            key={hour}
            className={`flex-1 rounded-[1px] ${n === 0 ? "bg-bg-tertiary" : "bg-data"}`}
            style={{ height: n === 0 ? 2 : `${Math.max(12, (n / max) * 100)}%` }}
          />
        ))}
      </div>
      <div className="grid grid-cols-4 text-caption text-text-tertiary" aria-hidden="true">
        <span>12 AM</span>
        <span>6 AM</span>
        <span>12 PM</span>
        <span>6 PM</span>
      </div>
    </div>
  );
}

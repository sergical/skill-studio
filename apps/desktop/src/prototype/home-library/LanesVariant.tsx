// PROTOTYPE (throwaway): "Lanes" - each agent's skill listing as one strip, in the order the
// agent lists skills. A block's width is the room its description takes; its fill rises with
// use. Past the line the agent keeps the name only. Hover or focus a block to read it.

import { useState } from "react";
import { deploymentLabelFromAgentId, formatCount } from "@skill-studio/lib";
import type { HarnessListing } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import {
  FocusStrip,
  Key,
  Legend,
  SkillCard,
  Swatch,
  UpdateDot,
  matchesFocus,
  useWidth,
} from "./shared";
import type { Focus } from "./shared";
import type { LibrarySkill } from "./library-model";

const LANE = 44;

function fill(state: string) {
  if (state === "described") return "bg-data";
  if (state === "truncated") return "bg-data/50";
  if (state === "omitted") return "bg-text-quaternary/50";
  return "bg-text-quaternary";
}

export function LanesVariant({
  skills,
  history,
  listings,
  open,
}: {
  skills: LibrarySkill[];
  history: boolean;
  listings: HarnessListing[];
  open: (name: string) => void;
}) {
  const { ref, width } = useWidth<HTMLDivElement>();
  const [focus, setFocus] = useState<Focus>("all");
  const [hover, setHover] = useState<{ item: LibrarySkill; x: number; y: number } | null>(null);
  const byName = new Map(skills.map((s) => [s.name, s]));
  // The card sits just below the block it describes, so it never covers that lane.
  const place = (block: HTMLElement, item: LibrarySkill) => {
    const box = ref.current?.getBoundingClientRect();
    const at = block.getBoundingClientRect();
    if (box) setHover({ item, x: at.left - box.left, y: at.bottom - box.top + 6 });
  };
  const maxUses = Math.max(1, ...skills.map((s) => s.uses));
  const lanes = listings.map((row) => {
    const entries = row.entries.filter((e) => e.state !== "hidden");
    const estimated = row.evidence === "preview" && row.budget !== null;
    const size = (chars: number, name: string) => (estimated ? chars + name.length + 4 : 1);
    const total = entries.reduce((t, e) => t + size(e.full_description_chars, e.name), 0) || 1;
    const cut = entries.findIndex((e) => e.state === "name_only" || e.state === "omitted");
    const full = entries.filter((e) => e.state === "described").length;
    return { row, entries, estimated, size, total, cut, full };
  });

  return (
    <section aria-label="What each agent lists" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FocusStrip skills={skills} history={history} focus={focus} setFocus={setFocus} />
        <Legend>
          <Key mark={<Swatch className="bg-data" />}>full</Key>
          <Key mark={<Swatch className="bg-data/50" />}>shortened</Key>
          <Key mark={<Swatch className="bg-text-quaternary" />}>name only</Key>
          <Key mark={<span className="inline-block h-3 w-0.5 bg-text-primary" />}>budget</Key>
          <Key mark={<span className="inline-block h-1 w-2.5 bg-error" />}>doesn’t load</Key>
          <Key mark={<UpdateDot />}>update</Key>
        </Legend>
      </div>
      <div ref={ref} className="relative flex flex-col gap-4" onPointerLeave={() => setHover(null)}>
        {width > 0 &&
          lanes.map(({ row, entries, estimated, size, total, cut, full }) => {
            const label = deploymentLabelFromAgentId(row.harness);
            const icon = harnessIdFromLabel(label);
            const lineAt =
              cut > 0
                ? entries
                    .slice(0, cut)
                    .reduce((t, e) => t + size(e.full_description_chars, e.name), 0) / total
                : null;
            return (
              <div key={row.harness}>
                <div className="mb-1.5 flex items-baseline justify-between gap-3 text-small">
                  <span className="inline-flex items-center gap-2 text-body font-medium">
                    {icon && <HarnessIcon harness={icon} size={14} />}
                    {label}
                  </span>
                  <span className="text-text-secondary tabular-nums">
                    {estimated
                      ? `${formatCount(full)} / ${formatCount(entries.length)} full · ${formatCount(row.budget ?? 0)} ${row.budget_unit === "characters" ? "chars" : row.budget_unit}`
                      : `${formatCount(entries.length)} skills · no budget`}
                  </span>
                </div>
                <div
                  className="relative flex gap-px overflow-hidden rounded-md"
                  style={{ height: LANE }}
                >
                  {entries.map((entry) => {
                    const item = byName.get(entry.name);
                    const w = (size(entry.full_description_chars, entry.name) / total) * width;
                    const heat = item?.uses ? Math.log1p(item.uses) / Math.log1p(maxUses) : 0;
                    const lit = item ? matchesFocus(item, focus) : focus === "all";
                    return (
                      <Button
                        key={entry.name + (entry.path ?? "")}
                        variant="ghost"
                        aria-label={`${entry.name}, ${entry.state.replace("_", " ")}`}
                        className="relative h-full shrink-0 rounded-none bg-bg-tertiary p-0 focus-visible:outline-2 focus-visible:outline-text-primary"
                        style={{ width: Math.max(1, w - 1), opacity: lit ? 1 : 0.15 }}
                        onPointerEnter={(e) => item && place(e.currentTarget, item)}
                        onFocus={(e) => item && place(e.currentTarget, item)}
                        onClick={() => item && open(item.name)}
                      >
                        <span
                          className={`absolute inset-x-0 bottom-0 ${fill(entry.state)}`}
                          style={{
                            height: `${estimated ? 30 + heat * 70 : 100}%`,
                            opacity: estimated ? 1 : 0.4,
                          }}
                        />
                        {item?.broken && <span className="absolute inset-x-0 top-0 h-1 bg-error" />}
                        {item?.update && (
                          <span className="absolute top-1 right-px size-1 rounded-full bg-text-primary" />
                        )}
                      </Button>
                    );
                  })}
                  {lineAt !== null && (
                    <span
                      aria-hidden
                      className="pointer-events-none absolute -top-1 -bottom-1 w-0.5 bg-text-primary"
                      style={{ left: `${lineAt * 100}%` }}
                    />
                  )}
                </div>
              </div>
            );
          })}
        {hover && <SkillCard item={hover.item} x={Math.min(hover.x, width - 264)} y={hover.y} />}
        {lanes.length === 0 && (
          <p className="text-small text-text-secondary">
            No agent has a skill listing to show yet.
          </p>
        )}
      </div>
    </section>
  );
}

// PROTOTYPE (throwaway): "Storage" - Home as a storage manager, the way the PlayStation and
// WhatsApp apps show a full disk: each agent's skill listing as one capacity bar, one
// suggestion that frees the most room, then every skill sorted by the room it takes, with
// checkboxes and one action for what you picked.

import { useState } from "react";
import { deploymentLabelFromAgentId, formatCount } from "@skill-studio/lib";
import type { HarnessListing, InstalledSkill } from "@skill-studio/lib";
import { Button, Checkbox } from "@skill-studio/ui";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import type { LibrarySkill } from "./library-model";

function Capacity({ row, unused }: { row: HarnessListing; unused: Set<string> }) {
  const label = deploymentLabelFromAgentId(row.harness);
  const icon = harnessIdFromLabel(label);
  // The budget is in characters for Claude Code and in tokens (about 4 characters) for Codex.
  const perUnit = row.budget_unit === "tokens" ? 4 : 1;
  const size = (e: HarnessListing["entries"][number]) =>
    (e.name.length + e.full_description_chars + 4) / perUnit;
  const entries = row.entries.filter((e) => e.state !== "hidden");
  // The core's full_size is the whole list in the budget's unit; split it by each skill's share.
  const usedRaw = entries.filter((e) => !unused.has(e.name)).reduce((t, e) => t + size(e), 0);
  const unusedRaw = entries.filter((e) => unused.has(e.name)).reduce((t, e) => t + size(e), 0);
  const share = usedRaw + unusedRaw || 1;
  const usedSize = Math.round((row.full_size * usedRaw) / share);
  const unusedSize = Math.round((row.full_size * unusedRaw) / share);
  const budget = row.budget ?? 0;
  const scale = Math.max(budget, usedSize + unusedSize, 1);
  const unit = row.budget_unit === "characters" ? "chars" : row.budget_unit;
  return (
    <div>
      <div className="mb-1.5 flex items-baseline justify-between gap-3 text-small">
        <span className="inline-flex items-center gap-2 font-medium">
          {icon && <HarnessIcon harness={icon} size={14} />}
          {label}
        </span>
        <span className="text-text-secondary tabular-nums">
          {budget === 0
            ? `${formatCount(entries.length)} skills · no limit Skill Studio can estimate`
            : `${formatCount(usedSize + unusedSize)} of ${formatCount(budget)} ${unit}`}
          {budget > 0 && usedSize + unusedSize > budget && (
            <span className="text-warning">
              {" "}
              · over by {formatCount(usedSize + unusedSize - budget)}
            </span>
          )}
        </span>
      </div>
      <div className="relative flex h-3 overflow-hidden rounded-full bg-bg-tertiary">
        <span className="h-full bg-data" style={{ width: `${(usedSize / scale) * 100}%` }} />
        <span
          className="h-full bg-text-quaternary"
          style={{ width: `${(unusedSize / scale) * 100}%` }}
        />
        {budget > 0 && budget < scale && (
          <span
            className="absolute inset-y-0 w-0.5 bg-text-primary"
            style={{ left: `${(budget / scale) * 100}%` }}
          />
        )}
      </div>
    </div>
  );
}

export function StorageVariant({
  skills,
  listings,
  open,
  park,
}: {
  skills: LibrarySkill[];
  listings: HarnessListing[];
  open: (name: string) => void;
  park: (skills: InstalledSkill[]) => void;
}) {
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [sort, setSort] = useState<"size" | "uses">("size");
  const unused = new Set(skills.filter((s) => !s.uses).map((s) => s.name));
  // Every agent gets a bar; one with no known limit shows its size against nothing.
  const estimated = listings;
  const rows = [...skills].sort((a, b) =>
    sort === "size" ? b.tokens - a.tokens : a.uses - b.uses || b.tokens - a.tokens,
  );
  const unusedTokens = skills.filter((s) => !s.uses).reduce((t, s) => t + s.tokens, 0);
  const pickedRows = skills.filter((s) => picked.has(s.name));
  const pickedTokens = pickedRows.reduce((t, s) => t + s.tokens, 0);
  const toggle = (name: string) =>
    setPicked((cur) => {
      const next = new Set(cur);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  const maxTokens = Math.max(1, ...skills.map((s) => s.tokens));

  return (
    <section aria-label="Skill listing space" className="flex flex-col gap-6">
      <div className="flex flex-col gap-4">
        {estimated.map((row) => (
          <Capacity key={row.harness} row={row} unused={unused} />
        ))}
        <ul aria-label="Legend" className="flex gap-4 text-caption text-text-secondary">
          <li className="inline-flex items-center gap-1.5">
            <span className="size-2.5 rounded-[2px] bg-data" />
            used in 30 days
          </li>
          <li className="inline-flex items-center gap-1.5">
            <span className="size-2.5 rounded-[2px] bg-text-quaternary" />
            not used
          </li>
          <li className="inline-flex items-center gap-1.5">
            <span className="h-3 w-0.5 bg-text-primary" />
            budget
          </li>
        </ul>
      </div>
      {unused.size > 0 && (
        <div className="flex items-center justify-between gap-4 rounded-lg border border-border-subtle p-4">
          <div>
            <p className="text-body font-semibold">
              Free about {formatCount(unusedTokens)} tokens in every message
            </p>
            <p className="text-small text-text-secondary">
              {formatCount(unused.size)} skills had no use in 30 days. Parked skills come back with
              Unpark.
            </p>
          </div>
          <Button variant="outline" onClick={() => setPicked(new Set(unused))}>
            Pick all {formatCount(unused.size)}
          </Button>
        </div>
      )}
      <div>
        <div className="mb-2 flex items-center justify-between text-small text-text-secondary">
          <span>{formatCount(skills.length)} skills</span>
          <div role="radiogroup" aria-label="Sort" className="flex">
            {(["size", "uses"] as const).map((s) => (
              <Button
                key={s}
                size="sm"
                variant={sort === s ? "outline" : "ghost"}
                role="radio"
                aria-checked={sort === s}
                onClick={() => setSort(s)}
              >
                {s === "size" ? "Largest first" : "Least used first"}
              </Button>
            ))}
          </div>
        </div>
        <ul className="max-h-[420px] overflow-y-auto border-t border-border-subtle">
          {rows.map((s) => (
            <li
              key={s.name}
              className="grid h-10 grid-cols-[auto_minmax(0,14rem)_minmax(4rem,1fr)_5rem_4rem] items-center gap-3 border-b border-border-subtle"
            >
              <Checkbox
                aria-label={`Pick ${s.name}`}
                checked={picked.has(s.name)}
                onCheckedChange={() => toggle(s.name)}
              />
              <Button
                variant="ghost"
                className="h-8 justify-start truncate px-0 font-normal"
                onClick={() => open(s.name)}
              >
                <span className={`truncate ${s.broken ? "text-error" : ""}`}>{s.name}</span>
              </Button>
              <span className="h-1.5 rounded-full bg-bg-tertiary">
                <span
                  className={`block h-full rounded-full ${s.uses ? "bg-data" : "bg-text-quaternary"}`}
                  style={{ width: `${(s.tokens / maxTokens) * 100}%` }}
                />
              </span>
              <span className="text-right text-small text-text-secondary tabular-nums">
                {formatCount(s.tokens)} tokens
              </span>
              <span className="text-right text-small text-text-secondary tabular-nums">
                {s.uses ? `${formatCount(s.uses)} uses` : "never"}
              </span>
            </li>
          ))}
        </ul>
      </div>
      {picked.size > 0 && (
        <div className="sticky bottom-16 flex items-center justify-between gap-4 rounded-lg border border-border bg-bg-elevated p-3 shadow-lg">
          <span className="text-small tabular-nums">
            {formatCount(picked.size)} picked · frees about {formatCount(pickedTokens)} tokens in
            every message
          </span>
          <span className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={() => setPicked(new Set())}>
              Clear
            </Button>
            <Button size="sm" onClick={() => park(pickedRows.map((s) => s.skill))}>
              Select {formatCount(picked.size)} to park
            </Button>
          </span>
        </div>
      )}
    </section>
  );
}

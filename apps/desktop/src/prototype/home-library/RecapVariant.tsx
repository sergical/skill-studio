// PROTOTYPE (throwaway): "Recap" - Home as a weekly report, the way Screen Time, Brick and Opal
// show phone use: one number and its change from last week, uses by day against the average,
// the most used skills with their own week, and what changed in the library this week.

import { useState } from "react";
import { HARNESS_CATALOG, deploymentLabelFromAgentId, formatCount } from "@skill-studio/lib";
import type { HarnessListing, SkillSnapshot } from "@skill-studio/lib";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { Button } from "@skill-studio/ui";
import { dailyUses } from "./library-model";
import type { LibrarySkill } from "./library-model";

const WEEK = 7;

function Bars({
  values,
  labels,
  average,
}: {
  values: number[];
  labels: string[];
  average: number;
}) {
  const max = Math.max(1, average, ...values);
  return (
    <div className="relative">
      <div className="flex h-40 items-end gap-2">
        {values.map((v, i) => (
          <div key={i} className="flex h-full flex-1 flex-col items-center justify-end gap-1.5">
            <span className="text-caption text-text-tertiary tabular-nums">
              {v ? formatCount(v) : ""}
            </span>
            <span
              className={`w-full max-w-10 rounded-t-[3px] ${i === values.length - 1 ? "bg-data" : "bg-data/45"}`}
              style={{ height: `${Math.max(v ? 4 : 1, (v / max) * 100)}%` }}
            />
          </div>
        ))}
      </div>
      {average > 0 && (
        <div
          className="pointer-events-none absolute inset-x-0 border-t border-dashed border-text-tertiary"
          style={{ bottom: `calc(${(average / max) * 100}% * 0.85 + 1.5rem)` }}
        >
          <span className="absolute -top-2.5 right-0 rounded bg-bg-tertiary px-1 text-caption text-text-secondary">
            avg {formatCount(Math.round(average))}
          </span>
        </div>
      )}
      <div className="mt-1.5 flex gap-2">
        {labels.map((l, i) => (
          <span key={i} className="flex-1 text-center text-caption text-text-tertiary">
            {l}
          </span>
        ))}
      </div>
    </div>
  );
}

function Spark({ values }: { values: number[] }) {
  const max = Math.max(1, ...values);
  return (
    <span className="inline-flex h-4 items-end gap-px" aria-hidden>
      {values.map((v, i) => (
        <span
          key={i}
          className={`w-1 rounded-[1px] ${v ? "bg-data" : "bg-bg-tertiary"}`}
          style={{ height: `${Math.max(12, (v / max) * 100)}%` }}
        />
      ))}
    </span>
  );
}

/** One agent's tile: its uses this week, how much of its skill listing has full descriptions,
 * and its problems. Pressing it narrows the recap to that agent. */
function AgentTile({
  agent,
  uses,
  listing,
  broken,
  pressed,
  onPress,
}: {
  agent: string;
  uses: number | null;
  listing: HarnessListing | undefined;
  broken: number;
  pressed: boolean;
  onPress: () => void;
}) {
  const label = deploymentLabelFromAgentId(agent);
  const icon = harnessIdFromLabel(label);
  const entries = (listing?.entries ?? []).filter((e) => e.state !== "hidden");
  const count = (...states: string[]) => entries.filter((e) => states.includes(e.state)).length;
  const full = count("described");
  const cut = count("truncated", "name_only", "omitted");
  const estimated = listing?.evidence === "preview" && listing.budget !== null;
  return (
    <Button
      variant="ghost"
      aria-pressed={pressed}
      onClick={onPress}
      className={`flex h-auto min-w-0 flex-col items-stretch gap-2 rounded-lg border p-3 text-left font-normal ${
        pressed ? "border-text-tertiary bg-bg-tertiary" : "border-border-subtle"
      }`}
    >
      <span className="flex min-w-0 items-center gap-2 text-small font-medium">
        {icon && <HarnessIcon harness={icon} size={14} />}
        <span className="truncate">{label}</span>
      </span>
      <span className="text-heading font-semibold tabular-nums">
        {uses === null ? "–" : formatCount(uses)}
        <span className="ml-1 text-caption font-normal text-text-tertiary">
          {uses === null ? "no readable history" : "uses"}
        </span>
      </span>
      {estimated ? (
        <span className="flex h-1.5 overflow-hidden rounded-full bg-bg-tertiary" aria-hidden>
          <span className="bg-data" style={{ flex: full }} />
          <span className="bg-warning" style={{ flex: cut }} />
        </span>
      ) : (
        <span className="h-1.5 rounded-full bg-bg-tertiary" aria-hidden />
      )}
      <span className="truncate text-caption text-text-secondary tabular-nums">
        {estimated
          ? `${formatCount(full)} full · ${formatCount(cut)} cut`
          : `${formatCount(entries.length)} skills · no limit`}
        {broken ? <span className="text-error"> · {formatCount(broken)} broken</span> : null}
      </span>
    </Button>
  );
}

export function RecapVariant({
  snapshot,
  skills,
  agents,
  listings,
  open,
}: {
  snapshot: SkillSnapshot;
  skills: LibrarySkill[];
  agents: string[] | undefined;
  listings: HarnessListing[];
  open: (name: string) => void;
}) {
  const [week, setWeek] = useState<"this" | "last">("this");
  const [only, setOnly] = useState<string | null>(null);
  const shownAgents = agents ?? listings.map((r) => r.harness);
  const readers = new Set<string>(
    HARNESS_CATALOG.filter((row) => row.usageReader).map((row) => row.id),
  );
  const everyAgent = dailyUses(snapshot, agents, WEEK * 2);
  const days = only ? dailyUses(snapshot, [only], WEEK * 2) : everyAgent;
  const slice = (values: number[]) =>
    week === "this" ? values.slice(WEEK) : values.slice(0, WEEK);
  const shown = slice(days.total);
  const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);
  const thisWeek = sum(days.total.slice(WEEK));
  const lastWeek = sum(days.total.slice(0, WEEK));
  const total = week === "this" ? thisWeek : lastWeek;
  const change = lastWeek ? Math.round(((thisWeek - lastWeek) / lastWeek) * 100) : null;
  const top = [...days.bySkill.entries()]
    .map(([name, values]) => ({ name, values: slice(values), count: sum(slice(values)) }))
    .filter((row) => row.count > 0)
    .sort((a, b) => b.count - a.count)
    .slice(0, 6);
  const weekStart = Date.parse(snapshot.scanned_at) - WEEK * 86_400_000;
  const added = skills.filter((s) => Date.parse(s.skill.installed_at) >= weekStart);
  const changes = [
    { label: "added", items: added, tone: "bg-data" },
    { label: "don’t load", items: skills.filter((s) => s.broken), tone: "bg-error" },
    { label: "cut from a skill listing", items: skills.filter((s) => s.cut), tone: "bg-warning" },
    { label: "have updates", items: skills.filter((s) => s.update), tone: "bg-text-primary" },
  ].filter((c) => c.items.length);
  const used = new Set(top.map((t) => t.name));
  // Weeks of history Skill Studio has read, so a change is only shown against a whole week.
  const earliest = Math.min(
    ...snapshot.invocations.flatMap((stats) => stats.by_hour.map((h) => h.hour)),
  );
  const historyWeeks = Number.isFinite(earliest)
    ? (Date.parse(snapshot.scanned_at) / 3_600_000 - earliest) / (24 * WEEK)
    : 0;
  const skillsUsed = [...days.bySkill.values()].filter((v) => sum(v.slice(WEEK))).length;

  return (
    <section aria-label="This week" className="flex flex-col gap-6">
      <div
        role="group"
        aria-label="Agents"
        className="grid gap-2"
        style={{ gridTemplateColumns: "repeat(auto-fill, minmax(11rem, 1fr))" }}
      >
        {shownAgents.map((agent) => {
          const label = deploymentLabelFromAgentId(agent);
          return (
            <AgentTile
              key={agent}
              agent={agent}
              uses={
                readers.has(agent) ? sum((everyAgent.byAgent.get(agent) ?? []).slice(WEEK)) : null
              }
              listing={listings.find((r) => r.harness === agent)}
              broken={
                skills.filter(
                  (s) =>
                    s.broken &&
                    s.skill.deployments.some((d) => d.agent === label || d.agent === "shared"),
                ).length
              }
              pressed={only === agent}
              onPress={() => setOnly(only === agent ? null : agent)}
            />
          );
        })}
      </div>
      <div className="grid gap-8 md:grid-cols-[minmax(0,1fr)_18rem]">
        <div className="flex flex-col gap-6">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-small text-text-secondary">
                Skill uses{only ? ` · ${deploymentLabelFromAgentId(only)}` : ""}
              </p>
              <p className="text-[2rem] leading-tight font-semibold tabular-nums">
                {formatCount(total)}
              </p>
              {week === "this" && (
                <p className="text-small text-text-secondary tabular-nums">
                  {change !== null && historyWeeks >= 2
                    ? `${change >= 0 ? "↑" : "↓"} ${Math.abs(change)}% from last week · `
                    : ""}
                  {formatCount(skillsUsed)} skills used
                </p>
              )}
            </div>
            {historyWeeks >= 2 && (
              <div
                role="radiogroup"
                aria-label="Week"
                className="flex rounded-md border border-border-subtle p-0.5"
              >
                {(["last", "this"] as const).map((w) => (
                  <Button
                    key={w}
                    size="sm"
                    variant={week === w ? "outline" : "ghost"}
                    role="radio"
                    aria-checked={week === w}
                    onClick={() => setWeek(w)}
                  >
                    {w === "this" ? "This week" : "Last week"}
                  </Button>
                ))}
              </div>
            )}
          </div>
          <Bars values={shown} labels={days.labels.slice(WEEK)} average={total / WEEK} />
          <div>
            <p className="mb-2 text-small font-medium text-text-secondary">Most used</p>
            <ul className="-mx-2 border-t border-border-subtle">
              {top.map((row) => (
                <li key={row.name}>
                  <Button
                    variant="ghost"
                    className="grid h-10 w-full grid-cols-[minmax(0,12rem)_minmax(4rem,1fr)_auto_3rem] items-center gap-4 rounded-none border-0 border-b border-border-subtle px-2 font-normal"
                    onClick={() => open(row.name)}
                  >
                    <span className="truncate text-left">{row.name}</span>
                    <span className="h-1.5 rounded-full bg-bg-tertiary">
                      <span
                        className="block h-full rounded-full bg-data"
                        style={{ width: `${(row.count / top[0].count) * 100}%` }}
                      />
                    </span>
                    <Spark values={row.values} />
                    <span className="text-right text-small text-text-secondary tabular-nums">
                      {formatCount(row.count)}
                    </span>
                  </Button>
                </li>
              ))}
              {!top.length && (
                <li className="py-3 text-small text-text-secondary">No skill use this week.</li>
              )}
            </ul>
          </div>
        </div>
        <aside className="flex flex-col gap-3">
          <p className="text-small font-medium text-text-secondary">Your library</p>
          <p className="text-small text-text-secondary">
            <span className="text-heading font-semibold text-text-primary tabular-nums">
              {formatCount(skills.length)}
            </span>{" "}
            skills · {formatCount(skills.filter((s) => !s.uses && !used.has(s.name)).length)} not
            used in 30 days
          </p>
          <ul className="flex flex-col gap-1">
            {changes.map((c) => (
              <li key={c.label} className="rounded-md border border-border-subtle p-3">
                <p className="flex items-center gap-2 text-small">
                  <span className={`size-2 rounded-full ${c.tone}`} aria-hidden />
                  <strong className="tabular-nums">{formatCount(c.items.length)}</strong> {c.label}
                </p>
                <p className="mt-1 truncate text-caption text-text-secondary">
                  {c.items
                    .slice(0, 3)
                    .map((s) => s.name)
                    .join(", ")}
                  {c.items.length > 3 ? ` +${c.items.length - 3}` : ""}
                </p>
              </li>
            ))}
          </ul>
        </aside>
      </div>
    </section>
  );
}

// PROTOTYPE (throwaway): "Recap" - Home as a weekly report, the way Screen Time, Brick and Opal
// show phone use. One tile per agent (its week, its skill listing, its broken copies; press it
// to narrow the report to that agent), then uses by day against the average, the most used
// skills with their own week, and the library's open items, each opening the list behind it.

import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { HARNESS_CATALOG, deploymentLabelFromAgentId, formatCount } from "@skill-studio/lib";
import type { HarnessListing, SkillListFilter, SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { useAppStore } from "../../store/appStore";
import { dailyUses } from "./library-model";
import type { LibrarySkill } from "./library-model";

const WEEK = 7;
const CHART = 132;
const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

/** Uses per day: one bar per day, today in full color, a dashed line at the week's average. */
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
  const y = (v: number) => (v / max) * CHART;
  return (
    <figure aria-label="Uses per day" className="m-0">
      {average > 0 && (
        <figcaption className="flex justify-end text-caption text-text-tertiary tabular-nums">
          <span className="inline-flex items-center gap-1.5">
            <span className="w-3 border-t border-dashed border-text-quaternary" aria-hidden />
            avg {formatCount(Math.round(average))} a day
          </span>
        </figcaption>
      )}
      <div className="relative flex items-end gap-2 pt-5" style={{ height: CHART + 20 }}>
        {average > 0 && (
          <div
            aria-hidden
            className="pointer-events-none absolute inset-x-0 border-t border-dashed border-text-quaternary"
            style={{ bottom: y(average) }}
          ></div>
        )}
        {values.map((v, i) => (
          <div key={i} className="relative flex flex-1 justify-center" style={{ height: CHART }}>
            <span
              className={`absolute bottom-0 w-full max-w-11 rounded-t-[3px] ${
                i === values.length - 1 ? "bg-data" : "bg-data/40"
              }`}
              style={{ height: Math.max(v ? 3 : 1, y(v)) }}
            />
            {v > 0 && (
              <span
                className="absolute text-caption text-text-secondary tabular-nums"
                style={{ bottom: y(v) + 4 }}
              >
                {formatCount(v)}
              </span>
            )}
          </div>
        ))}
      </div>
      <div className="mt-2 flex gap-2 border-t border-border-subtle pt-1.5">
        {labels.map((l, i) => (
          <span
            key={i}
            className={`flex-1 text-center text-caption ${
              i === labels.length - 1 ? "font-medium text-text-primary" : "text-text-tertiary"
            }`}
          >
            {i === labels.length - 1 ? "Today" : l}
          </span>
        ))}
      </div>
    </figure>
  );
}

/** A skill's week as seven small bars, scaled to its own busiest day. */
function Spark({ values }: { values: number[] }) {
  const max = Math.max(1, ...values);
  return (
    <span className="inline-flex h-4 items-end gap-px" aria-hidden>
      {values.map((v, i) => (
        <span
          key={i}
          className={`w-1.5 rounded-[1px] ${v ? "bg-data" : "bg-bg-tertiary"}`}
          style={{ height: `${v ? Math.max(20, (v / max) * 100) : 12}%` }}
        />
      ))}
    </span>
  );
}

interface TileFacts {
  agent: string;
  uses: number | null;
  week: number[];
  listing: HarnessListing | undefined;
  broken: number;
}

/** One agent's week, skill listing and broken copies. Pressing it narrows the report to it. */
function AgentTile({
  facts,
  pressed,
  onPress,
}: {
  facts: TileFacts;
  pressed: boolean;
  onPress: () => void;
}) {
  const label = deploymentLabelFromAgentId(facts.agent);
  const icon = harnessIdFromLabel(label);
  const entries = (facts.listing?.entries ?? []).filter((e) => e.state !== "hidden");
  const count = (...states: string[]) => entries.filter((e) => states.includes(e.state)).length;
  const full = count("described");
  const shortened = count("truncated");
  const names = count("name_only", "omitted");
  const estimated = facts.listing?.evidence === "preview" && facts.listing.budget !== null;
  const listingLine = !facts.listing
    ? "Skill list not read yet"
    : estimated
      ? [
          `${formatCount(full)} full`,
          shortened ? `${formatCount(shortened)} shortened` : "",
          names ? `${formatCount(names)} name only` : "",
        ]
          .filter(Boolean)
          .join(" · ")
      : `${formatCount(entries.length)} found · no estimate`;
  return (
    <Button
      variant="ghost"
      aria-pressed={pressed}
      title={`${label}: ${listingLine}`}
      onClick={onPress}
      className={`flex h-auto min-w-0 flex-col items-stretch gap-2.5 rounded-lg border p-3 text-left font-normal ${
        pressed ? "border-text-tertiary bg-bg-tertiary" : "border-border-subtle"
      }`}
    >
      <span className="flex min-w-0 items-center justify-between gap-2 text-small font-medium">
        <span className="flex min-w-0 items-center gap-2">
          {icon && <HarnessIcon harness={icon} size={14} />}
          <span className="truncate">{label}</span>
        </span>
        {facts.broken > 0 && (
          <span className="shrink-0 rounded-full bg-error/15 px-1.5 text-caption text-error tabular-nums">
            {formatCount(facts.broken)} broken
          </span>
        )}
      </span>
      <span className="flex items-end justify-between gap-2">
        <span className="min-w-0">
          <span className="block text-heading font-semibold tabular-nums">
            {facts.uses === null ? "–" : formatCount(facts.uses)}
          </span>
          <span className="block truncate text-caption text-text-tertiary">
            {facts.uses === null ? "No readable history" : "uses"}
          </span>
        </span>
        {facts.uses !== null && <Spark values={facts.week} />}
      </span>
      <span className="flex h-1 overflow-hidden rounded-full bg-bg-tertiary" aria-hidden>
        {estimated && (
          <>
            <span className="bg-data" style={{ flex: full }} />
            <span className="bg-data/45" style={{ flex: shortened }} />
            <span className="bg-warning" style={{ flex: names }} />
          </>
        )}
      </span>
      <span className="truncate text-caption text-text-secondary tabular-nums">{listingLine}</span>
    </Button>
  );
}

interface LibraryItem {
  id: string;
  tone: string;
  label: string;
  count: number;
  names: string[];
  go: () => void;
}

export function RecapVariant({
  snapshot,
  skills,
  agents,
  listings,
  counts,
  open,
}: {
  snapshot: SkillSnapshot;
  skills: LibrarySkill[];
  agents: string[] | undefined;
  listings: HarnessListing[];
  /** Skills, not issue rows, in each of Home's groups, so the numbers match the list below. */
  counts: { broken: string[]; warnings: string[]; updates: string[] };
  open: (name: string) => void;
}) {
  const [week, setWeek] = useState<"this" | "last">("this");
  const [only, setOnly] = useState<string | null>(null);
  const replaceSkillListFilter = useAppStore((s) => s.replaceSkillListFilter);
  const setActiveView = useAppStore((s) => s.setActiveView);
  const goToSkills = (patch: Partial<SkillListFilter>) => {
    replaceSkillListFilter(patch);
    setActiveView({ kind: "skills" });
  };
  const shownAgents = agents ?? listings.map((r) => r.harness);
  const readers = new Set<string>(HARNESS_CATALOG.filter((r) => r.usageReader).map((r) => r.id));
  const hasHistory = snapshot.invocations.some((stats) => stats.by_hour.length > 0);
  const everyAgent = dailyUses(snapshot, agents, WEEK * 2);
  const days = only ? dailyUses(snapshot, [only], WEEK * 2) : everyAgent;
  const pick = <T,>(values: T[]) => (week === "this" ? values.slice(WEEK) : values.slice(0, WEEK));
  const thisWeek = sum(days.total.slice(WEEK));
  const lastWeek = sum(days.total.slice(0, WEEK));
  const total = week === "this" ? thisWeek : lastWeek;
  const earliest = Math.min(...snapshot.invocations.flatMap((s) => s.by_hour.map((h) => h.hour)));
  const historyDays = Number.isFinite(earliest)
    ? (Date.parse(snapshot.scanned_at) / 3_600_000 - earliest) / 24
    : 0;
  // A change is only worth showing against a whole week that Skill Studio could read.
  const change =
    historyDays >= WEEK * 2 && lastWeek > 0
      ? Math.round(((thisWeek - lastWeek) / lastWeek) * 100)
      : null;
  const top = [...days.bySkill.entries()]
    .map(([name, values]) => ({ name, values: pick(values), count: sum(pick(values)) }))
    .filter((row) => row.count > 0)
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  const weekStart = Date.parse(snapshot.scanned_at) - WEEK * 86_400_000;
  const items: LibraryItem[] = [
    {
      id: "broken",
      tone: "bg-error",
      label: "Broken",
      count: counts.broken.length,
      names: counts.broken,
      go: () => goToSkills({ attention: "broken" }),
    },
    {
      id: "warnings",
      tone: "bg-warning",
      label: "Need a look",
      count: counts.warnings.length,
      names: counts.warnings,
      go: () => goToSkills({ attention: "warning" }),
    },
    {
      id: "updates",
      tone: "bg-text-primary",
      label: "Updates",
      count: counts.updates.length,
      names: counts.updates,
      go: () => goToSkills({ attention: "update" }),
    },
    {
      id: "unused",
      tone: "bg-text-quaternary",
      label: "Not used in 30 days",
      count: hasHistory ? skills.filter((s) => !s.uses).length : 0,
      names: skills.filter((s) => !s.uses).map((s) => s.name),
      go: () => goToSkills({ usage: "unused-30d" }),
    },
    {
      id: "added",
      tone: "bg-data",
      label: "Added this week",
      count: skills.filter((s) => Date.parse(s.skill.installed_at) >= weekStart).length,
      names: skills.filter((s) => Date.parse(s.skill.installed_at) >= weekStart).map((s) => s.name),
      go: () => goToSkills({}),
    },
  ].filter((item) => item.count > 0);
  const scope = only ? deploymentLabelFromAgentId(only) : "All agents";

  return (
    <section aria-label="This week" className="flex flex-col gap-8">
      <div
        role="group"
        aria-label="Agents"
        className="grid gap-2"
        style={{ gridTemplateColumns: "repeat(auto-fill, minmax(11.5rem, 1fr))" }}
      >
        {shownAgents.map((agent) => {
          const label = deploymentLabelFromAgentId(agent);
          const series = everyAgent.byAgent.get(agent) ?? Array.from({ length: WEEK * 2 }, () => 0);
          return (
            <AgentTile
              key={agent}
              facts={{
                agent,
                uses: readers.has(agent) && hasHistory ? sum(pick(series)) : null,
                week: pick(series),
                listing: listings.find((r) => r.harness === agent),
                // The copy an agent reads is its own folder's, else the shared one.
                broken: skills.filter((s) => {
                  const own = s.skill.deployments.filter((d) => d.agent === label);
                  const read = own.length
                    ? own
                    : s.skill.deployments.filter((d) => d.agent === "shared");
                  return read.some((d) => d.symlink_is_broken);
                }).length,
              }}
              pressed={only === agent}
              onPress={() => setOnly(only === agent ? null : agent)}
            />
          );
        })}
      </div>

      <div className="grid gap-x-10 gap-y-8 md:grid-cols-[minmax(0,1fr)_17rem]">
        <div className="flex min-w-0 flex-col gap-6">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="text-small text-text-secondary">Skill uses · {scope}</p>
              <p className="text-[2rem] leading-tight font-semibold tabular-nums">
                {hasHistory ? formatCount(total) : "–"}
              </p>
              <p className="text-small text-text-secondary tabular-nums">
                {!hasHistory
                  ? "No readable skill history yet"
                  : [
                      week === "this" && change !== null
                        ? `${change >= 0 ? "↑" : "↓"} ${Math.abs(change)}% from last week`
                        : "",
                      `${formatCount(top.length)} ${top.length === 1 ? "skill" : "skills"} used`,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
              </p>
            </div>
            {lastWeek > 0 && (
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

          {hasHistory ? (
            <Bars values={pick(days.total)} labels={pick(days.labels)} average={total / WEEK} />
          ) : (
            <p className="rounded-lg border border-dashed border-border-subtle px-4 py-6 text-small text-text-secondary">
              Uses appear here once {joinAgents(shownAgents.filter((a) => readers.has(a)))} records
              a skill in its history.
            </p>
          )}

          {hasHistory && (
            <div>
              <div className="grid grid-cols-[minmax(0,13rem)_minmax(3rem,1fr)_auto_2.5rem] gap-4 pb-1.5 text-caption text-text-tertiary">
                <span>Most used</span>
                <span />
                <span>by day</span>
                <span className="text-right">uses</span>
              </div>
              <ul className="border-t border-border-subtle">
                {top.slice(0, 6).map((row) => (
                  <li key={row.name}>
                    <Button
                      variant="ghost"
                      title={row.name}
                      className="-mx-2 grid h-10 w-[calc(100%+1rem)] grid-cols-[minmax(0,13rem)_minmax(3rem,1fr)_auto_2.5rem] items-center gap-4 rounded-none border-0 border-b border-border-subtle px-2 font-normal"
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
                  <li className="py-3 text-small text-text-secondary">
                    No skill use {week === "this" ? "this week" : "last week"}
                    {only ? ` in ${scope}` : ""}.
                  </li>
                )}
              </ul>
            </div>
          )}
        </div>

        <aside aria-label="Your library" className="flex min-w-0 flex-col gap-3">
          <p className="text-small text-text-secondary">Your library</p>
          <p className="-mt-2 text-[2rem] leading-tight font-semibold tabular-nums">
            {formatCount(skills.length)}
            <span className="ml-1.5 text-small font-normal text-text-secondary">skills</span>
          </p>
          {items.length ? (
            <ul className="border-t border-border-subtle">
              {items.map((item) => (
                <li key={item.id}>
                  <Button
                    variant="ghost"
                    title={item.names.slice(0, 12).join(", ")}
                    className="-mx-2 grid h-auto w-[calc(100%+1rem)] grid-cols-[0.5rem_minmax(0,1fr)_auto_0.875rem] items-center gap-x-2.5 rounded-none border-0 border-b border-border-subtle px-2 py-2 text-left font-normal"
                    onClick={item.go}
                  >
                    <span className={`size-2 rounded-full ${item.tone}`} aria-hidden />
                    <span className="min-w-0">
                      <span className="block text-small">{item.label}</span>
                      <span className="block truncate text-caption text-text-tertiary">
                        {item.names.slice(0, 3).join(", ")}
                      </span>
                    </span>
                    <span className="text-small font-semibold tabular-nums">
                      {formatCount(item.count)}
                    </span>
                    <ChevronRight size={14} className="text-text-tertiary" aria-hidden />
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-small text-text-secondary">
              Nothing broken, waiting or out of date.
            </p>
          )}
        </aside>
      </div>
    </section>
  );
}

function joinAgents(ids: string[]): string {
  const names = ids.map(deploymentLabelFromAgentId);
  if (names.length === 0) return "an agent";
  if (names.length === 1) return names[0];
  return `${names.slice(0, -1).join(", ")} or ${names[names.length - 1]}`;
}

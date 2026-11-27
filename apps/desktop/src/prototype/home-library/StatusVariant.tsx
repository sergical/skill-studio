// PROTOTYPE (throwaway): "Status" - Home as a status page, the way OpenAI and Stripe show
// service health: one headline, then one row per agent with its state and a 30-day strip of
// use like an uptime bar, opening to the skills behind its problems; history on the side.

import { useState } from "react";
import { deploymentLabelFromAgentId, formatCount, formatRelativeTime } from "@skill-studio/lib";
import type { HarnessListing, SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { ChevronRight } from "lucide-react";
import { HarnessIcon, harnessIdFromLabel } from "../../components/ui/HarnessIcon";
import { dailyUses } from "./library-model";
import type { LibrarySkill } from "./library-model";

const DAYS = 30;

function Strip({ values, start }: { values: number[]; start: number }) {
  const max = Math.max(1, ...values);
  return (
    <span className="flex h-6 items-stretch gap-px" aria-hidden>
      {values.map((v, i) => (
        <span
          key={i}
          title={`${new Date((start + i) * 86_400_000).toLocaleDateString()} · ${v} uses`}
          className={`w-1.5 flex-1 rounded-[2px] ${v ? "bg-data" : "bg-bg-tertiary"}`}
          style={v ? { opacity: 0.35 + 0.65 * (v / max) } : undefined}
        />
      ))}
    </span>
  );
}

export function StatusVariant({
  snapshot,
  skills,
  listings,
  agents,
  open,
}: {
  snapshot: SkillSnapshot;
  skills: LibrarySkill[];
  listings: HarnessListing[];
  agents: string[] | undefined;
  open: (name: string) => void;
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const days = dailyUses(snapshot, agents, DAYS);
  const byName = new Map(skills.map((s) => [s.name, s]));
  const rows = (agents ?? listings.map((r) => r.harness)).map((agent) => {
    const label = deploymentLabelFromAgentId(agent);
    const listing = listings.find((r) => r.harness === agent);
    const cut = (listing?.entries ?? []).filter(
      (e) => e.state === "name_only" || e.state === "omitted",
    );
    const broken = skills.filter(
      (s) => s.broken && s.skill.deployments.some((d) => d.agent === label || d.agent === "shared"),
    );
    const uses = days.byAgent.get(agent) ?? Array.from({ length: DAYS }, () => 0);
    const problems = broken.length + cut.length;
    return { agent, label, listing, cut, broken, uses, problems };
  });
  const problems = rows.reduce((t, r) => t + r.problems, 0);
  const updates = skills.filter((s) => s.update);
  const history = [...skills]
    .filter((s) => s.skill.updated_at)
    .sort((a, b) => Date.parse(b.skill.updated_at ?? "") - Date.parse(a.skill.updated_at ?? ""))
    .slice(0, 6);

  return (
    <section aria-label="Agent status" className="grid gap-8 md:grid-cols-[minmax(0,1fr)_17rem]">
      <div className="flex flex-col gap-4">
        <p className="flex items-center gap-2 text-heading font-semibold">
          <span
            className={`size-2.5 rounded-full ${problems ? "bg-warning" : "bg-success"}`}
            aria-hidden
          />
          {problems
            ? `${formatCount(problems)} problems across your agents`
            : "Every agent sees its skills"}
        </p>
        <ul className="overflow-hidden rounded-lg border border-border-subtle">
          {rows.map((row) => {
            const icon = harnessIdFromLabel(row.label);
            const isOpen = expanded === row.agent;
            const total = row.uses.reduce((a, b) => a + b, 0);
            return (
              <li key={row.agent} className="border-b border-border-subtle last:border-b-0">
                <Button
                  variant="ghost"
                  aria-expanded={isOpen}
                  className="grid h-auto w-full grid-cols-[1rem_8rem_minmax(8rem,1fr)_9rem] items-center gap-3 rounded-none px-3 py-3 font-normal"
                  onClick={() => setExpanded(isOpen ? null : row.agent)}
                >
                  <ChevronRight
                    size={14}
                    className={`transition-transform duration-150 ${isOpen ? "rotate-90" : ""}`}
                    aria-hidden
                  />
                  <span className="inline-flex items-center gap-2 text-left">
                    {icon && <HarnessIcon harness={icon} size={14} />}
                    {row.label}
                  </span>
                  <Strip values={row.uses} start={days.start} />
                  <span className="text-right text-small tabular-nums">
                    {row.problems ? (
                      <span className="text-warning">
                        {[
                          row.broken.length && `${row.broken.length} broken`,
                          row.cut.length && `${row.cut.length} cut`,
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                    ) : (
                      <span className="text-success">Healthy</span>
                    )}
                    <span className="block text-caption text-text-tertiary">
                      {formatCount(total)} uses
                    </span>
                  </span>
                </Button>
                {isOpen && (
                  <div className="grid gap-4 bg-bg-secondary px-10 py-3 text-small sm:grid-cols-2">
                    {[
                      { title: "Don’t load", items: row.broken.map((s) => s.name) },
                      {
                        title: "Name only in its skill listing",
                        items: row.cut.map((e) => e.name),
                      },
                    ].map((group) => (
                      <div key={group.title}>
                        <p className="mb-1 text-caption text-text-tertiary">
                          {group.title} · {group.items.length}
                        </p>
                        {group.items.length ? (
                          group.items.slice(0, 8).map((name) => (
                            <Button
                              key={name}
                              variant="ghost"
                              size="sm"
                              className="-ml-2 block h-7 font-normal"
                              onClick={() => byName.has(name) && open(name)}
                            >
                              {name}
                            </Button>
                          ))
                        ) : (
                          <p className="text-text-secondary">None</p>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
        <p className="flex justify-between text-caption text-text-tertiary">
          <span>{DAYS} days ago</span>
          <span>today</span>
        </p>
      </div>
      <aside className="flex flex-col gap-3">
        <p className="text-small font-medium text-text-secondary">
          Updates · {formatCount(updates.length)}
        </p>
        <ul className="flex flex-col">
          {updates.slice(0, 4).map((s) => (
            <li key={s.name}>
              <Button
                variant="ghost"
                size="sm"
                className="-ml-2 h-7 font-normal"
                onClick={() => open(s.name)}
              >
                {s.name}
              </Button>
            </li>
          ))}
        </ul>
        <p className="mt-2 text-small font-medium text-text-secondary">Recently changed</p>
        <ul className="flex flex-col gap-1.5 border-l border-border-subtle pl-3">
          {history.map((s) => (
            <li key={s.name} className="text-small">
              <Button
                variant="ghost"
                size="sm"
                className="-ml-2 h-6 font-normal"
                onClick={() => open(s.name)}
              >
                {s.name}
              </Button>
              <span className="block text-caption text-text-tertiary">
                {formatRelativeTime(s.skill.updated_at ?? "")}
              </span>
            </li>
          ))}
        </ul>
      </aside>
    </section>
  );
}

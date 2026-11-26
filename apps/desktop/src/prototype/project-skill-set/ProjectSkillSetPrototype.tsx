// PROTOTYPE (throwaway, branch prototype/project-skill-set).
// Question: how should "choose which skills a project gets" and "move a global
// skill into a project" fit on the Skills page? Three structurally different
// variants on the Skills page in Project scope, switched with ?variant=A|B|C and
// the floating bar. Changes are kept in memory and never written.

import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, ArrowRightLeft } from "lucide-react";
import { basename, formatCount } from "@skill-studio/lib";
import type { SkillSnapshot } from "@skill-studio/lib";
import { Button, ToggleGroup, ToggleGroupItem } from "@skill-studio/ui";
import { PageSection } from "../../components/Shell/PageSection";
import { SummaryCard, SummaryMetric } from "../../components/ui/SummaryCard";
import { useAppStore } from "../../store/appStore";
import { planSummary, projectSkillRows } from "./project-skill-model";
import type { Home, Plan, ProjectSkillRow } from "./project-skill-model";

const VARIANTS = [
  ["A", "Checklist: every skill, where it lives"],
  ["B", "Suggestions: one click per kind of change"],
  ["C", "Two columns: move skills across"],
] as const;

function useVariant() {
  const [index, setIndex] = useState(() =>
    Math.max(
      0,
      VARIANTS.findIndex(([key]) => key === new URLSearchParams(location.search).get("variant")),
    ),
  );
  const go = (step: number) => {
    const next = (index + step + VARIANTS.length) % VARIANTS.length;
    setIndex(next);
    const url = new URL(location.href);
    url.searchParams.set("variant", VARIANTS[next][0]);
    history.replaceState(history.state, "", url);
  };
  return { index, go };
}

function elsewhereText(row: ProjectSkillRow) {
  if (!row.usesElsewhere.length) return "–";
  const [top, ...rest] = row.usesElsewhere;
  return `${basename(top.project)} ${formatCount(top.count)}${rest.length ? ` +${rest.length}` : ""}`;
}

function PlanBar({
  rows,
  plan,
  project,
  reset,
}: {
  rows: ProjectSkillRow[];
  plan: Plan;
  project: string;
  reset: () => void;
}) {
  const addToast = useAppStore((s) => s.addToast);
  const summary = planSummary(rows, plan, project);
  if (!summary.changes.length) return null;
  return (
    <div className="sticky bottom-14 z-10 flex flex-col gap-2 rounded-lg border border-border bg-bg-elevated p-3 shadow-md">
      <div className="flex items-center gap-2">
        <strong className="mr-auto text-body font-semibold">
          {summary.changes.length} {summary.changes.length === 1 ? "change" : "changes"}
          {summary.freed > 0 && (
            <span className="font-normal text-text-secondary">
              {" "}
              · frees {formatCount(summary.freed)} tokens per message in your other projects
            </span>
          )}
        </strong>
        <Button size="sm" variant="ghost" onClick={reset}>
          Reset
        </Button>
        <Button
          size="sm"
          onClick={() =>
            addToast({
              type: "success",
              title: "Prototype: nothing was written",
              message: summary.text.join("\n"),
            })
          }
        >
          Apply
        </Button>
      </div>
      <ul className="m-0 list-none p-0 text-small text-text-secondary">
        {summary.text.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
    </div>
  );
}

/** A: every skill this project can use, one row each, with where it lives. */
function Checklist({ rows, plan, setHome, project }: VariantProps) {
  return (
    <PageSection title={`Skills in ${basename(project)}`} meta="Uses in the last 30 days">
      <table className="w-full border-collapse text-left text-body tabular-nums [&_td]:py-1.5 [&_td]:pr-3 [&_th]:pb-2 [&_th]:pr-3 [&_tr]:border-b [&_tr]:border-border-subtle">
        <thead className="text-small text-text-secondary">
          <tr>
            <th className="font-medium">Skill</th>
            <th className="font-medium">Here</th>
            <th className="font-medium">Elsewhere</th>
            <th className="w-1 font-medium">Lives in</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const home = plan.get(row.skill.name) ?? row.home;
            return (
              <tr
                key={row.skill.name}
                className={home !== row.home ? "bg-accent-softer" : undefined}
              >
                <td>{row.skill.name}</td>
                <td className={row.usesHere ? "" : "text-text-tertiary"}>
                  {row.usesHere ? formatCount(row.usesHere) : "–"}
                </td>
                <td className="text-text-secondary">{elsewhereText(row)}</td>
                <td>
                  <ToggleGroup
                    variant="segmented"
                    aria-label={`Where ${row.skill.name} lives`}
                    value={[home]}
                    onValueChange={([next]) =>
                      next && setHome(row.skill.name, next === "here" ? "here" : "global")
                    }
                  >
                    <ToggleGroupItem value="global">Every project</ToggleGroupItem>
                    <ToggleGroupItem value="here">Only here</ToggleGroupItem>
                  </ToggleGroup>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </PageSection>
  );
}

/** B: the data decides; each kind of change is one button. */
function Suggestions({ rows, plan, setHome, project }: VariantProps) {
  const name = basename(project);
  const used = rows.filter((row) => row.usesHere > 0);
  const onlyHere = rows.filter(
    (row) => row.home === "global" && row.usesHere > 0 && row.usesElsewhere.length === 0,
  );
  const shared = rows.filter((row) => row.usesHere > 0 && row.usesElsewhere.length > 0);
  const unused = rows.filter((row) => row.usesHere === 0 && row.usesElsewhere.length === 0);
  const groups = [
    {
      title: `Used only in ${name}`,
      why: "Move them here, so your other projects stop paying for them.",
      rows: onlyHere,
      action: "Move here",
      apply: () => onlyHere.forEach((row) => setHome(row.skill.name, "here")),
    },
    {
      title: "Used here and in other projects",
      why: "Keep them in every project.",
      rows: shared,
    },
    {
      title: "Not used anywhere in 30 days",
      why: "Park them to take them out of every project; Unpark brings them back.",
      rows: unused,
      action: "Park (prototype)",
    },
  ];
  return (
    <SummaryCard title={`Skills for ${name}`}>
      <SummaryMetric
        value={used.length}
        unit={`of ${rows.length} skills ${name} can use ran here in the last 30 days`}
      />
      <div className="mt-4 flex flex-col gap-3">
        {groups.map((group) =>
          group.rows.length === 0 ? null : (
            <div key={group.title} className="border-t border-border-subtle pt-3">
              <div className="flex items-center gap-2">
                <strong className="mr-auto text-body font-semibold">
                  {group.title}{" "}
                  <span className="font-normal text-text-tertiary">{group.rows.length}</span>
                </strong>
                {group.action && (
                  <Button size="sm" variant="outline" onClick={group.apply} disabled={!group.apply}>
                    {group.action}
                  </Button>
                )}
              </div>
              <p className="text-small text-text-secondary">{group.why}</p>
              <p className="mt-1 text-small">
                {group.rows
                  .map(
                    (row) => `${row.skill.name}${plan.get(row.skill.name) === "here" ? " ✓" : ""}`,
                  )
                  .join(" · ")}
              </p>
            </div>
          ),
        )}
      </div>
    </SummaryCard>
  );
}

/** C: two columns; a skill moves across with one click. */
function TwoColumns({ rows, plan, setHome, project }: VariantProps) {
  const column = (home: Home, title: string, note: string) => {
    const list = rows.filter((row) => (plan.get(row.skill.name) ?? row.home) === home);
    return (
      <div className="flex min-w-0 flex-1 flex-col rounded-lg border border-border-subtle">
        <div className="border-b border-border-subtle px-3 py-2">
          <strong className="text-body font-semibold">
            {title} <span className="font-normal text-text-tertiary">{list.length}</span>
          </strong>
          <p className="text-small text-text-secondary">{note}</p>
        </div>
        <ul className="m-0 max-h-96 list-none overflow-y-auto p-1">
          {list.map((row) => (
            <li key={row.skill.name}>
              <Button
                variant="ghost"
                size="sm"
                className={`w-full justify-between font-normal ${plan.has(row.skill.name) && plan.get(row.skill.name) !== row.home ? "bg-accent-softer" : ""}`}
                onClick={() => setHome(row.skill.name, home === "here" ? "global" : "here")}
              >
                <span className="truncate">{row.skill.name}</span>
                <span className="flex items-center gap-2 text-text-tertiary tabular-nums">
                  {row.usesHere ? `${formatCount(row.usesHere)} here` : "unused here"}
                  <ArrowRightLeft size={12} aria-hidden />
                </span>
              </Button>
            </li>
          ))}
        </ul>
      </div>
    );
  };
  return (
    <PageSection title={`Skills for ${basename(project)}`} meta="Click a skill to move it across">
      <div className="flex gap-3">
        {column("global", "Every project", "Global: listed in every project you open")}
        {column("here", `Only ${basename(project)}`, "Listed only when you work here")}
      </div>
    </PageSection>
  );
}

interface VariantProps {
  rows: ProjectSkillRow[];
  plan: Plan;
  setHome: (name: string, home: Home) => void;
  project: string;
}

export function ProjectSkillSetPrototype({
  snapshot,
  project,
}: {
  snapshot: SkillSnapshot;
  project: string;
}) {
  const { index, go } = useVariant();
  const [plan, setPlan] = useState<Plan>(new Map());
  const rows = projectSkillRows(snapshot, project);
  const setHome = (name: string, home: Home) =>
    setPlan((current) => new Map(current).set(name, home));

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target;
      if (target instanceof HTMLElement && target.closest("input, textarea, [contenteditable]"))
        return;
      if (!event.altKey) return;
      if (event.key === "ArrowLeft") go(-1);
      if (event.key === "ArrowRight") go(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const props = { rows, plan, setHome, project };
  const [key, label] = VARIANTS[index];
  return (
    <>
      {key === "A" && <Checklist {...props} />}
      {key === "B" && <Suggestions {...props} />}
      {key === "C" && <TwoColumns {...props} />}
      <PlanBar rows={rows} plan={plan} project={project} reset={() => setPlan(new Map())} />
      {import.meta.env.DEV && (
        <div className="fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 items-center gap-2 rounded-full bg-text-primary px-2 py-1 text-small text-bg-primary shadow-lg">
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label="Previous variant"
            className="rounded-full text-bg-primary"
            onClick={() => go(-1)}
          >
            <ArrowLeft size={14} />
          </Button>
          <span>
            {key} · {label}
          </span>
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label="Next variant"
            className="rounded-full text-bg-primary"
            onClick={() => go(1)}
          >
            <ArrowRight size={14} />
          </Button>
        </div>
      )}
    </>
  );
}

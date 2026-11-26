// ============================================================================
// HomeOverview - The top of Home: what needs the user, most harmful first,
// then the skills they use most and how full each agent's skill space is.
// Each item opens the place that fixes it.
// ============================================================================

import { ArrowRight, Plus } from "lucide-react";
import {
  ALL_ACTIVITY,
  HARNESS_CATALOG,
  deploymentLabelFromAgentId,
  formatCount,
  skillRows,
} from "@skill-studio/lib";
import type { HarnessListing, SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { PageSection } from "../Shell/PageSection";
import { HarnessIcon, harnessIdFromLabel } from "../ui/HarnessIcon";
import { useClientIds } from "../../hooks/useClientRows";
import { useAppStore } from "../../store/appStore";
import { homeTodos, joinNames } from "./home-todos";
import type { HomeTodo } from "./home-todos";
import type { HomeFilter, HomeGroups } from "./home-inbox-data";
import { useHomeListing } from "./useHomeListing";
import { CostUseChart } from "./CostUseChart";

const TONE = { error: "bg-error", warning: "bg-warning", neutral: "bg-text-tertiary" };

function AgentName({ id }: { id: string }) {
  const label = deploymentLabelFromAgentId(id);
  const icon = harnessIdFromLabel(label);
  return (
    <span className="inline-flex min-w-0 items-center gap-2">
      {icon && <HarnessIcon harness={icon} size={14} />}
      <span className="truncate">{label}</span>
    </span>
  );
}

/** One agent's skill space as a bar: full descriptions, shortened, names only. */
function SpaceRow({ row, open }: { row: HarnessListing; open: () => void }) {
  const count = (...states: string[]) => row.entries.filter((e) => states.includes(e.state)).length;
  const full = count("described");
  const cut = count("truncated");
  const names = count("name_only", "omitted");
  const total = full + cut + names;
  const estimated = row.evidence === "preview" && row.budget !== null;
  const text = !estimated
    ? `Folders only · ${row.entries.filter((e) => e.scanned).length} found`
    : names
      ? `${full + cut} of ${total} descriptions fit`
      : cut
        ? `${full} full · ${cut} shortened`
        : `All ${total} descriptions fit`;
  return (
    <Button
      variant="ghost"
      className="-mx-2 grid h-9 w-[calc(100%+1rem)] grid-cols-[7rem_minmax(4rem,1fr)_auto] gap-3 px-2 font-normal"
      onClick={open}
    >
      <AgentName id={row.harness} />
      <span className="flex h-2 overflow-hidden rounded-full bg-bg-tertiary" aria-hidden>
        {estimated && (
          <>
            <span className="bg-data" style={{ flex: full }} />
            <span className="bg-data/45" style={{ flex: cut }} />
            <span style={{ flex: names }} />
          </>
        )}
      </span>
      <span className="text-small text-text-secondary tabular-nums">{text}</span>
    </Button>
  );
}

interface HomeOverviewProps {
  snapshot: SkillSnapshot;
  groups: HomeGroups;
  active: boolean;
  updateCaption: string;
  open: (name: string) => void;
  showFilter: (filter: HomeFilter) => void;
}

export function HomeOverview({
  snapshot,
  groups,
  active,
  updateCaption,
  open,
  showFilter,
}: HomeOverviewProps) {
  const agents = useClientIds();
  const navigate = useAppStore((s) => s.setActiveView);
  const openAddSkillSheet = useAppStore((s) => s.openAddSkillSheet);
  const { load } = useHomeListing(snapshot.revision, active && Boolean(agents?.length));
  const listings = (load?.data?.listings ?? []).filter(
    (row) => row.scope.scope === "global" && (!agents || agents.includes(row.harness)),
  );
  const shown = (id: string) => !agents || agents.includes(id);
  const uses = snapshot.invocations
    .map((stats) => ({
      ...stats,
      by_hour: stats.by_hour.filter((hour) => shown(hour.harness)),
    }))
    .filter((stats) => stats.by_hour.length);
  const historyAgents = HARNESS_CATALOG.flatMap((row) =>
    row.usageReader &&
    shown(row.id) &&
    uses.some((s) => s.by_hour.some((h) => h.harness === row.id))
      ? [row.id]
      : [],
  );
  const ranked = skillRows(uses, "30d", ALL_ACTIVITY, new Date(snapshot.scanned_at));
  const top = ranked.slice(0, 5);
  const totalUses = ranked.reduce((sum, row) => sum + row.count, 0);
  const todos = homeTodos(groups, listings, historyAgents, updateCaption);
  const toggleSkillSelection = useAppStore((s) => s.toggleSkillSelection);
  const clearSkillSelection = useAppStore((s) => s.clearSkillSelection);
  const usesBySkill = new Map(ranked.map((row) => [row.skill, row.count]));
  // The chart needs enough skills to show a spread, and history to place them by use.
  const chartable = groups.own.filter((skill) => !skill.parked && skill.invocation !== "user-only");
  const showChart = chartable.length >= 10 && historyAgents.length > 0;
  const live = groups.own.filter((skill) => !skill.parked).length;
  const go = (todo: HomeTodo) =>
    todo.to.kind === "filter"
      ? showFilter(todo.to.filter)
      : navigate({ kind: "listing", scope: "global", harness: todo.to.harness });

  return (
    <>
      <section aria-labelledby="home-todos-title">
        <h2 id="home-todos-title" className="text-heading font-semibold">
          {live === 0
            ? "No skills yet"
            : todos.length
              ? `${todos.length} ${todos.length === 1 ? "thing needs" : "things need"} you`
              : "Nothing needs you"}
        </h2>
        <p className="mt-1 text-small text-text-secondary">
          {live === 0
            ? "Add a skill and Skill Studio shows here whether your agents can see it and use it."
            : `${formatCount(live)} ${live === 1 ? "skill" : "skills"} you installed · ${formatCount(totalUses)} ${totalUses === 1 ? "use" : "uses"} in 30 days · ${
                historyAgents.length
                  ? `uses from ${joinNames(historyAgents.map(deploymentLabelFromAgentId))} history`
                  : "no readable skill history yet"
              }`}
        </p>
        {live === 0 && (
          <Button className="mt-3" onClick={() => openAddSkillSheet()}>
            <Plus size={14} aria-hidden /> Add a skill
          </Button>
        )}
        {todos.length > 0 && (
          <ul aria-label="Things that need you" className="mt-3 border-t border-border-subtle">
            {todos.map((todo) => (
              <li
                key={todo.id}
                className="grid grid-cols-[8px_minmax(0,1fr)_auto] items-center gap-3 border-b border-border-subtle py-2.5"
              >
                <span aria-hidden className={`size-2 rounded-full ${TONE[todo.tone]}`} />
                <span className="min-w-0">
                  <strong className="block text-body font-semibold">{todo.title}</strong>
                  <span className="block text-small text-text-secondary">{todo.detail}</span>
                </span>
                <Button size="sm" variant="outline" onClick={() => go(todo)}>
                  {todo.action}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>

      {showChart && (
        <PageSection
          title="What each skill costs, and what it gives back"
          meta="Right: used more · Up: more room in every message"
        >
          <CostUseChart
            points={chartable.map((skill) => ({ skill, uses: usesBySkill.get(skill.name) ?? 0 }))}
            open={open}
            park={(skills) => {
              clearSkillSelection();
              for (const skill of skills) {
                const path = skill.deployments[0]?.path;
                if (path) toggleSkillSelection(path);
              }
            }}
          />
        </PageSection>
      )}

      {live > 0 && (
        <div className="grid gap-8 @container md:grid-cols-2">
          <PageSection
            title="Used most"
            meta={
              <Button
                variant="ghost"
                size="sm"
                className="-mr-2.5 text-text-primary"
                onClick={() => navigate({ kind: "activity" })}
              >
                View activity <ArrowRight aria-hidden />
              </Button>
            }
          >
            {top.length ? (
              <ul aria-label="Used most, 30 days">
                {top.map(({ skill, count }) => (
                  <li key={skill}>
                    <Button
                      variant="ghost"
                      className="-mx-2 grid h-8 w-[calc(100%+1rem)] grid-cols-[minmax(0,10rem)_minmax(3rem,1fr)_auto] gap-3 px-2 font-normal"
                      onClick={() => open(skill)}
                    >
                      <span className="truncate text-left">{skill}</span>
                      <span className="flex h-1.5 rounded-full" aria-hidden>
                        <span
                          className="rounded-full bg-data"
                          style={{ width: `${Math.max(4, (count / top[0].count) * 100)}%` }}
                        />
                      </span>
                      <span className="text-small text-text-secondary tabular-nums">
                        {formatCount(count)}
                      </span>
                    </Button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-small text-text-secondary">
                {historyAgents.length
                  ? "No skill use in the last 30 days."
                  : "None of your agents has skill history Skill Studio can read yet."}
              </p>
            )}
          </PageSection>
          <PageSection title="Skill space">
            {listings.length ? (
              <div aria-label="Skill space by agent" role="group">
                {listings.map((row) => (
                  <SpaceRow
                    key={row.harness}
                    row={row}
                    open={() =>
                      navigate({ kind: "listing", scope: "global", harness: row.harness })
                    }
                  />
                ))}
              </div>
            ) : (
              <p className="text-small text-text-secondary">
                {load?.error ?? (load?.data ? "No agent to estimate." : "Reading your agents…")}
              </p>
            )}
          </PageSection>
        </div>
      )}
    </>
  );
}

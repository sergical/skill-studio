// PROTOTYPE (throwaway, branch prototype/project-skill-set): the data every Home library
// variant draws from. Deleted with the prototype once a direction is picked.

import { ALL_ACTIVITY, skillRows } from "@skill-studio/lib";
import type { HarnessListing, InstalledSkill, SkillSnapshot } from "@skill-studio/lib";
import type { HomeGroups } from "../../components/Home/home-inbox-data";

export type ListState = "described" | "truncated" | "name_only" | "omitted" | "hidden";

export interface LibrarySkill {
  name: string;
  skill: InstalledSkill;
  uses: number;
  tokens: number;
  source: string;
  broken: boolean;
  warning: boolean;
  update: boolean;
  /** Agent id -> how that agent lists the skill, for agents with an estimate. */
  listed: Record<string, ListState>;
  /** True when at least one estimated agent lists only the name, or leaves it out. */
  cut: boolean;
}

export function sourceLabel(skill: InstalledSkill): string {
  if (skill.source_kind === "plugin")
    return skill.deployments.find((d) => d.plugin)?.plugin?.name ?? "Plugin";
  if (skill.source_kind === "manual" || skill.source_kind === "in-repo") return "Your folders";
  if (skill.source_kind === "fork") return "Your forks";
  return skill.source || "Unknown source";
}

export function buildLibrary(
  snapshot: SkillSnapshot,
  groups: HomeGroups,
  listings: HarnessListing[],
  agents: string[] | undefined,
) {
  const shown = (id: string) => !agents || agents.includes(id);
  const uses = snapshot.invocations
    .map((stats) => ({ ...stats, by_hour: stats.by_hour.filter((h) => shown(h.harness)) }))
    .filter((stats) => stats.by_hour.length);
  const counts = new Map(
    skillRows(uses, "30d", ALL_ACTIVITY, new Date(snapshot.scanned_at)).map((r) => [
      r.skill,
      r.count,
    ]),
  );
  const broken = new Set(groups.broken.map((issue) => issue.skill.name));
  const warning = new Set(groups.warnings.map((issue) => issue.skill.name));
  const listed = new Map<string, Record<string, ListState>>();
  for (const row of listings) {
    if (row.evidence !== "preview" || row.budget === null) continue;
    for (const entry of row.entries) {
      const map = listed.get(entry.name) ?? {};
      map[row.harness] = entry.state;
      listed.set(entry.name, map);
    }
  }
  const skills: LibrarySkill[] = groups.own
    .filter((skill) => !skill.parked)
    .map((skill) => {
      const states = listed.get(skill.name) ?? {};
      return {
        name: skill.name,
        skill,
        uses: counts.get(skill.name) ?? 0,
        tokens: skill.description_tokens,
        source: sourceLabel(skill),
        broken: broken.has(skill.name),
        warning: warning.has(skill.name),
        update: skill.update_owner_ids.length > 0,
        listed: states,
        cut: Object.values(states).some((s) => s === "name_only" || s === "omitted"),
      };
    });
  return { skills, history: uses.length > 0 };
}

export const STATE_WORDS = {
  described: "Full description",
  truncated: "Shortened",
  name_only: "Name only",
  omitted: "Not listed",
  hidden: "You run it",
} satisfies Record<ListState, string>;

/** Uses per day for the last `days` days (oldest first), from the hourly history, for the
 * agents shown: in total, per agent and per skill. */
export function dailyUses(snapshot: SkillSnapshot, agents: string[] | undefined, days = 14) {
  const end = Math.floor(Date.parse(snapshot.scanned_at) / 86_400_000);
  const start = end - days + 1;
  const total = Array.from({ length: days }, () => 0);
  const byAgent = new Map<string, number[]>();
  const bySkill = new Map<string, number[]>();
  for (const stats of snapshot.invocations) {
    for (const hour of stats.by_hour) {
      if (agents && !agents.includes(hour.harness)) continue;
      const day = Math.floor(hour.hour / 24) - start;
      if (day < 0 || day >= days) continue;
      total[day] += hour.count;
      const agent = byAgent.get(hour.harness) ?? Array.from({ length: days }, () => 0);
      agent[day] += hour.count;
      byAgent.set(hour.harness, agent);
      const skill = bySkill.get(stats.skill) ?? Array.from({ length: days }, () => 0);
      skill[day] += hour.count;
      bySkill.set(stats.skill, skill);
    }
  }
  const labels = Array.from({ length: days }, (_, i) =>
    new Date((start + i) * 86_400_000).toLocaleDateString(undefined, { weekday: "narrow" }),
  );
  return { total, byAgent, bySkill, labels, start };
}

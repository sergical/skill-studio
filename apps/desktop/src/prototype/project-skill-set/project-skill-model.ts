// PROTOTYPE (throwaway, branch prototype/project-skill-set): the data the three
// "project skill set" variants share. Nothing here writes to disk.

import { basename } from "@skill-studio/lib";
import type { InstalledSkill, SkillInvocationStats, SkillSnapshot } from "@skill-studio/lib";

export type Home = "global" | "here";

export interface ProjectSkillRow {
  skill: InstalledSkill;
  /** Where the skill lives now: a global copy reaches every project, even with a project copy too. */
  home: Home;
  usesHere: number;
  usesElsewhere: { project: string; count: number }[];
  tokens: number;
}

export function projectSkillRows(snapshot: SkillSnapshot, project: string): ProjectSkillRow[] {
  const stats = new Map<string, SkillInvocationStats>(
    snapshot.invocations.map((s) => [s.skill, s]),
  );
  return snapshot.skills
    .filter((skill) => !skill.parked && skill.deployments.some((d) => d.scope !== "plugin"))
    .flatMap((skill) => {
      const here = skill.deployments.some((d) => d.project_path === project);
      const global = skill.deployments.some((d) => d.scope === "global");
      if (!here && !global) return [];
      const byProject = stats.get(skill.name)?.by_project_30_days ?? {};
      return [
        {
          skill,
          home: global ? ("global" as const) : ("here" as const),
          usesHere: byProject[project] ?? 0,
          usesElsewhere: Object.entries(byProject)
            .filter(([path, count]) => path !== project && count > 0)
            .map(([path, count]) => ({ project: path, count }))
            .sort((a, b) => b.count - a.count),
          tokens: skill.description_tokens,
        },
      ];
    })
    .sort((a, b) => b.usesHere - a.usesHere || a.skill.name.localeCompare(b.skill.name));
}

export const projectName = basename;

/** One pending change: the skill and where it would live. */
export type Plan = Map<string, Home>;

export function planSummary(rows: ProjectSkillRow[], plan: Plan, project: string) {
  const changes = rows.flatMap((row) => {
    const next = plan.get(row.skill.name);
    if (!next || next === row.home) return [];
    return [{ row, next }];
  });
  const freed = changes
    .filter(({ next }) => next === "here")
    .reduce((sum, { row }) => sum + row.tokens, 0);
  return {
    changes,
    text: changes.map(({ row, next }) =>
      next === "here"
        ? `Move ${row.skill.name} from every project to only ${projectName(project)}`
        : `Make ${row.skill.name} available in every project`,
    ),
    freed,
  };
}

import { describe, expect, it } from "vitest";
import {
  applySkillListFilter,
  defaultSkillListFilter,
  ownSkillsWithPluginUpdates,
} from "@skill-studio/lib";
import type { InstalledSkill, SkillInvocationStats } from "@skill-studio/lib";
import { buildHarnessSnapshot, deployment, skill } from "../../dev/harness/skill-fixture";
import { homeAsks, homeCounts, names, promptFor } from "./home-asks";

// Claude Code reads only its own folder, so its history covers these skills.
const inClaude = (name: string, overrides: Partial<InstalledSkill> = {}) =>
  skill({
    name,
    deployments: [
      deployment({ agent: "Claude Code", scope: "global", path: `/h/.claude/skills/${name}` }),
    ],
    ...overrides,
  });
const inCursor = (name: string) =>
  skill({
    name,
    deployments: [
      deployment({ agent: "Cursor", scope: "global", path: `/h/.cursor/skills/${name}` }),
    ],
  });
const uses = (name: string, byAgent: Record<string, number>): SkillInvocationStats => {
  const count = Object.values(byAgent).reduce((a, b) => a + b, 0);
  return {
    skill: name,
    last_24_hours: 0,
    last_7_days: count,
    last_14_days: count,
    last_30_days: count,
    last_used: null,
    by_project_30_days: {},
    by_day: {},
    by_harness_30_days: byAgent,
    by_trigger_30_days: { user: 0, agent: count, file_read: 0 },
    by_hour: [],
  };
};

describe("homeCounts", () => {
  it("offers no parked skill for parking, so the count names only skills an agent still reads", () => {
    const parked = skill({
      name: "parked-idle",
      parked: true,
      deployments: [
        deployment({ agent: "parked", scope: "parked", path: "/h/.agents/skills-parked/x" }),
      ],
    });
    const counts = homeCounts(
      {
        ...buildHarnessSnapshot(),
        skills: [inClaude("live-used"), inClaude("live-idle"), parked],
        invocations: [uses("live-used", { "claude-code": 3 })],
      },
      undefined,
    );
    expect(counts.unused).toEqual(["live-idle"]);
  });

  it("counts only the broken skills its Skills filter lists, so a parked broken skill adds none", () => {
    const broken = (name: string, parked: boolean) =>
      skill({
        name,
        parked,
        spec_violations: ["missing required frontmatter field: description"],
        deployments: [
          deployment({
            agent: parked ? "parked" : "shared",
            scope: parked ? "parked" : "global",
            path: parked ? `/h/.agents/skills-parked/${name}` : `/h/.agents/skills/${name}`,
            spec_violations: ["missing required frontmatter field: description"],
          }),
        ],
      });
    const skills = [broken("live-broken", false), broken("parked-broken", true)];
    const counts = homeCounts({ ...buildHarnessSnapshot(), skills }, undefined);
    const [chip] = homeAsks(counts, []).chips;
    const listed = applySkillListFilter(ownSkillsWithPluginUpdates(skills), {
      ...defaultSkillListFilter(),
      ...chip.filter,
    });
    expect(chip).toMatchObject({ id: "broken", count: 1 });
    expect(listed.map((s) => s.name)).toEqual(["live-broken"]);
  });

  it("puts a skill with an unquoted colon in its description under warnings, not broken", () => {
    const colon =
      "invalid YAML frontmatter at line 3, column 296: mapping values are not allowed in this context (unquoted colon in a value)";
    const withViolation = (name: string, violation: string) =>
      skill({
        name,
        spec_violations: [violation],
        deployments: [
          deployment({
            agent: "shared",
            scope: "global",
            path: `/h/.agents/skills/${name}`,
            spec_violations: [violation],
          }),
        ],
      });
    const counts = homeCounts(
      {
        ...buildHarnessSnapshot(),
        skills: [
          withViolation("verify-skill-studio", colon),
          withViolation("no-description", "missing required frontmatter field: description"),
        ],
      },
      undefined,
    );
    expect(counts.broken).toEqual(["no-description"]);
    expect(counts.warnings).toEqual(["verify-skill-studio"]);
  });

  it("counts unused from the kept agents' history, so Cursor turned off adds no Cursor-only skill", () => {
    const snapshot = {
      ...buildHarnessSnapshot(),
      skills: [inClaude("used"), inClaude("idle"), inCursor("cursor-only")],
      invocations: [uses("used", { "claude-code": 3 }), uses("idle", { cursor: 1 })],
    };
    // Cursor's history makes its own skill unused and its one use of "idle" a use.
    expect(homeCounts(snapshot, ["claude-code", "cursor"]).unused).toEqual(["cursor-only"]);
    // With Cursor off, its records still sit in the snapshot but count for nothing.
    const counts = homeCounts(snapshot, ["claude-code"]);
    expect(counts.unused).toEqual(["idle"]);
    const { tasks, chips } = homeAsks(counts, []);
    expect(chips.find((chip) => chip.id === "unused")?.count).toBe(1);
    expect(tasks.find((task) => task.id === "unused")?.line).toMatch(/: idle\.$/);
  });
});

describe("the prompt", () => {
  it("names ten skills, then says how many more", () => {
    const list = Array.from({ length: 13 }, (_, i) => `skill-${i + 1}`);
    expect(names(list)).toBe(`${list.slice(0, 10).join(", ")} and 3 more`);
    expect(names(list.slice(0, 10))).toBe(list.slice(0, 10).join(", "));
  });

  it("reads as one sentence for one item and as a list for more, each count with its own line", () => {
    const { tasks, chips } = homeAsks(
      { broken: ["a"], warnings: [], updates: ["b", "c"], unused: [] },
      [],
    );
    expect(chips.map((chip) => [chip.id, chip.count])).toEqual([
      ["broken", 1],
      ["updates", 2],
    ]);
    expect(tasks.map((task) => task.id)).toEqual(["broken", "updates"]);
    expect(promptFor(tasks.slice(0, 1))).toBe(
      "Use the Skill Studio MCP tools to fix the 1 broken skill: a. Tell me what you changed.",
    );
    expect(promptFor(tasks).split("\n")).toEqual([
      "Use the Skill Studio MCP tools to sort out my skills:",
      "- Fix the 1 broken skill: a.",
      "- Show me what changed in the 2 skills with updates, then update the ones I approve: b, c.",
      "Tell me what you changed.",
    ]);
  });
});

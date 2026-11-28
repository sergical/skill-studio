// ============================================================================
// Skill Studio - skill-list-filter tests
// ============================================================================

import fc from "fast-check";
import { describe, expect, it } from "vitest";
import { applySkillListFilter, defaultSkillListFilter } from "./skill-list-filter";
import type { SkillListFilter } from "./skill-list-filter";
import type { Deployment, InstalledSkill, SkillInvocationStats } from "./skill-types";

function fixtureStats(overrides: Partial<SkillInvocationStats> = {}): SkillInvocationStats {
  return {
    skill: "find-bugs",
    total: 0,
    last_24_hours: 0,
    last_7_days: 0,
    last_14_days: 0,
    last_30_days: 0,
    by_project_30_days: {},
    by_day: {},
    by_harness_30_days: {},
    by_trigger_30_days: { user: 0, agent: 0, file_read: 0 },
    ...overrides,
  };
}

function fixtureDeployment(overrides: Partial<Deployment> = {}): Deployment {
  return {
    agent: "Claude Code",
    scope: "global",
    path: "/home/.claude/skills/find-bugs",
    is_symlink: false,
    symlink_is_broken: false,
    content_hash: "abc",
    disabled: false,
    ...overrides,
  };
}

function fixtureSkill(overrides: Partial<InstalledSkill> = {}): InstalledSkill {
  return {
    name: "find-bugs",
    source: "getsentry/find-bugs",
    source_type: "github",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: false,
    source_kind: "dotagents",
    deployments: [fixtureDeployment()],
    has_spec: false,
    spec_violations: [],
    skill_md_tokens: 0,
    description_tokens: 0,
    folder_bytes: 0,
    file_count: 0,
    content_hash: "",
    content_hashes: [],
    frontmatter_fields: {},
    folder_truncated: false,
    parked: false,
    invocation: "both",
    ...overrides,
    update_owner_ids: overrides.update_owner_ids ?? [],
  };
}

describe("applySkillListFilter", () => {
  it("defaults to every non-parked skill with an empty query", () => {
    const skills = [fixtureSkill(), fixtureSkill({ name: "parked-one", parked: true })];
    expect(applySkillListFilter(skills, defaultSkillListFilter())).toEqual([skills[0]]);
  });

  it("parked scope shows only parked skills", () => {
    const skills = [fixtureSkill(), fixtureSkill({ name: "parked-one", parked: true })];
    const result = applySkillListFilter(skills, { scope: "parked", query: "" });
    expect(result.map((s) => s.name)).toEqual(["parked-one"]);
  });

  it("global scope includes global and plugin deployments, not project ones", () => {
    const globalSkill = fixtureSkill({ name: "global-skill" });
    const projectSkill = fixtureSkill({
      name: "project-skill",
      deployments: [fixtureDeployment({ scope: "project", project_path: "/repo" })],
    });
    const result = applySkillListFilter([globalSkill, projectSkill], {
      scope: "global",
      query: "",
    });
    expect(result.map((s) => s.name)).toEqual(["global-skill"]);
  });

  it("project scope matches only deployments with that project_path", () => {
    const skillA = fixtureSkill({
      name: "a",
      deployments: [fixtureDeployment({ scope: "project", project_path: "/repo-a" })],
    });
    const skillB = fixtureSkill({
      name: "b",
      deployments: [fixtureDeployment({ scope: "project", project_path: "/repo-b" })],
    });
    const result = applySkillListFilter([skillA, skillB], {
      scope: { project: "/repo-a" },
      query: "",
    });
    expect(result.map((s) => s.name)).toEqual(["a"]);
  });

  it("filters by harness display label", () => {
    const skills = [
      fixtureSkill({ name: "claude-only" }),
      fixtureSkill({ name: "codex-only", deployments: [fixtureDeployment({ agent: "Codex" })] }),
    ];
    const result = applySkillListFilter(skills, {
      scope: "all",
      harness: "Codex",
      query: "",
    });
    expect(result.map((s) => s.name)).toEqual(["codex-only"]);
  });

  it("filters by source kind", () => {
    const skills = [
      fixtureSkill({ name: "a", source_kind: "dotagents" }),
      fixtureSkill({ name: "b", source_kind: "manual" }),
    ];
    const result = applySkillListFilter(skills, { scope: "all", source: "manual", query: "" });
    expect(result.map((s) => s.name)).toEqual(["b"]);
  });

  it("filters by issue kind", () => {
    const skills = [
      fixtureSkill({
        name: "broken",
        deployments: [fixtureDeployment({ symlink_is_broken: true })],
      }),
      fixtureSkill({ name: "healthy" }),
    ];
    const result = applySkillListFilter(skills, {
      scope: "all",
      issue: "broken-symlink",
      query: "",
    });
    expect(result.map((s) => s.name)).toEqual(["broken"]);
  });

  it("plugin source filters by deployment, not the aggregate source_kind", () => {
    const skill = fixtureSkill({
      name: "mixed",
      source_kind: "dotagents",
      deployments: [
        fixtureDeployment(),
        fixtureDeployment({ scope: "plugin", plugin: { harness: "Claude Code", name: "acme" } }),
      ],
    });
    const pluginResult = applySkillListFilter([skill], {
      scope: "all",
      source: "plugin",
      query: "",
    });
    expect(pluginResult.map((s) => s.name)).toEqual(["mixed"]);

    const dotagentsResult = applySkillListFilter([skill], {
      scope: "all",
      source: "dotagents",
      query: "",
    });
    expect(dotagentsResult.map((s) => s.name)).toEqual(["mixed"]);
  });

  it("each attention category keeps the skills Home lists under it, and a skill in two shows in both", () => {
    const skills = [
      fixtureSkill({
        name: "broken-and-stale",
        deployments: [fixtureDeployment({ symlink_is_broken: true })],
        update_owner_ids: ["owner-1"],
      }),
      fixtureSkill({ name: "stale", update_owner_ids: ["owner-1"] }),
      fixtureSkill({ name: "healthy" }),
    ];
    const names = (attention: SkillListFilter["attention"]) =>
      applySkillListFilter(skills, { scope: "all", attention, query: "" }).map((s) => s.name);
    expect(names("broken")).toEqual(["broken-and-stale"]);
    expect(names("warning")).toEqual([]);
    expect(names("update")).toEqual(["broken-and-stale", "stale"]);
    expect(names("any")).toEqual(["broken-and-stale", "stale"]);
  });

  it("query matches name, description, and source, case-insensitively", () => {
    const skills = [
      fixtureSkill({ name: "find-bugs", description: "Locates regressions" }),
      fixtureSkill({ name: "other", description: "Unrelated", source: "acme/other" }),
    ];
    expect(applySkillListFilter(skills, { scope: "all", query: "REGRESSIONS" })).toEqual([
      skills[0],
    ]);
    expect(applySkillListFilter(skills, { scope: "all", query: "acme" })).toEqual([skills[1]]);
  });

  it("query words match across hyphens, so typing a skill name with spaces finds it", () => {
    // Users type the words they remember; a hyphenated name must not need its exact separators.
    const skills = [
      fixtureSkill({ name: "i-have-adhd", description: "Focus helper" }),
      fixtureSkill({ name: "ask-emil", description: "Routes to the skill I have in mind" }),
      fixtureSkill({ name: "find-bugs", description: "Locates regressions" }),
    ];
    const names = (query: string) =>
      applySkillListFilter(skills, { scope: "all", query }).map((s) => s.name);
    expect(names("i have")).toEqual(["i-have-adhd", "ask-emil"]);
    expect(names("have adhd")).toEqual(["i-have-adhd"]);
    expect(names("adhd have")).toEqual(["i-have-adhd"]);
    expect(names("ihave")).toEqual(["i-have-adhd"]);
    expect(names("adhd regressions")).toEqual([]);
  });

  it("description words must match as a phrase, so short words do not flood the list", () => {
    const skills = [
      fixtureSkill({ name: "ask-emil", description: "Pick a skill" }),
      fixtureSkill({ name: "planner", description: "Ask a task, emil reviews it" }),
      fixtureSkill({ name: "reviewer", description: "Runs a code-review pass" }),
      fixtureSkill({ name: "linter", description: "Reviews code style" }),
    ];
    const names = (query: string) =>
      applySkillListFilter(skills, { scope: "all", query }).map((s) => s.name);
    expect(names("ask emil")).toEqual(["ask-emil"]);
    expect(names("code review")).toEqual(["reviewer"]);
  });

  it("filters by invocation policy", () => {
    const skills = [
      fixtureSkill({ name: "both", invocation: "both" }),
      fixtureSkill({ name: "model-only", invocation: "model-only" }),
    ];
    const result = applySkillListFilter(skills, {
      scope: "all",
      invocation: "model-only",
      query: "",
    });
    expect(result.map((s) => s.name)).toEqual(["model-only"]);
  });

  it("filters by 30-day usage", () => {
    const skills = [fixtureSkill({ name: "used" }), fixtureSkill({ name: "unused" })];
    const invocations = [
      fixtureStats({ skill: "used", last_30_days: 3, by_harness_30_days: { "claude-code": 3 } }),
    ];

    const usedResult = applySkillListFilter(
      skills,
      { scope: "all", usage: "used-30d", query: "" },
      undefined,
      invocations,
    );
    expect(usedResult.map((s) => s.name)).toEqual(["used"]);

    const unusedResult = applySkillListFilter(
      skills,
      { scope: "all", usage: "unused-30d", query: "" },
      undefined,
      invocations,
    );
    expect(unusedResult.map((s) => s.name)).toEqual(["unused"]);
  });

  it("lists no skill as unused when no agent that reads it has history", () => {
    const unused = { scope: "all", usage: "unused-30d", query: "" } as const;
    const skills = [
      fixtureSkill({ name: "claude" }),
      fixtureSkill({
        name: "cursor-only",
        deployments: [fixtureDeployment({ agent: "Cursor", path: "/home/.cursor/skills/x" })],
      }),
    ];
    expect(applySkillListFilter(skills, unused, undefined, [])).toEqual([]);
    const claudeHistory = [
      fixtureStats({ skill: "other", last_30_days: 1, by_harness_30_days: { "claude-code": 1 } }),
    ];
    expect(
      applySkillListFilter(skills, unused, undefined, claudeHistory).map((s) => s.name),
    ).toEqual(["claude"]);
  });
});

describe("applySkillListFilter query properties", () => {
  const word = fc.string({
    unit: fc.constantFrom(...Array.from("abcXYZ09")),
    minLength: 1,
    maxLength: 4,
  });
  const words = fc.array(word, { minLength: 1, maxLength: 4 });
  const wordBreak = fc.constantFrom("-", " ", "_", "/", ".", "--", " - ");
  /** `parts` joined with a word break picked per gap. */
  const joined = (parts: string[]) =>
    fc
      .array(wordBreak, { minLength: parts.length - 1, maxLength: parts.length - 1 })
      .map((breaks) => parts.reduce((text, part, i) => `${text}${breaks[i - 1] ?? ""}${part}`));
  const query = (skills: InstalledSkill[], q: string) =>
    applySkillListFilter(skills, { ...defaultSkillListFilter(), query: q }).map((s) => s.name);

  it("matches the same skills whichever word break the name and query use, or a hyphenated name hides from a spaced query", () => {
    fc.assert(
      fc.property(
        words.chain((parts) => fc.tuple(fc.constant(parts), joined(parts))),
        words.chain((parts) => fc.tuple(fc.constant(parts), joined(parts))),
        ([nameParts, name], [queryParts, typed]) => {
          const asTyped = query(
            [fixtureSkill({ name, description: null, source: "owner/repo" })],
            typed,
          );
          const hyphenated = query(
            [fixtureSkill({ name: nameParts.join("-"), description: null, source: "owner/repo" })],
            queryParts.join(" ").toUpperCase(),
          );
          expect(asTyped.length).toBe(hyphenated.length);
        },
      ),
    );
  });

  it("keeps a subset of the input in input order", () => {
    fc.assert(
      fc.property(
        fc.uniqueArray(word, { minLength: 0, maxLength: 8 }),
        fc.string({ maxLength: 4 }),
        (names, q) => {
          const result = query(
            names.map((name) => fixtureSkill({ name })),
            q,
          );
          expect(result).toEqual(names.filter((name) => result.includes(name)));
        },
      ),
    );
  });
});

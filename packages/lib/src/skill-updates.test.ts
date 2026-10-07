// ============================================================================
// Skill Studio - skill-updates tests
// ============================================================================

import { describe, expect, it } from "vitest";
import { ownSkillsWithPluginUpdates, skillsWithUpdates } from "./skill-updates";
import type { Deployment, InstalledSkill, SkillSnapshot } from "./skill-types";

function fixtureSkill(overrides: Partial<InstalledSkill> = {}): InstalledSkill {
  return {
    name: "agent-browser",
    source: "getsentry/agent-browser",
    source_type: "github",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: false,
    source_kind: "dotagents",
    deployments: [],
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
    update_owner_ids:
      overrides.update_owner_ids ?? (overrides.has_update ? ["owner:v1/global/agent-browser"] : []),
  };
}

function fixtureSnapshot(skills: InstalledSkill[]): SkillSnapshot {
  return {
    skills,
    projects: [],
    invocations: [],
    heatmap: { days: {} },
    scanned_at: "2026-01-01T00:00:00Z",
    last_test_by_skill: {},
    update_check: {
      checked_at: null,
      gh_status: "ok",
      message: null,
      updates_available: 0,
      upstream_ahead: [],
    },
  };
}

describe("skillsWithUpdates", () => {
  it("returns only skills with update owner ids", () => {
    const skills = [fixtureSkill({ has_update: true }), fixtureSkill({ name: "other" })];
    expect(skillsWithUpdates(fixtureSnapshot(skills)).map((s) => s.name)).toEqual([
      "agent-browser",
    ]);
  });

  it("returns nothing when no skill has an update", () => {
    expect(skillsWithUpdates(fixtureSnapshot([fixtureSkill()]))).toEqual([]);
  });
});

describe("ownSkillsWithPluginUpdates", () => {
  it("lists_a_plugin_only_skill_with_an_update_or_home_counts_a_row_the_list_drops", () => {
    // SAFETY: test fixture; only the fields the code under test reads are filled in.
    const plugin = { plugin: { name: "p" } } as Deployment;
    // SAFETY: test fixture; only the fields the code under test reads are filled in.
    const own = {} as Deployment;
    const pluginOnlyUpdate = fixtureSkill({
      name: "plugin-only",
      has_update: true,
      deployments: [plugin],
    });
    const pluginOnlyCurrent = fixtureSkill({ name: "plugin-current", deployments: [plugin] });
    const ownUpdate = fixtureSkill({ name: "mine", has_update: true, deployments: [own] });
    const skills = [pluginOnlyUpdate, pluginOnlyCurrent, ownUpdate];

    const listed = ownSkillsWithPluginUpdates(skills).map((skill) => skill.name);

    expect(listed.sort()).toEqual(["mine", "plugin-only"]);
    expect(listed.length).toBe(skillsWithUpdates(fixtureSnapshot(skills)).length);
  });
});

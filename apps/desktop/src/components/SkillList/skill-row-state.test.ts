// ============================================================================
// Skill Studio - skill-row-state tests
// ============================================================================

import { describe, expect, it } from "vitest";
import { attentionBySkill, collectDashboardIssues } from "@skill-studio/lib";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";
import { buildScopeGroups, skillRollup } from "../SkillDetail/skill-location-status";
import { rowGroup, rowState } from "./skill-row-state";

/** The group the Skills table puts `skill` in, from the same issues Home lists. */
function groupOf(skill: InstalledSkill) {
  const attention = attentionBySkill([skill], collectDashboardIssues([skill]));
  return rowGroup(skill, attention.get(skill.name));
}

function fixtureDeployment(overrides: Partial<Deployment> = {}): Deployment {
  return {
    id: "dep:v1/global/universal/find-bugs",
    destination: "universal",
    owner_kind: "manual",
    mutability: "read-only",
    backing: { kind: "canonical" },
    agent: "shared",
    scope: "global",
    path: "/home/.agents/skills/find-bugs",
    is_symlink: false,
    symlink_is_broken: false,
    content_hash: "abc",
    disabled: false,
    codex_implicit_invocation: null,
    disabled_by: null,
    invocation: "both",
    spec_violations: [],
    shared_via_whole_dir_link: false,
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
    has_spec: true,
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
    update_owners: [],
    update_owner_ids: [],
    description: null,
    fork: null,
    parked_at: null,
    skill_path: null,
    source_url: null,
    update_commit: null,
    update_commit_at: null,
    updated_at: null,
    ...overrides,
  };
}

describe("rowState", () => {
  it("shows the Fix action as a warning for an unquoted colon in the YAML, which most agents still load", () => {
    const skill = fixtureSkill({
      spec_violations: [
        "invalid YAML frontmatter at line 3, column 1: mapping values are not allowed in this context",
      ],
    });
    const state = rowState(skill);
    expect(state?.kind).toBe("violation");
    expect(state?.level).toBe("warning");
    expect(state?.action).toBe("Fix");
  });

  it("keeps a notes-only skill Healthy, or a harmless length note would crowd Warnings", () => {
    const note = "description exceeds 1024 characters";
    const skill = fixtureSkill({
      spec_violations: [note],
      deployments: [fixtureDeployment({ spec_violations: [note] })],
    });
    expect(rowState(skill)).toBeNull();
    expect(groupOf(skill)).toBe("healthy");
  });

  it("puts a name-mismatch skill in Warnings, so agents' different names get checked", () => {
    const mismatch = 'name "other" does not match its directory name "find-bugs"';
    const skill = fixtureSkill({
      spec_violations: [mismatch],
      deployments: [fixtureDeployment({ spec_violations: [mismatch] })],
    });
    expect(rowState(skill)?.level).toBe("warning");
    expect(groupOf(skill)).toBe("warning");
  });
});

describe("rowState rollup note", () => {
  it("shows the counted rollup as a phrase, or the row note reads 'inside: · …'", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({ is_symlink: true, symlink_is_broken: true }),
        fixtureDeployment({
          id: "dep:v1/global/codex/find-bugs",
          agent: "codex",
          destination: "per-harness",
          path: "/home/.codex/skills/find-bugs",
          content_hash: "def",
        }),
      ],
    });
    const tip = skillRollup(skill, buildScopeGroups(skill)).tip;
    expect(tip.split("\n")[0]).toMatch(/inside:$/);
    const label = rowState(skill)?.label ?? "";
    expect(label).not.toBe("");
    expect(label).not.toMatch(/:$/);
  });
});

describe("rowState fix button", () => {
  it("shows no fix button for a left-behind copy, whose choice lives on the page the row click opens", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment(),
        fixtureDeployment({
          id: "dep:v1/parked/find-bugs",
          scope: "parked",
          agent: "parked",
          path: "/home/.agents/skills-parked/universal/find-bugs",
          parked_origin: { kind: "universal", scope: "global", project_path: null },
        }),
      ],
    });
    const state = rowState(skill);
    expect(state?.level).toBe("error");
    expect(state?.action).toBeNull();
  });

  it("names the copy in a drift note, or the row reads '1 differs'", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment(),
        fixtureDeployment({
          id: "dep:v1/global/codex/find-bugs",
          agent: "codex",
          destination: "per-harness",
          path: "/home/.codex/skills/find-bugs",
          content_hash: "def",
        }),
      ],
    });
    const state = rowState(skill);
    expect(state?.label).toBe("1 copy differs");
    expect(state?.action).toBe("Compare");
  });
});

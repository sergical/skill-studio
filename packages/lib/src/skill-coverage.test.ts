// ============================================================================
// Skill Studio - skill-coverage tests
// ============================================================================

import fc from "fast-check";
import { describe, expect, it } from "vitest";
import {
  AGENT_MATRIX_LABELS,
  agentMatrix,
  agentIdFromDeploymentLabel,
  deploymentLabelFromAgentId,
  driftingCopies,
  locationSummary,
  pickCompareDefaults,
  resolveCompareSelection,
  skillVisibleToAgent,
} from "./skill-coverage";
import { INSTALL_HARNESSES } from "./skill-install-destination";
import type { Deployment, InstalledSkill } from "./skill-types";

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

describe("pickCompareDefaults", () => {
  it("picks the strict-majority copy as the left side", () => {
    const majority1 = fixtureDeployment({ agent: "shared", content_hash: "aaa" });
    const majority2 = fixtureDeployment({ agent: "Claude Code", content_hash: "aaa" });
    const minority = fixtureDeployment({ agent: "Cursor", content_hash: "bbb" });
    const { left, right } = pickCompareDefaults([majority1, majority2, minority]);
    expect(left).toBe(majority1);
    expect(right).toBe(minority);
  });

  it("falls back to the first candidate with no strict majority", () => {
    const first = fixtureDeployment({ agent: "shared", content_hash: "aaa" });
    const second = fixtureDeployment({ agent: "Cursor", content_hash: "bbb" });
    const { left, right } = pickCompareDefaults([first, second]);
    expect(left).toBe(first);
    expect(right).toBe(second);
  });

  it("returns undefined for both sides with no candidates", () => {
    expect(pickCompareDefaults([])).toEqual({ left: undefined, right: undefined });
  });
});

describe("resolveCompareSelection", () => {
  it("keeps the current selection when it is still a candidate", () => {
    const kept = fixtureDeployment({ path: "/home/.claude/skills/find-bugs" });
    const other = fixtureDeployment({ agent: "Codex", path: "/home/.codex/skills/find-bugs" });
    expect(resolveCompareSelection(kept.path, [kept, other], other.path)).toBe(kept.path);
  });

  it("falls back to the default pick when the current selection no longer exists", () => {
    const remaining = fixtureDeployment({ agent: "Codex", path: "/home/.codex/skills/find-bugs" });
    expect(
      resolveCompareSelection("/home/.claude/skills/find-bugs", [remaining], remaining.path),
    ).toBe(remaining.path);
  });

  it("treats an unset selection as gone, using the fallback", () => {
    const remaining = fixtureDeployment({ path: "/home/.claude/skills/find-bugs" });
    expect(resolveCompareSelection(undefined, [remaining], remaining.path)).toBe(remaining.path);
  });
});

describe("agent label <-> id", () => {
  it("round-trips every first-class agent label through its id, or a chip and its filter name different agents", () => {
    for (const label of AGENT_MATRIX_LABELS) {
      const id = agentIdFromDeploymentLabel(label);
      expect(id, label).not.toBeNull();
      expect(deploymentLabelFromAgentId(id ?? ""), label).toBe(label);
    }
  });

  it("round-trips every installable harness id through its label", () => {
    for (const { id } of INSTALL_HARNESSES) {
      expect(agentIdFromDeploymentLabel(deploymentLabelFromAgentId(id)), id).toBe(id);
    }
  });

  it("maps an unknown label to no agent and shows an unknown id as itself", () => {
    const known = new Set<string>([...AGENT_MATRIX_LABELS, "shared"]);
    const knownIds = new Set<string>(INSTALL_HARNESSES.map((h) => h.id));
    fc.assert(
      fc.property(fc.string(), (text) => {
        if (!known.has(text)) expect(agentIdFromDeploymentLabel(text)).toBeNull();
        if (!knownIds.has(text)) expect(deploymentLabelFromAgentId(text)).toBe(text);
      }),
    );
  });
});

describe("locationSummary", () => {
  it("puts the Universal deployment in truth", () => {
    const skill = fixtureSkill({
      deployments: [fixtureDeployment({ agent: "shared", path: "/home/.agents/skills/find-bugs" })],
    });
    expect(locationSummary(skill).truth?.agent).toBe("shared");
  });

  it("groups a symlink into the Universal root as a link", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({
          agent: "Codex",
          is_symlink: true,
          symlink_target: "/home/.agents/skills/find-bugs",
        }),
      ],
    });
    const summary = locationSummary(skill);
    expect(summary.links.map((d) => d.agent)).toEqual(["Codex"]);
    expect(summary.truth).toBeNull();
  });

  it("groups a real (non-symlink) own directory as a copy", () => {
    const skill = fixtureSkill({
      deployments: [fixtureDeployment({ agent: "Claude Code", is_symlink: false })],
    });
    expect(locationSummary(skill).copies.map((d) => d.agent)).toEqual(["Claude Code"]);
  });

  it("groups a healthy symlink outside the Universal root as a copy", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({
          agent: "Claude Code",
          is_symlink: true,
          symlink_target: "/home/src/skills-repo/find-bugs",
        }),
      ],
    });
    expect(locationSummary(skill).copies.map((d) => d.agent)).toEqual(["Claude Code"]);
  });

  it("groups an unresolved symlink as broken", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({ agent: "Claude Code", is_symlink: true, symlink_is_broken: true }),
      ],
    });
    expect(locationSummary(skill).broken.map((d) => d.agent)).toEqual(["Claude Code"]);
  });

  it("groups a real (non-symlink) directory reached through a linked root as a link, not a copy", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({
          agent: "Claude Code",
          is_symlink: false,
          resolved_path: "/home/.agents/skills/find-bugs",
        }),
      ],
    });
    const summary = locationSummary(skill);
    expect(summary.links.map((d) => d.agent)).toEqual(["Claude Code"]);
    expect(summary.copies).toEqual([]);
  });
});

describe("driftingCopies", () => {
  it("finds no drift when copies share the truth's content hash", () => {
    const truth = fixtureDeployment({ agent: "shared", content_hash: "abc" });
    const copy = fixtureDeployment({ agent: "Codex", content_hash: "abc" });
    expect(driftingCopies({ truth, links: [], copies: [copy], broken: [] })).toEqual([]);
  });

  it("keeps the global Universal folder as truth and compares a project one as a copy", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({
          agent: "shared",
          scope: "project",
          project_path: "/src/web",
          content_hash: "bbb",
        }),
        fixtureDeployment({ agent: "shared", scope: "global", content_hash: "aaa" }),
      ],
    });
    const summary = locationSummary(skill);
    expect(summary.truth?.scope).toBe("global");
    expect(driftingCopies(summary).map((d) => d.scope)).toEqual(["project"]);
  });

  it("flags a copy whose content hash differs from the truth's", () => {
    const truth = fixtureDeployment({ agent: "shared", content_hash: "abc" });
    const copy = fixtureDeployment({ agent: "Codex", content_hash: "xyz" });
    expect(driftingCopies({ truth, links: [], copies: [copy], broken: [] })).toEqual([copy]);
  });
});

describe("disabled shared-root readers", () => {
  it("reports a disabled reader as not visible in the coverage matrix", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({
          agent: "shared",
          path: "/home/.agents/skills/find-bugs",
          disabled_readers: ["opencode"],
        }),
      ],
    });

    expect(skillVisibleToAgent(skill, "opencode")).toBe("none");
    expect(skillVisibleToAgent(skill, "codex")).toBe("shared");
    expect(agentMatrix([skill])[0]?.cells.OpenCode.state).toBe("none");
    expect(agentMatrix([skill])[0]?.cells.Codex.state).toBe("shared");
  });

  it("prefers a healthy own deployment over a disabled shared-root reader", () => {
    const skill = fixtureSkill({
      deployments: [
        fixtureDeployment({
          agent: "shared",
          path: "/home/.agents/skills/find-bugs",
          disabled_readers: ["opencode"],
        }),
        fixtureDeployment({
          agent: "OpenCode",
          path: "/home/.config/opencode/skills/find-bugs",
        }),
      ],
    });

    expect(skillVisibleToAgent(skill, "opencode")).toBe("own");
    expect(agentMatrix([skill])[0]?.cells.OpenCode.state).toBe("own");
  });
});

// ============================================================================
// Skill Studio - skill-location-status tests
// ============================================================================

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { TooltipProvider } from "@skill-studio/ui";
import { findLeftBehindPairs } from "@skill-studio/lib";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";
import { SkillLocationScope } from "./SkillLocationScope";
import {
  buildInvocationFiles,
  buildScopeGroups,
  folderReaders,
  invocationFooterNote,
  parkActionFor,
  promoteToGlobal,
  rowMenu,
  siblingRows,
  skillRollup,
  titleLink,
} from "./skill-location-status";
import type { ScopeGroup } from "./skill-location-status";
import {
  perSkillLinkDeployment,
  realCopyDeployment,
  universalDeployment,
  wholeFolderDeployment,
  withScannerIdentity,
} from "../../dev/harness/scanned-deployment";

/**
 * A Global Universal row, with `overrides` applied and then `id`,
 * `destination`, and `backing` derived from the result the way the scanner
 * derives them - so a test can move a row to another harness or scope and
 * still get a shape the scanner produces.
 */
function fixtureDeployment(overrides: Partial<Deployment> = {}): Deployment {
  return withScannerIdentity({
    ...universalDeployment(
      { universalPath: "/home/.agents/skills/find-bugs" },
      {
        owner_kind: "manual",
        mutability: "read-only",
        content_hash: "abc",
      },
    ),
    ...overrides,
  });
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
    description: null,
    fork: null,
    parked_at: null,
    skill_path: null,
    source_url: null,
    update_commit: null,
    update_commit_at: null,
    updated_at: null,
    ...overrides,
    update_owner_ids:
      overrides.update_owner_ids ?? (overrides.has_update ? ["owner:v1/global/find-bugs"] : []),
  };
}

/** A parked copy of the Universal folder, overridable per test. */
function parkedCopy(overrides: Partial<Deployment> = {}): Deployment {
  return fixtureDeployment({
    scope: "parked",
    agent: "parked",
    path: "/home/.agents/skills-parked/universal/find-bugs",
    parked_origin: { kind: "universal", scope: "global", project_path: null },
    ...overrides,
  });
}

/** One scope block as the card renders it. */
function renderGroup(group: ScopeGroup): string {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(SkillLocationScope, {
        group,
        showEyebrow: false,
        onAction: () => Promise.resolve(true),
      }),
    ),
  );
}

describe("buildScopeGroups", () => {
  it("flags a broken link with the error dot and its relink/remove menu", () => {
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_is_broken: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), claude] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((r) => r.harness === "claude-code");
    expect(row?.level).toBe("error");
    expect(row?.conditions[0].what).toBe("Broken link. The target is missing.");
    const menu = rowMenu(row!, global.label);
    expect(menu.entries.map((e) => e.label)).toContain("Relink to the folder");
    expect(menu.danger.map((e) => e.label)).toContain("Remove broken link");
  });

  it("flags an unreadable link distinctly from a broken one", () => {
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_error: "permission denied",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), claude] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((r) => r.harness === "claude-code");
    expect(row?.conditions[0].what).toBe("Link cannot be read: permission denied.");
    expect(row?.conditions[0].status).toBe("Link unreadable");
  });

  it("treats a missing required field as blocking (error) and says some agents skip it, or the row claims no agent loads a skill Claude Code still loads", () => {
    const shared = fixtureDeployment({
      spec_violations: ["missing required frontmatter field: description"],
    });
    const skill = fixtureSkill({ deployments: [shared], has_spec: false });
    const [global] = buildScopeGroups(skill);
    expect(global.shared?.level).toBe("error");
    expect(global.shared?.conditions[0].status).toBe("Skipped by some agents");
    expect(global.shared?.conditions[0].what).toBe(
      "SKILL.md: SKILL.md has no description in its frontmatter. Codex, OpenCode, and pi skip it. Claude Code still loads it.",
    );
  });

  it("gives a notes-only copy no spec condition, so a length note never raises a dot", () => {
    const shared = fixtureDeployment({ spec_violations: ["description exceeds 1024 characters"] });
    const skill = fixtureSkill({ deployments: [shared] });
    const [global] = buildScopeGroups(skill);
    expect(global.shared?.conditions).toEqual([]);
  });

  it("treats a warning-severity violation as a soft warning that still loads", () => {
    const shared = fixtureDeployment({
      spec_violations: ['name "other" does not match its directory name "find-bugs"'],
    });
    const skill = fixtureSkill({ deployments: [shared] });
    const [global] = buildScopeGroups(skill);
    expect(global.shared?.level).toBe("warning");
    expect(global.shared?.conditions[0].what).toContain("The skill still loads.");
  });

  it("flags a copy that drifted from the Universal deployment", () => {
    const shared = fixtureDeployment({ content_hash: "aaa" });
    const codexCopy = fixtureDeployment({
      agent: "Codex",
      scope: "project",
      project_path: "/repo",
      path: "/repo/.codex/skills/find-bugs",
      content_hash: "zzz",
    });
    const skill = fixtureSkill({ deployments: [shared, codexCopy] });
    const groups = buildScopeGroups(skill);
    const project = groups.find((g) => !g.isGlobal)!;
    const row = project.rows.find((r) => r.harness === "codex");
    expect(row?.level).toBe("warning");
    expect(row?.conditions[0].what).toBe("This copy differs from the Universal folder.");
  });

  it("treats a whole-root link as a plain link row with no agent switch", () => {
    const shared = fixtureDeployment();
    const claude = wholeFolderDeployment({
      agent: "Claude Code",
      path: "/home/.claude/skills/find-bugs",
      universalPath: "/home/.agents/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((r) => r.harness === "claude-code");
    expect(row?.kind).toBe("link");
    expect(row?.level).toBe(null);
    expect(row?.hasSwitch).toBe(false);
    expect(row?.conditions).toHaveLength(0);
  });

  it.each([
    ["codex-config", "Hidden by Codex setting — switched off in ~/.codex/config.toml."],
    [
      "opencode-permission",
      "Hidden by OpenCode setting — denied in ~/.config/opencode/opencode.json.",
    ],
    [
      "claude-skill-overrides",
      "Hidden by Claude Code setting — switched off in ~/.claude/settings.json.",
    ],
    ["claude-link-removed", "Off for Claude Code — the link under ~/.claude/skills was removed."],
    ["studio-moved", "Off for pi — moved into .skill-studio-disabled."],
  ] as const)("reports the %s off mode with its own sentence", (disabledBy, expectedWhat) => {
    const agentLabel =
      disabledBy === "studio-moved"
        ? "pi"
        : disabledBy === "codex-config"
          ? "Codex"
          : disabledBy === "opencode-permission"
            ? "OpenCode"
            : "Claude Code";
    const shared = fixtureDeployment();
    const row = fixtureDeployment({
      agent: agentLabel,
      is_symlink: agentLabel === "Claude Code",
      symlink_target: agentLabel === "Claude Code" ? "/home/.agents/skills/find-bugs" : null,
      disabled: true,
      disabled_by: disabledBy,
      path: `/home/.${agentLabel.toLowerCase()}/skills/find-bugs`,
      disabling_config_files: [
        { agent: "codex", path: "/Users/dev/.codex/config.toml" },
        { agent: "open-code", path: "/Users/dev/.config/opencode/opencode.json" },
        { agent: "claude-code", path: "/Users/dev/.claude/settings.json" },
      ],
    });
    const skill = fixtureSkill({ deployments: [shared, row] });
    const [global] = buildScopeGroups(skill);
    const found = global.rows.find((r) => r.harnessLabel === agentLabel);
    expect(found?.level).toBe("off");
    expect(found?.conditions[0].what).toBe(expectedWhat);
  });

  // Flow: one copy is live, another parked. Failure caught: the skill reads as parked as a whole,
  // its live copy loses the Park button, or the parked copy hides inside the live rows.
  it("keeps a live copy live and gives the parked copy its own Turn on row", () => {
    const claude = fixtureDeployment({
      agent: "Claude Code",
      path: "/home/.claude/skills/find-bugs",
    });
    const parked = parkedCopy({
      path: "/home/.agents/skills-parked/codex/find-bugs",
      parked_origin: { kind: "codex", scope: "global", project_path: null },
    });
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), claude, parked] });
    const [global] = buildScopeGroups(skill);

    expect(global.shared?.switchOn).toBe(true);
    expect(global.rows.map((r) => r.kind)).not.toContain("parked");
    expect(global.parked).toHaveLength(1);
    expect(global.parked[0]).toMatchObject({
      kind: "parked",
      harnessLabel: "Codex",
      level: "off",
      leftBehind: null,
    });
    expect(rowMenu(global.parked[0], global.label).entries[0]).toMatchObject({
      label: "Turn on",
      action: { kind: "unpark", deployment: parked },
    });
    expect(skillRollup(skill, [global]).level).toBeNull();
    expect(parkActionFor(global.shared!, global.label, null)).toMatchObject({ kind: "park" });
    expect(
      parkActionFor(
        global.rows.find((r) => r.harness === "claude-code")!,
        "Global",
        null,
      ),
    ).toMatchObject({
      kind: "park",
      deployment: claude,
    });
  });

  // Flow: every copy is parked. Failure caught: no row offers Turn on, or the card still draws a live folder.
  it("shows a fully parked skill as one off switch row and no live rows", () => {
    const parked = parkedCopy();
    const skill = fixtureSkill({ deployments: [parked], parked: true });
    const [global] = buildScopeGroups(skill);

    expect(global.shared).toBeNull();
    expect(global.rows).toEqual([]);
    expect(global.parked.map((r) => r.harnessLabel)).toEqual(["Universal folder"]);
    expect(skillRollup(skill, [global]).level).toBe("off");

    const markup = renderGroup(global);
    expect(markup).toContain("Turn on the parked");
    expect(markup.match(/role="switch"/g)).toHaveLength(1);
    expect(markup).toContain('aria-checked="false"');
    expect(buildInvocationFiles([global])).toEqual([]);
  });

  // Failure caught: the parked copy shows in the Invocation section, where a segmented control
  // would edit a SKILL.md that no agent reads.
  it("leaves parked copies out of the Invocation section", () => {
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), parkedCopy()] });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(files.map((f) => f.path)).toEqual(["/home/.agents/skills/find-bugs"]);
  });

  // Flow: a project copy was parked. Failure caught: it lands in the Global block and its Turn on
  // looks like it acts on a global copy.
  it("puts a parked project copy in the block of the project it came from", () => {
    const projectLive = fixtureDeployment({
      agent: "Codex",
      scope: "project",
      project_path: "/repo",
      path: "/repo/.codex/skills/find-bugs",
    });
    const parked = parkedCopy({
      path: "/home/.agents/skills-parked/project/find-bugs",
      parked_origin: { kind: "universal", scope: "project", project_path: "/repo" },
    });
    const groups = buildScopeGroups(fixtureSkill({ deployments: [projectLive, parked] }));
    expect(groups.find((g) => g.isGlobal)?.parked ?? []).toEqual([]);
    expect(groups.find((g) => g.projectPath === "/repo")?.parked).toHaveLength(1);
  });

  // Flow: a hand `mv` or install put a live folder back beside the parked one. Failure caught:
  // the parked row offers Turn on, which core refuses because a copy sits at the origin.
  it("flags a parked copy with a live copy at its origin and offers the two fixes instead of Turn on", () => {
    const live = fixtureDeployment();
    const parked = parkedCopy();
    const skill = fixtureSkill({ deployments: [live, parked] });
    const [global] = buildScopeGroups(skill);
    const row = global.parked[0];

    expect(row.level).toBe("error");
    expect(row.conditions[0].status).toBe("Left behind");
    expect(row.leftBehind).toEqual({ live, parked });
    expect(rowMenu(row, global.label).entries.map((e) => e.label)).toEqual([
      "Keep live",
      "Keep parked",
      "Reveal in Finder",
    ]);
    expect(skillRollup(skill, [global]).level).toBe("error");

    const markup = renderGroup(global);
    expect(markup).toContain("Keep live");
    expect(markup).toContain("Keep parked");
    expect(markup).not.toContain("Turn on");
  });

  // Failure caught: the live copy of a left-behind pair keeps a Park button, a second way to
  // act on the pair that skips the parked copy.
  it("offers no Park on the live copy of a left-behind pair", () => {
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), parkedCopy()] });
    const [global] = buildScopeGroups(skill);

    expect(global.shared?.leftBehindLive).toBe(true);
    expect(parkActionFor(global.shared!, global.label, null)).toBeNull();
  });

  // Failure caught: a parked project copy pairs with a live copy in another project or in
  // Global, so Keep live would delete the wrong folder.
  it("pairs a parked project copy only with a live copy in the same project", () => {
    const otherProject = fixtureDeployment({
      scope: "project",
      project_path: "/other",
      path: "/other/.agents/skills/find-bugs",
    });
    const parked = parkedCopy({
      path: "/home/.agents/skills-parked/project/find-bugs",
      parked_origin: { kind: "universal", scope: "project", project_path: "/repo" },
    });
    const apart = fixtureSkill({ deployments: [fixtureDeployment(), otherProject, parked] });
    expect(findLeftBehindPairs(apart)).toEqual([]);

    const sameProject = fixtureDeployment({
      scope: "project",
      project_path: "/repo",
      path: "/repo/.agents/skills/find-bugs",
    });
    const together = fixtureSkill({ deployments: [fixtureDeployment(), sameProject, parked] });
    expect(findLeftBehindPairs(together)).toEqual([{ live: sameProject, parked }]);
  });

  // Failure caught: Park shows on a plugin copy or a link, which core refuses; or a project row
  // loses its project path and skips the git confirm.
  it("offers Park only on real copies and carries the project path for the confirm", () => {
    const shared = fixtureDeployment({
      scope: "project",
      project_path: "/repo",
      path: "/repo/.agents/skills/find-bugs",
    });
    const link = fixtureDeployment({
      agent: "Claude Code",
      scope: "project",
      project_path: "/repo",
      is_symlink: true,
      symlink_target: "/repo/.agents/skills/find-bugs",
      path: "/repo/.claude/skills/find-bugs",
    });
    const [project] = buildScopeGroups(fixtureSkill({ deployments: [shared, link] }));
    expect(parkActionFor(project.shared!, project.label, "/repo")).toMatchObject({
      kind: "park",
      projectPath: "/repo",
    });
    expect(
      parkActionFor(
        project.rows.find((r) => r.kind === "link")!,
        project.label,
        "/repo",
      ),
    ).toBeNull();
    expect(
      parkActionFor(
        project.rows.find((r) => r.kind === "reader")!,
        project.label,
        "/repo",
      ),
    ).toBeNull();
  });

  it("synthesizes reader rows for agents that read the Universal folder natively", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude] });
    const [global] = buildScopeGroups(skill);
    const pi = global.rows.find((r) => r.harness === "pi");
    expect(pi?.kind).toBe("reader");
    expect(pi?.hasSwitch).toBe(false);
    expect(pi?.switchOn).toBe(true);
    const codex = global.rows.find((r) => r.harness === "codex");
    expect(codex?.hasSwitch).toBe(false);
  });

  it("keeps broken link errors on their own rows, not the folder", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      symlink_is_broken: true,
      path: "/home/.claude/skills/find-bugs",
    });
    const codex = fixtureDeployment({
      agent: "Codex",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      symlink_is_broken: true,
      path: "/home/.codex/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude, codex] });
    const [global] = buildScopeGroups(skill);
    expect(global.folderLevel).toBe(null);
    expect(global.rows.find((r) => r.harness === "claude-code")?.level).toBe("error");
    expect(global.rows.find((r) => r.harness === "codex")?.level).toBe("error");
  });

  it("leaves_the_folder_dot_empty_when_only_agent_settings_hide_the_skill_or_names_the_dot_it_shows", () => {
    // Agent settings are read-only here. Only the shared-folder switch or a
    // park makes a folder all-off, so settings-hidden rows do not roll up.
    const shared = fixtureDeployment({ disabled_readers: ["open-code"] });
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      disabled: true,
      disabled_by: "claude-skill-overrides",
      path: "/home/.claude/skills/find-bugs",
    });
    const codex = fixtureDeployment({
      agent: "Codex",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      disabled: true,
      disabled_by: "codex-config",
      path: "/home/.codex/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude, codex] });
    const [global] = buildScopeGroups(skill);
    expect(global.folderLevel).toBe(null);
  });

  it("keeps_the_claude_code_row_visible_with_a_caption_and_no_switch_while_skill_overrides_has_it_off_or_names_the_layout", () => {
    const universalPath = "/home/.agents/skills/find-bugs";
    const path = "/home/.claude/skills/find-bugs";
    const off: Partial<Deployment> = {
      disabled: true,
      disabled_by: "claude-skill-overrides",
      disabling_config_files: [{ agent: "claude-code", path: "/Users/dev/.claude/settings.json" }],
    };
    const layouts = {
      "per-skill link": perSkillLinkDeployment({ agent: "Claude Code", path, universalPath }, off),
      "whole-folder link": wholeFolderDeployment(
        { agent: "Claude Code", path, universalPath },
        off,
      ),
      "real copy": realCopyDeployment({ agent: "Claude Code", path }, off),
    };
    for (const [layout, claude] of Object.entries(layouts)) {
      const [global] = buildScopeGroups(
        fixtureSkill({ deployments: [fixtureDeployment(), claude] }),
      );
      const row = global.rows.find((r) => r.harness === "claude-code");
      expect(row, `${layout}: the Claude Code row is gone while off`).toBeDefined();
      expect(row?.switchOn, `${layout}: the switch shows on`).toBe(false);
      expect(row?.hasSwitch, `${layout}: the row offers a switch that writes settings.json`).toBe(
        false,
      );
      expect(row?.caption, `${layout}: the caption is missing`).toBe(
        "Hidden by Claude Code setting",
      );
      expect(
        row?.conditions.map((c) => c.what),
        `${layout}: the off sentence is missing`,
      ).toContain("Hidden by Claude Code setting — switched off in ~/.claude/settings.json.");
    }
  });

  it("shows_a_not_linked_claude_code_row_for_a_universal_only_skill_with_no_switch_or_names_what_is_missing", () => {
    const shared = fixtureDeployment({ disabled_readers: ["claude-code"] });
    const [global] = buildScopeGroups(fixtureSkill({ deployments: [shared] }));

    const claudeRows = global.rows.filter((r) => r.harness === "claude-code");
    expect(claudeRows, "expected exactly one Claude Code row").toHaveLength(1);
    const [row] = claudeRows;
    expect(row).toMatchObject({
      kind: "reader",
      hasSwitch: false,
      switchOn: false,
      level: "off",
    });
    expect(row.conditions[0]?.status).toBe("Not linked");
    expect(rowMenu(row, global.label).entries.map((entry) => entry.action.kind)).toEqual([
      "reveal",
    ]);
  });

  it("shows_no_not_linked_row_when_a_whole_folder_link_already_gives_claude_code_the_skill_or_names_the_extra_row", () => {
    // Even a stale `disabled_readers` entry must not add a second Claude row
    // beside the one the whole-folder link already produces.
    const shared = fixtureDeployment({ disabled_readers: ["claude-code"] });
    const claude = wholeFolderDeployment({
      agent: "Claude Code",
      path: "/home/.claude/skills/find-bugs",
      universalPath: "/home/.agents/skills/find-bugs",
    });
    const [global] = buildScopeGroups(fixtureSkill({ deployments: [shared, claude] }));
    const claudeRows = global.rows.filter((r) => r.harness === "claude-code");
    expect(claudeRows.map((r) => r.kind)).toEqual(["link"]);
    expect(claudeRows[0].conditions.map((c) => c.status)).not.toContain("Not linked");
  });

  it("shows_no_not_linked_row_for_a_project_universal_skill_or_names_the_extra_row", () => {
    // Project-scope per-harness switches stay hidden in 0.1.0.
    const shared = fixtureDeployment({
      scope: "project",
      project_path: "/repo",
      path: "/repo/.agents/skills/find-bugs",
      disabled_readers: ["claude-code"],
    });
    const project = buildScopeGroups(fixtureSkill({ deployments: [shared] })).find(
      (g) => !g.isGlobal,
    )!;
    expect(project.rows.some((r) => r.harness === "claude-code")).toBe(false);
  });

  it("puts Claude Code beside the folder, not inside it", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude] });
    const [global] = buildScopeGroups(skill);
    expect(folderReaders(global).some((row) => row.harness === "claude-code")).toBe(false);
    expect(folderReaders(global).every((row) => row.kind === "reader")).toBe(true);
    const siblings = siblingRows(global);
    expect(siblings).toHaveLength(1);
    expect(siblings[0]).toMatchObject({ harness: "claude-code", kind: "link" });
  });

  it("a scope with only copies has no reader rows", () => {
    const cursorCopy = fixtureDeployment({
      agent: "Cursor",
      scope: "project",
      project_path: "/repo",
      is_symlink: false,
      path: "/repo/.cursor/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [cursorCopy] });
    const groups = buildScopeGroups(skill);
    const project = groups.find((g) => !g.isGlobal)!;
    expect(folderReaders(project)).toHaveLength(0);
    expect(siblingRows(project)).toHaveLength(1);
  });

  it("locations_card_offers_no_agent_switch_on_a_copy_row_or_names_the_row", () => {
    const shared = fixtureDeployment();
    const projectClaudeCopy = fixtureDeployment({
      id: "dep:v1/project/claude-code/find-bugs",
      agent: "Claude Code",
      scope: "project",
      project_path: "/repo",
      is_symlink: false,
      path: "/repo/.claude/skills/find-bugs",
    });
    const globalCodex = fixtureDeployment({
      id: "dep:v1/global/codex/find-bugs",
      agent: "Codex",
      scope: "global",
      is_symlink: false,
      path: "/home/.codex/skills/find-bugs",
    });
    const groups = buildScopeGroups(
      fixtureSkill({ deployments: [shared, projectClaudeCopy, globalCodex] }),
    );
    for (const group of groups) {
      for (const row of group.rows) {
        expect(row.hasSwitch, `${group.label}: ${row.harnessLabel} offers a switch`).toBe(false);
      }
    }
  });
});

describe("skillRollup", () => {
  it("reports a lock-only skill with no deployments as a warning", () => {
    const skill = fixtureSkill({ deployments: [] });
    const groups = buildScopeGroups(skill);
    const roll = skillRollup(skill, groups);
    expect(roll.level).toBe("warning");
    expect(roll.tip).toContain("Listed in the lock file");
  });

  it("prefixes every child line with its scope", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      symlink_is_broken: true,
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude] });
    const groups = buildScopeGroups(skill);
    const roll = skillRollup(skill, groups);
    expect(roll.tip).toContain("Global · Claude Code:");
  });
});

describe("titleLink", () => {
  it("prefers Compare copies over Install again and Update when there's drift", () => {
    const skill = fixtureSkill({ has_update: true });
    expect(titleLink(skill, true)).toBe("Compare copies");
  });

  // Failure caught: a title link comes back that unparks the whole skill, hiding which copy turns on.
  it("never offers an unpark link, because each parked row has its own Turn on", () => {
    const skill = fixtureSkill({ deployments: [parkedCopy()], parked: true, has_update: true });
    expect(titleLink(skill, false)).toBeNull();
  });

  it("prefers Install again for a lock-only skill", () => {
    const skill = fixtureSkill({ deployments: [], has_update: true });
    expect(titleLink(skill, false)).toBe("Install again");
  });

  it("leaves Update to the page header, so the card title never reads as updating locations", () => {
    const skill = fixtureSkill({ has_update: true });
    expect(titleLink(skill, false)).toBeNull();
  });

  it("returns null when there is nothing to fix", () => {
    const skill = fixtureSkill();
    expect(titleLink(skill, false)).toBeNull();
  });
});

describe("rowMenu", () => {
  it("does not duplicate switch controls with Disable menu actions", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      path: "/home/.claude/skills/find-bugs",
    });
    const [global] = buildScopeGroups(fixtureSkill({ deployments: [shared, claude] }));

    for (const row of global.rows) {
      const menu = rowMenu(row, global.label);
      const labels = [...menu.entries, ...menu.danger].map((entry) => entry.label);
      expect(labels).not.toContain(`Disable for ${row.harnessLabel}`);
    }
  });

  it("hides Remove copy on a per-agent copy row, or the menu offers a Remove the backend refuses", () => {
    const perAgent = realCopyDeployment(
      { agent: "Claude Code", path: "/home/.claude/skills/find-bugs" },
      { owner_kind: "copy", mutability: "mutable" },
    );
    const [group] = buildScopeGroups(
      fixtureSkill({ deployments: [fixtureDeployment(), perAgent] }),
    );
    const row = group.rows.find((candidate) => candidate.kind === "copy")!;
    const menu = rowMenu(row, group.label);

    expect([...menu.entries, ...menu.danger].map((entry) => entry.label)).not.toContain(
      "Remove Claude Code copy…",
    );
  });

  it("offers an independent copy only for a healthy enabled Universal-backed link", () => {
    const linked = perSkillLinkDeployment({
      agent: "Claude Code",
      path: "/home/.claude/skills/find-bugs",
      universalPath: "/home/.agents/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), linked] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((candidate) => candidate.harness === "claude-code")!;

    expect(rowMenu(row, global.label).entries.map((entry) => entry.label)).toContain(
      "Make independent copy",
    );

    for (const deployment of [
      { ...linked, disabled: true },
      { ...linked, symlink_is_broken: true },
      fixtureDeployment({
        agent: "Claude Code",
        is_symlink: true,
        symlink_target: "/home/src/find-bugs",
        path: "/home/.claude/skills/find-bugs",
      }),
      realCopyDeployment({ agent: "Claude Code", path: "/home/.claude/skills/find-bugs" }),
    ]) {
      const [scope] = buildScopeGroups(
        fixtureSkill({ deployments: [fixtureDeployment(), deployment] }),
      );
      const candidate = scope.rows.find((item) => item.harness === "claude-code")!;
      expect(rowMenu(candidate, scope.label).entries.map((entry) => entry.label)).not.toContain(
        "Make independent copy",
      );
    }
  });

  it("leads with the highest condition's first fix even when it is destructive", () => {
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_error: "permission denied",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [fixtureDeployment(), claude] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((r) => r.harness === "claude-code")!;
    const menu = rowMenu(row, global.label);
    expect(menu.entries[0].label).toBe("Remove link");
  });

  it("puts non-leading danger items after the plain entries, in their own bucket", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((r) => r.harness === "claude-code")!;
    const menu = rowMenu(row, global.label);
    expect(menu.entries.map((e) => e.danger ?? false)).not.toContain(true);
    expect(menu.danger.map((e) => e.label)).toContain("Remove link");
  });

  it("offers_open_config_file_for_a_row_hidden_by_a_codex_setting_or_names_the_missing_entry", () => {
    const shared = fixtureDeployment();
    const codex = fixtureDeployment({
      agent: "Codex",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      disabled: true,
      disabled_by: "codex-config",
      disabling_config_files: [{ agent: "codex", path: "/Users/dev/.codex/config.toml" }],
      path: "/home/.codex/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, codex] });
    const [global] = buildScopeGroups(skill);
    const row = global.rows.find((r) => r.harness === "codex")!;
    const menu = rowMenu(row, global.label);
    expect(menu.entries.map((entry) => entry.label)).toContain("Open config.toml");
  });

  it("offers disable and uninstall for a Claude Code plugin row", () => {
    const plugin = fixtureDeployment({
      agent: "Claude Code",
      scope: "plugin",
      mutability: "read-only",
      path: "/home/.claude/plugins/cache/anthropics/codex/1.0.6/skills/find-bugs",
      plugin: {
        name: "codex",
        harness: "Claude Code",
        version: "1.0.6",
        marketplace: "anthropics",
        id: "codex@anthropics",
      },
    });
    const [global] = buildScopeGroups(fixtureSkill({ deployments: [plugin] }));
    const row = global.rows.find((r) => r.kind === "plugin")!;
    const menu = rowMenu(row, global.label);

    expect(menu.entries.map((e) => e.label)).toContain("Disable the codex plugin for Claude Code");
    expect(menu.danger.map((e) => e.label)).toContain("Uninstall the codex plugin…");
    expect(menu.hint).toBe("Applies to every skill the codex plugin ships.");
  });

  it("offers Update the plugin only on a plugin row whose plugin has an update", () => {
    const plugin = fixtureDeployment({
      agent: "Claude Code",
      scope: "plugin",
      mutability: "read-only",
      path: "/home/.claude/plugins/cache/anthropics/codex/1.0.6/skills/find-bugs",
      plugin: {
        name: "codex",
        harness: "Claude Code",
        version: "1.0.5",
        marketplace: "anthropics",
        id: "codex@anthropics",
      },
    });
    const labelsFor = (update_owners: InstalledSkill["update_owners"]) => {
      const [global] = buildScopeGroups(fixtureSkill({ deployments: [plugin], update_owners }));
      const row = global.rows.find((r) => r.kind === "plugin")!;
      return rowMenu(row, global.label).entries.map((e) => e.label);
    };

    expect(labelsFor([])).not.toContain("Update the codex plugin");
    expect(
      labelsFor([
        {
          owner_id: "plugin:codex@anthropics",
          latest_commit: null,
          latest_commit_at: null,
          plugin_scope: "user",
        },
      ]),
    ).toContain("Update the codex plugin");
  });

  it("offers enable for a Claude Code plugin row disabled by claude-plugin-disabled", () => {
    const plugin = fixtureDeployment({
      agent: "Claude Code",
      scope: "plugin",
      mutability: "read-only",
      disabled: true,
      disabled_by: "claude-plugin-disabled",
      path: "/home/.claude/plugins/cache/anthropics/codex/1.0.6/skills/find-bugs",
      plugin: {
        name: "codex",
        harness: "Claude Code",
        version: "1.0.6",
        marketplace: "anthropics",
        id: "codex@anthropics",
      },
    });
    const [global] = buildScopeGroups(fixtureSkill({ deployments: [plugin] }));
    const row = global.rows.find((r) => r.kind === "plugin")!;
    const menu = rowMenu(row, global.label);

    expect(menu.entries.map((e) => e.label)).toContain("Enable the codex plugin for Claude Code");
  });

  it("offers no plugin actions and a /plugins hint for a Codex plugin row", () => {
    const plugin = fixtureDeployment({
      agent: "Codex",
      scope: "plugin",
      mutability: "read-only",
      path: "/home/.codex/plugins/cache/openai/openai-templates/1.0.0/skills/find-bugs",
      plugin: {
        name: "openai-templates",
        harness: "Codex",
        version: "1.0.0",
        marketplace: "openai",
        id: "openai-templates@openai",
      },
    });
    const [global] = buildScopeGroups(fixtureSkill({ deployments: [plugin] }));
    const row = global.rows.find((r) => r.kind === "plugin")!;
    const menu = rowMenu(row, global.label);

    const labels = menu.entries.map((e) => e.label);
    expect(labels.some((label) => label.includes("plugin for Codex"))).toBe(false);
    expect(menu.danger).toEqual([]);
    expect(menu.hint).toBe("Manage this plugin with /plugins inside Codex.");
  });
});

describe("buildInvocationFiles / invocationFooterNote", () => {
  it("builds one footer row per SKILL.md file, skipping per-skill links", () => {
    const shared = fixtureDeployment();
    const claude = fixtureDeployment({
      agent: "Claude Code",
      is_symlink: true,
      symlink_target: "/home/.agents/skills/find-bugs",
      path: "/home/.claude/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, claude] });
    const groups = buildScopeGroups(skill);
    const files = buildInvocationFiles(groups);
    expect(files).toHaveLength(1);
    expect(files[0].kind).toBe("shared");
  });

  it("explains the single file's own invocation value", () => {
    const shared = fixtureDeployment({ invocation: "user-only" });
    const skill = fixtureSkill({ deployments: [shared], invocation: "user-only" });
    const groups = buildScopeGroups(skill);
    const files = buildInvocationFiles(groups);
    expect(invocationFooterNote(files, skill.name)).toBe("User only: only /find-bugs starts it.");
  });

  it("points at each-file-sets-its-own with more than one file", () => {
    const shared = fixtureDeployment();
    const codexCopy = fixtureDeployment({
      agent: "Codex",
      scope: "project",
      project_path: "/repo",
      path: "/repo/.codex/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [shared, codexCopy] });
    const groups = buildScopeGroups(skill);
    const files = buildInvocationFiles(groups);
    expect(files).toHaveLength(2);
    expect(invocationFooterNote(files, skill.name)).toBe(
      "All locations sets every file; a file can still differ. Symlinks follow the folder they point to.",
    );
  });
});

describe("buildInvocationFiles editability", () => {
  it("keeps the global Universal folder editable even when the skill is managed", () => {
    const shared = fixtureDeployment();
    const skill = fixtureSkill({ deployments: [shared], source_kind: "dotagents" });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(files[0]).toMatchObject({ kind: "shared", editable: true });
  });

  it("disables a managed project Universal folder", () => {
    const projectShared = fixtureDeployment({
      scope: "project",
      project_path: "/repo",
      path: "/repo/.agents/skills/find-bugs",
      owner_kind: "skills-sh",
    });
    const skill = fixtureSkill({ deployments: [projectShared], source_kind: "skills-sh" });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(files[0]).toMatchObject({ kind: "shared", editable: false });
    expect(files[0].disabledReason).toContain("skills.sh");
  });

  it("disables a managed copy", () => {
    const copy = fixtureDeployment({
      agent: "Cursor",
      scope: "project",
      project_path: "/repo",
      is_symlink: false,
      path: "/repo/.cursor/skills/find-bugs",
      owner_kind: "dotagents",
    });
    const skill = fixtureSkill({ deployments: [copy], source_kind: "dotagents" });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(files[0]).toMatchObject({ kind: "copy", editable: false });
    expect(files[0].disabledReason).toContain("dotagents");
  });

  it("keeps_an_ambiguous_copy_editable_even_though_the_skill_reads_as_dotagents", () => {
    const copy = fixtureDeployment({
      agent: "Cursor",
      scope: "project",
      project_path: "/repo",
      is_symlink: false,
      path: "/repo/.cursor/skills/find-bugs",
      owner_kind: "ambiguous",
    });
    const skill = fixtureSkill({ deployments: [copy], source_kind: "dotagents" });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(
      files[0],
      "no ledger row owns it, so nothing upstream would overwrite an edit",
    ).toMatchObject({
      kind: "copy",
      editable: true,
    });
  });

  it("keeps a manual copy editable in place", () => {
    const copy = fixtureDeployment({
      agent: "Cursor",
      scope: "project",
      project_path: "/repo",
      is_symlink: false,
      path: "/repo/.cursor/skills/find-bugs",
    });
    const skill = fixtureSkill({ deployments: [copy], source_kind: "manual" });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(files[0]).toMatchObject({ kind: "copy", editable: true });
  });

  it("disables a plugin file regardless of the skill's own source_kind", () => {
    const plugin = fixtureDeployment({
      agent: "Codex",
      scope: "plugin",
      is_symlink: false,
      path: "/home/.codex/plugins/cache/foo/skills/find-bugs",
      plugin: {
        name: "openai-templates",
        harness: "Codex",
        version: null,
        marketplace: "openai",
        id: "openai-templates@openai",
      },
    });
    const skill = fixtureSkill({ deployments: [plugin], source_kind: "manual" });
    const files = buildInvocationFiles(buildScopeGroups(skill));
    expect(files[0]).toMatchObject({ kind: "plugin", editable: false });
    expect(files[0].disabledReason).toContain("openai-templates");
  });
});

describe("promoteToGlobal", () => {
  const projectShared = (project: string) =>
    fixtureDeployment({
      scope: "project",
      project_path: project,
      path: `${project}/.agents/skills/find-bugs`,
    });
  const projectClaudeLink = (project: string) =>
    fixtureDeployment({
      agent: "Claude Code",
      scope: "project",
      project_path: project,
      is_symlink: true,
      symlink_target: `${project}/.agents/skills/find-bugs`,
      path: `${project}/.claude/skills/find-bugs`,
    });

  it("offers the first project's folder when two projects have it and Global does not", () => {
    const skill = fixtureSkill({
      deployments: [
        projectShared("/repo-a"),
        projectClaudeLink("/repo-a"),
        projectShared("/repo-b"),
      ],
    });
    expect(promoteToGlobal(buildScopeGroups(skill))).toEqual({
      path: "/repo-a/.agents/skills/find-bugs",
      agents: ["claude-code"],
    });
  });

  it("offers nothing when a global copy already exists", () => {
    const skill = fixtureSkill({
      deployments: [fixtureDeployment(), projectShared("/repo-a"), projectShared("/repo-b")],
    });
    expect(promoteToGlobal(buildScopeGroups(skill))).toBe(null);
  });

  it("offers nothing for a single project", () => {
    const skill = fixtureSkill({ deployments: [projectShared("/repo-a")] });
    expect(promoteToGlobal(buildScopeGroups(skill))).toBe(null);
  });
});

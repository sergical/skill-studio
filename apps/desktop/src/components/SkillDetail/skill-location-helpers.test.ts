// ============================================================================
// Skill Studio - skill location helper tests
// ============================================================================

import { describe, expect, it } from "vitest";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";
import { gitWarningText, leftBehindFix } from "./skill-location-helpers";
import { buildScopeGroups, rowMenu } from "./skill-location-status";
import { universalDeployment, withScannerIdentity } from "../../dev/harness/scanned-deployment";

/** A Global Universal row with `overrides`; id, destination, and backing follow the scanner. */
function sharedDeployment(overrides: Partial<Deployment> = {}): Deployment {
  return withScannerIdentity({
    ...universalDeployment(
      { universalPath: "/home/.agents/skills/find-bugs" },
      { owner_kind: "manual", mutability: "read-only", content_hash: "abc" },
    ),
    ...overrides,
  });
}

function skillWithDeployments(deployments: Deployment[]): InstalledSkill {
  return {
    name: "find-bugs",
    source: "local",
    source_type: "local",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: false,
    source_kind: "manual",
    deployments,
    has_spec: true,
    spec_violations: [],
    skill_md_tokens: 0,
    description_tokens: 0,
    folder_bytes: 0,
    file_count: 0,
    content_hash: "abc",
    content_hashes: ["abc"],
    frontmatter_fields: {},
    folder_truncated: false,
    parked: false,
    invocation: "both",
    update_owner_ids: [],
    update_owners: [],
    description: null,
    fork: null,
    parked_at: null,
    skill_path: null,
    source_url: null,
    update_commit: null,
    update_commit_at: null,
    updated_at: null,
  };
}

describe("gitWarningText", () => {
  // Flow: park or remove a project copy that git tracks. Failure caught: the confirm
  // stays silent and the user finds deleted files in the repository afterwards.
  it("warns that the move shows as deleted files when git tracks the folder", () => {
    expect(gitWarningText(true, "parking")).toBe(
      "Git tracks this folder, so parking it shows as deleted files in your repository.",
    );
  });

  // Failure caught: an unknown answer reads as "safe", or the confirm hides that the check failed.
  it("says plainly that the check could not run when the answer is null", () => {
    expect(gitWarningText(null, "removing")).toBe(
      "Couldn't check whether git tracks this folder. If it does, removing it shows as deleted files in your repository.",
    );
  });

  // Failure caught: a warning shows for an untracked folder and trains users to ignore it.
  it("shows no warning when git does not track the folder", () => {
    expect(gitWarningText(false, "parking")).toBeNull();
  });
});

describe("leftBehindFix", () => {
  const live = sharedDeployment({ path: "/home/.agents/skills/find-bugs" });
  const parked = withScannerIdentity({
    ...sharedDeployment(),
    scope: "parked",
    agent: "parked",
    path: "/home/.agents/skills-parked/universal/find-bugs",
    parked_origin: { kind: "universal", scope: "global", project_path: null },
  });

  // Failure caught: "Keep live" deletes the live copy, the only one agents read.
  it("deletes the parked copy for Keep live and keeps the live one", () => {
    const fix = leftBehindFix("keep-live", { live, parked });
    expect(fix.discard).toBe(parked);
    expect(fix.keep).toBe(live);
    expect(fix.checksRepository).toBe(false);
  });

  // Failure caught: "Keep parked" deletes the parked copy, the backup of the skill.
  it("deletes the live copy for Keep parked and keeps the parked one", () => {
    const fix = leftBehindFix("keep-parked", { live, parked });
    expect(fix.discard).toBe(live);
    expect(fix.keep).toBe(parked);
    expect(fix.checksRepository).toBe(false);
  });

  // Failure caught: deleting a live project copy gives no repository warning.
  it("checks the repository when Keep parked deletes a live project copy", () => {
    const projectLive = sharedDeployment({
      scope: "project",
      project_path: "/repo",
      path: "/repo/.agents/skills/find-bugs",
    });
    expect(leftBehindFix("keep-parked", { live: projectLive, parked }).checksRepository).toBe(true);
  });
});

describe("agent rows", () => {
  it("offers no per-agent switch on any row of a Global Universal skill, or names the row that does", () => {
    const groups = buildScopeGroups(skillWithDeployments([sharedDeployment()]));
    const global = groups.find((g) => g.isGlobal)!;
    const withSwitch = global.rows
      .filter((row) => row.kind !== "shared" && row.hasSwitch)
      .map((row) => row.harness);
    expect(withSwitch, "these agent rows still offer an on/off switch").toEqual([]);
  });

  it("captions a Codex reader row 'Hidden by Codex setting' and offers 'Open config.toml', or names what the row shows instead", () => {
    const groups = buildScopeGroups(
      skillWithDeployments([
        sharedDeployment({
          disabled_readers: ["codex"],
          disabling_config_files: [{ agent: "codex", path: "/custom/codex-home/config.toml" }],
        }),
      ]),
    );
    const codex = groups.find((g) => g.isGlobal)!.rows.find((r) => r.harness === "codex")!;
    expect(codex.caption).toBe("Hidden by Codex setting");
    expect(rowMenu(codex, "Global").entries[0]).toMatchObject({
      label: "Open config.toml",
      action: { kind: "open-editor", path: "/custom/codex-home/config.toml", label: "config.toml" },
    });
  });

  it("opens_the_global_settings_file_from_a_project_row_hidden_by_a_setting_or_names_the_path_it_opens", () => {
    const projectClaude = withScannerIdentity({
      ...sharedDeployment({
        agent: "Claude Code",
        scope: "project",
        project_path: "/proj",
        path: "/proj/.claude/skills/find-bugs",
        is_symlink: false,
        disabled: true,
        disabled_by: "claude-skill-overrides",
        disabling_config_files: [{ agent: "claude-code", path: "/home/.claude/settings.json" }],
      }),
    });
    const groups = buildScopeGroups(skillWithDeployments([projectClaude]));
    const row = groups.find((g) => !g.isGlobal)!.rows.find((r) => r.harness === "claude-code")!;
    expect(rowMenu(row, "Project").entries[0]).toMatchObject({
      label: "Open settings.json",
      action: { kind: "open-editor", path: "/home/.claude/settings.json" },
    });
  });
});

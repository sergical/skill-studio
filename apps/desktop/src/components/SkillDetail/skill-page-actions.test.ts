// ============================================================================
// Skill Studio - pullUpstreamToast tests
// ============================================================================

import { describe, expect, it } from "vitest";
import type { InstalledSkill, PullResult } from "@skill-studio/lib";

import { pullUpstreamToast } from "../../lib/skill-lifecycle-target";
import type { UpdateFinish } from "../../hooks/useGuardedSkillUpdate";
import { headerUpdateLabel, removeSuccessToast, runHeaderUpdate } from "./skill-page-actions";

function fixtureResult(overrides: Partial<PullResult> = {}): PullResult {
  return {
    from_commit: "aaa",
    to_commit: "bbb",
    merged: [],
    conflicts: [],
    added: [],
    removed: [],
    unchanged: 0,
    message: null,
    ...overrides,
  };
}

describe("pullUpstreamToast", () => {
  it("appends the F2 editor-open failure message to the conflict toast, or names the dropped message", () => {
    const result = fixtureResult({
      conflicts: ["skill.md"],
      message: "Conflict markers written to skill.md; could not open editor: no editor found",
    });

    const toast = pullUpstreamToast(result);

    expect(toast.type).toBe("warning");
    expect(toast.title).toBe("1 conflicts — open the editor to resolve");
    expect(toast.message).toBe(
      "skill.md Conflict markers written to skill.md; could not open editor: no editor found",
    );
  });

  it("shows the plain conflict list when the editor opened fine, or names the missing conflict path", () => {
    const result = fixtureResult({ conflicts: ["a.md", "b.md"] });

    const toast = pullUpstreamToast(result);

    expect(toast.message).toBe("a.md, b.md");
  });

  it("shows the already-up-to-date message when there are no conflicts, or names the lost message", () => {
    const result = fixtureResult({ message: "Already up to date" });

    const toast = pullUpstreamToast(result);

    expect(toast).toEqual({ type: "info", title: "Already up to date" });
  });

  it("counts merged, added, and removed files on a clean pull, or names the miscounted total", () => {
    const result = fixtureResult({ merged: ["a.md"], added: ["b.md"], removed: ["c.md"] });

    const toast = pullUpstreamToast(result);

    expect(toast).toEqual({ type: "success", title: "Updated 3 files" });
  });
});

describe("removeSuccessToast", () => {
  it("the_remove_success_toast_reads_removed_not_updated_n_deployments", () => {
    const toast = removeSuccessToast("find-bugs");

    expect(toast).toEqual({ type: "success", title: "Removed", message: "find-bugs" });
    expect(toast.title).not.toMatch(/updated/i);
    expect(toast.title).not.toMatch(/deployments/i);
  });
});

describe("headerUpdateLabel", () => {
  // The header is the page's only Update button, so a skill with an update must never lose it.
  it("offers Update for a plugin-kind skill whose skills.sh copy has an update", () => {
    expect(headerUpdateLabel({ source_kind: "plugin", update_owner_ids: ["owner-1"] })).toBe(
      "Update",
    );
  });

  it("offers Pull latest for a fork with an update", () => {
    expect(headerUpdateLabel({ source_kind: "fork", update_owner_ids: ["owner-1"] })).toBe(
      "Pull latest",
    );
  });

  it("offers Update for a fork whose only outdated owner is its plugin", () => {
    expect(
      headerUpdateLabel({ source_kind: "fork", update_owner_ids: ["plugin:codex@official"] }),
    ).toBe("Update");
  });

  it("offers Pull latest for a fork with both a managed owner and a plugin outdated", () => {
    expect(
      headerUpdateLabel({
        source_kind: "fork",
        update_owner_ids: ["plugin:codex@official", "owner-1"],
      }),
    ).toBe("Pull latest");
  });

  it("offers nothing when no owner has an update", () => {
    expect(headerUpdateLabel({ source_kind: "skills-sh", update_owner_ids: [] })).toBeNull();
  });
});

describe("runHeaderUpdate", () => {
  const owner = (
    id: string,
    extra: { plugin_scope?: string; plugin_project_path?: string | null } = {},
  ) => ({
    owner_id: id,
    latest_commit: "next",
    latest_commit_at: null,
    ...extra,
  });
  const withPlugin = (ownerIds: string[]) =>
    ({
      name: "codex",
      deployments: [],
      update_owner_ids: ["plugin:codex@official", ...ownerIds],
      update_owners: [
        owner("plugin:codex@official", { plugin_scope: "user", plugin_project_path: null }),
        ...ownerIds.map((id) => owner(id)),
      ],
    }) satisfies Pick<
      InstalledSkill,
      "name" | "deployments" | "update_owner_ids" | "update_owners"
    >;

  /** A guard whose overwrite dialog ends with `finish`, or never reports back when `null`. */
  const guardThatFinishes = (finish: UpdateFinish | null) => ({
    requestUpdate: async (
      _skill: ReturnType<typeof withPlugin>,
      options: { skipPlugins: boolean; onFinished: (finish: UpdateFinish) => Promise<void> },
    ) => {
      expect(options.skipPlugins).toBe(true);
      if (finish) await options.onFinished(finish);
    },
  });

  it("header_update_leaves_plugins_alone_when_the_overwrite_dialog_is_cancelled", async () => {
    const plugins: string[] = [];
    await runHeaderUpdate(
      withPlugin(["owner:v1/global/codex"]),
      guardThatFinishes(null),
      () => {},
      async (target) => {
        plugins.push(target.plugin_id);
        return { outcome: "updated", message: null };
      },
    );
    expect(plugins).toEqual([]);
  });

  it("header_update_runs_plugins_after_the_managed_copy_update_succeeds", async () => {
    const plugins: string[] = [];
    await runHeaderUpdate(
      withPlugin(["owner:v1/global/codex"]),
      guardThatFinishes({ success: true, error: "" }),
      () => {},
      async (target) => {
        plugins.push(target.plugin_id);
        return { outcome: "updated", message: null };
      },
    );
    expect(plugins).toEqual(["codex@official"]);
  });

  it("header_update_skips_plugins_when_the_managed_copy_update_fails", async () => {
    const plugins: string[] = [];
    await runHeaderUpdate(
      withPlugin(["owner:v1/global/codex"]),
      guardThatFinishes({ success: false, error: "boom" }),
      () => {},
      async (target) => {
        plugins.push(target.plugin_id);
        return { outcome: "updated", message: null };
      },
    );
    expect(plugins).toEqual([]);
  });

  it("header_update_runs_plugins_straight_away_when_there_is_no_managed_copy", async () => {
    const plugins: string[] = [];
    await runHeaderUpdate(
      withPlugin([]),
      {
        requestUpdate: async () => {
          throw new Error("no managed copy to request");
        },
      },
      () => {},
      async (target) => {
        plugins.push(target.plugin_id);
        return { outcome: "updated", message: null };
      },
    );
    expect(plugins).toEqual(["codex@official"]);
  });
});

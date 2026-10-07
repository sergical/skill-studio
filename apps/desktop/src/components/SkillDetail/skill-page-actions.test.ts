// ============================================================================
// Skill Studio - pullUpstreamToast tests
// ============================================================================

import { describe, expect, it } from "vitest";
import type { BulkTargetResult, Deployment, InstalledSkill, PullResult } from "@skill-studio/lib";

import { pullUpstreamToast } from "../../lib/skill-lifecycle-target";
import type { UpdateFinish } from "../../hooks/useGuardedSkillUpdate";
import {
  headerUpdateLabel,
  parkForEveryAgent,
  removeSuccessToast,
  runHeaderUpdate,
} from "./skill-page-actions";

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

describe("parkForEveryAgent", () => {
  const folder = (id: string, overrides: Partial<Deployment> = {}): Deployment => ({
    id,
    destination: "universal",
    owner_kind: "skills-sh",
    mutability: "mutable",
    backing: { kind: "canonical" },
    agent: "Universal",
    scope: "global",
    path: `/home/u/.agents/skills/${id}`,
    is_symlink: false,
    symlink_is_broken: false,
    content_hash: "x",
    disabled: false,
    codex_implicit_invocation: null,
    disabled_by: null,
    invocation: "both",
    spec_violations: [],
    shared_via_whole_dir_link: false,
    ...overrides,
  });
  const skill = (parked: boolean, deployments: Deployment[]) =>
    // SAFETY: parkForEveryAgent reads only name, parked and deployments.
    ({ name: "tidy", parked, source_kind: "skills-sh", deployments }) as InstalledSkill;
  const fakeApi = (
    errors: (string | null)[] = [],
    { confirm = true, gitTracked = false }: { confirm?: boolean; gitTracked?: boolean | null } = {},
  ) => {
    const calls: { kind: "park" | "unpark"; ids: string[] }[] = [];
    const prompts: string[] = [];
    const respond =
      (kind: "park" | "unpark") =>
      async (targets: { deployment_id?: string | null }[]): Promise<BulkTargetResult[]> => {
        calls.push({ kind, ids: targets.map((target) => target.deployment_id ?? "") });
        return targets.map((_, i) => ({ error: errors[i] ?? null }));
      };
    const api = {
      parkSkills: respond("park"),
      unparkSkills: respond("unpark"),
      parkCheck: async () => ({ git_tracked: gitTracked, project: "/home/u/web" }),
      ask: async (message: string) => {
        prompts.push(message);
        return confirm;
      },
    };
    return { calls, prompts, api };
  };

  it("park_for_every_agent_parks_the_separate_agent_copy_too_not_only_the_shared_folder", async () => {
    const shared = folder("shared");
    const codexCopy = folder("codex-copy", {
      destination: "per-harness",
      agent: "Codex",
      backing: { kind: "independent" },
      path: "/home/u/.codex/skills/tidy",
    });
    const claudeLink = folder("claude-link", {
      destination: "per-harness",
      agent: "Claude Code",
      backing: { kind: "linked-to", deployment_id: "shared" },
      is_symlink: true,
      path: "/home/u/.claude/skills/tidy",
    });
    const { calls, api } = fakeApi();

    const toast = await parkForEveryAgent(skill(false, [shared, codexCopy, claudeLink]), api);

    expect(calls).toEqual([{ kind: "park", ids: ["shared", "codex-copy"] }]);
    expect(toast).toEqual({ type: "success", title: "Parked tidy" });
  });

  it("turn_on_for_every_agent_unparks_every_parked_copy_not_only_the_universal_one", async () => {
    const parkedShared = folder("parked-shared", {
      scope: "parked",
      parked_origin: { kind: "universal", scope: "global" },
    });
    const parkedCodex = folder("parked-codex", {
      scope: "parked",
      parked_origin: { kind: "codex", scope: "global" },
    });
    const { calls, api } = fakeApi();

    const toast = await parkForEveryAgent(skill(true, [parkedShared, parkedCodex]), api);

    expect(calls).toEqual([{ kind: "unpark", ids: ["parked-shared", "parked-codex"] }]);
    expect(toast?.title).toBe("Turned on tidy");
  });

  it("park_for_every_agent_warns_with_the_first_refusal_when_one_copy_stays_live", async () => {
    const { api } = fakeApi([null, "the folder changed on disk"]);

    const toast = await parkForEveryAgent(
      skill(false, [
        folder("shared"),
        folder("codex-copy", { destination: "per-harness", backing: { kind: "independent" } }),
      ]),
      api,
    );

    expect(toast).toEqual({
      type: "warning",
      title: "Parked 1 of 2 copies of tidy",
      message: "the folder changed on disk",
    });
  });

  it("park_for_every_agent_warns_that_a_plugin_copy_stays_on_instead_of_reporting_success", async () => {
    const pluginCopy = folder("plugin-copy", {
      destination: "per-harness",
      owner_kind: "plugin",
      backing: { kind: "independent" },
      path: "/home/u/.claude/plugins/cache/official/tidy/skills/tidy",
      plugin: {
        name: "tidy",
        version: "1.0.0",
        harness: "claude",
        marketplace: "official",
        id: "tidy@official",
      },
    });
    const { calls, api } = fakeApi();

    const toast = await parkForEveryAgent(skill(false, [folder("shared"), pluginCopy]), api);

    expect(calls).toEqual([{ kind: "park", ids: ["shared"] }]);
    expect(toast?.type).toBe("warning");
    expect(toast?.message).toContain(".claude/plugins/cache/official/tidy/skills/tidy");
    expect(toast?.message).toContain("/plugin");
  });

  const devLink = folder("dev-link", {
    is_symlink: true,
    symlink_target: "/home/u/src/tidy",
    resolved_path: "/home/u/src/tidy",
  });

  it("park_for_every_agent_moves_a_shared_folder_that_is_a_link_to_a_dev_checkout", async () => {
    const claudeLink = folder("claude-link", {
      backing: { kind: "linked-to", deployment_id: "dev-link" },
      is_symlink: true,
      symlink_target: "/home/u/.agents/skills/dev-link",
      path: "/home/u/.claude/skills/tidy",
      resolved_path: "/home/u/src/tidy",
    });
    const { calls, api } = fakeApi();

    const toast = await parkForEveryAgent(skill(false, [devLink, claudeLink]), api);

    expect(calls).toEqual([{ kind: "park", ids: ["dev-link"] }]);
    expect(toast).toEqual({ type: "success", title: "Parked tidy" });
  });

  it("park_for_every_agent_warns_when_an_agent_folder_reads_the_dev_checkout_directly", async () => {
    // ~/.codex/skills -> ~/src: parking moves only the shared link, so Codex still loads the checkout.
    const codexAlias = folder("codex-alias", {
      backing: { kind: "linked-to", deployment_id: "dev-link" },
      shared_via_whole_dir_link: true,
      path: "/home/u/.codex/skills/tidy",
      resolved_path: "/home/u/src/tidy",
    });
    const { calls, api } = fakeApi();

    const toast = await parkForEveryAgent(skill(false, [devLink, codexAlias]), api);

    expect(calls).toEqual([{ kind: "park", ids: ["dev-link"] }]);
    expect(toast?.type).toBe("warning");
    expect(toast?.message).toContain(".codex/skills/tidy");
  });

  it("park_for_every_agent_waits_for_a_confirm_with_the_git_warning_before_moving_a_project_folder", async () => {
    const project = folder("project", {
      scope: "project",
      path: "/home/u/web/.agents/skills/tidy",
      project_path: "/home/u/web",
    });
    const cancelled = fakeApi([], { confirm: false, gitTracked: true });

    const toast = await parkForEveryAgent(skill(false, [folder("shared"), project]), cancelled.api);

    expect(toast).toBeNull();
    expect(cancelled.calls).toEqual([]);
    expect(cancelled.prompts[0]).toContain("web/.agents/skills/tidy");
    expect(cancelled.prompts[0]).toContain("Git tracks this folder");

    const confirmed = fakeApi([], { confirm: true, gitTracked: true });
    await parkForEveryAgent(skill(false, [folder("shared"), project]), confirmed.api);
    expect(confirmed.calls).toEqual([{ kind: "park", ids: ["shared", "project"] }]);
  });
});

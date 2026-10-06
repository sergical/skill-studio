// ============================================================================
// Skill Studio - skill-bulk-actions tests
// ============================================================================

import { describe, expect, it, vi } from "vitest";
import type {
  Deployment,
  ForkRecord,
  InstalledSkill,
  PluginInfo,
  PullResult,
} from "@skill-studio/lib";
import { realCopyDeployment, universalDeployment } from "../../dev/harness/scanned-deployment";
import {
  bulkActionToast,
  bulkDisabledReason,
  bulkRemovalTargets,
  bulkUpdateTargets,
  bulkUpdateResult,
  planBulkAction,
  runBulkSequentially,
  runBulkUpdate,
} from "./skill-bulk-actions";

function globalFolder(name: string, fields: Partial<Deployment> = {}): Deployment {
  return universalDeployment(
    { universalPath: `/home/.agents/skills/${name}` },
    { owner_kind: "manual", mutability: "mutable", owner_id: `owner:v1/global/${name}`, ...fields },
  );
}

/** The Global Universal folder after a park: the copy Unpark moves back. */
function parkedFolder(name: string): Deployment {
  return {
    ...globalFolder(name),
    scope: "parked",
    parked_origin: { kind: "universal", scope: "global", project_path: null },
  };
}

function fixtureSkill(name: string, overrides: Partial<InstalledSkill> = {}): InstalledSkill {
  return {
    name,
    source: "",
    source_type: "local",
    installed_at: "2026-01-01T00:00:00Z",
    has_update: false,
    source_kind: "manual",
    deployments: [globalFolder(name)],
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

function pluginSkill(name: string): InstalledSkill {
  return fixtureSkill(name, {
    source_kind: "plugin",
    deployments: [
      realCopyDeployment(
        { agent: "Claude Code", path: `/home/.claude/plugins/cache/p/${name}` },
        {
          owner_kind: "plugin",
          mutability: "read-only",
          owner_id: null,
          // SAFETY: the plugin-skill checks read only `plugin.name`; the other PluginInfo fields are irrelevant here.
          plugin: { name: "p" } as PluginInfo,
        },
      ),
    ],
  });
}

function updatable(name: string): InstalledSkill {
  return fixtureSkill(name, {
    source_kind: "skills-sh",
    update_owner_ids: [`owner:v1/global/${name}`],
    update_owners: [
      { owner_id: `owner:v1/global/${name}`, latest_commit: "abc", latest_commit_at: null },
    ],
    deployments: [globalFolder(name, { owner_kind: "skills-sh" })],
  });
}

const names = (skills: InstalledSkill[]) => skills.map((skill) => skill.name);

describe("planBulkAction", () => {
  const unparked = fixtureSkill("unparked");
  const parked = fixtureSkill("parked", { parked: true, deployments: [parkedFolder("parked")] });
  const plugin = pluginSkill("from-plugin");

  it("splits a mixed selection into parkable and already-parked skills, and skips a plugin skill; fails if park ignores the parked flag or the missing folder", () => {
    const plan = planBulkAction([unparked, parked, plugin], { kind: "park" });
    expect(names(plan.applicable)).toEqual(["unparked"]);
    expect(plan.skipped.map((s) => [s.skill.name, s.reason])).toEqual([
      ["parked", "already parked"],
      ["from-plugin", "no Global Universal folder"],
    ]);
  });

  it("offers unpark only for parked skills; fails if unpark applies to a skill that is not parked", () => {
    const plan = planBulkAction([unparked, parked, plugin], { kind: "unpark" });
    expect(names(plan.applicable)).toEqual(["parked"]);
    expect(plan.skipped.map((s) => s.reason)).toEqual(["not parked", "no Global Universal folder"]);
  });

  it("skips a plugin skill for invocation because it has no editable file; fails if read-only files count as editable", () => {
    const plan = planBulkAction([unparked, plugin], { kind: "invocation", policy: "user-only" });
    expect(names(plan.applicable)).toEqual(["unparked"]);
    expect(names(plan.skipped.map((s) => s.skill))).toEqual(["from-plugin"]);
  });

  it("updates only skills whose owner reports a newer commit; fails if a current skill is selected for update", () => {
    const plan = planBulkAction([updatable("stale"), unparked, plugin], { kind: "update" });
    expect(names(plan.applicable)).toEqual(["stale"]);
    expect(names(plan.skipped.map((s) => s.skill))).toEqual(["unparked", "from-plugin"]);
  });

  it("removes skills with a mutable copy and skips a plugin skill; fails if a read-only plugin copy is offered for removal", () => {
    const plan = planBulkAction([unparked, plugin], { kind: "remove" });
    expect(names(plan.applicable)).toEqual(["unparked"]);
    expect(plan.skipped).toEqual([{ skill: plugin, reason: "no removable copy" }]);
  });

  it("skips a skill whose global scope has two lifecycle owners, because removal would be ambiguous; fails if the removal availability check is bypassed", () => {
    const ambiguous = fixtureSkill("ambiguous", {
      deployments: [
        globalFolder("ambiguous"),
        realCopyDeployment(
          { agent: "Claude Code", path: "/home/.claude/skills/ambiguous" },
          { owner_kind: "copy", mutability: "mutable", owner_id: "owner:v1/global/other" },
        ),
      ],
    });
    const plan = planBulkAction([ambiguous], { kind: "remove" });
    expect(plan.applicable).toEqual([]);
    expect(plan.skipped.map((s) => s.reason)).toEqual(["needs a specific copy"]);
  });

  it("gives a disabled tooltip that names the reasons when nothing applies; fails if an empty plan has no reason or a runnable one has", () => {
    const none = planBulkAction([parked, parked], { kind: "park" });
    expect(bulkDisabledReason({ kind: "park" }, none)).toBe("Nothing to park: 2 already parked");
    const some = planBulkAction([unparked], { kind: "park" });
    expect(bulkDisabledReason({ kind: "park" }, some)).toBeNull();
  });
});

describe("bulkActionToast", () => {
  const a = fixtureSkill("a");
  const b = fixtureSkill("b");
  const c = fixtureSkill("c", { parked: true, deployments: [parkedFolder("c")] });

  it("names the count when every skill changed; fails if the title drops the count", () => {
    const plan = { applicable: [a, b], skipped: [] };
    expect(bulkActionToast({ kind: "park" }, plan, { succeeded: [a, b], failed: [] })).toEqual({
      type: "success",
      title: "Parked 2 skills",
    });
  });

  it("reports changed of total with the skip reasons on a partial skip; fails if skipped skills are hidden", () => {
    const plan = { applicable: [a, b], skipped: [{ skill: c, reason: "already parked" }] };
    expect(bulkActionToast({ kind: "park" }, plan, { succeeded: [a, b], failed: [] })).toEqual({
      type: "success",
      title: "Parked 2 of 3 skills · 1 already parked",
    });
  });

  it("lists the failed skill with its error and downgrades the toast; fails if a failure is reported as success", () => {
    const plan = { applicable: [a, b], skipped: [] };
    const toast = bulkActionToast({ kind: "remove" }, plan, {
      succeeded: [a],
      failed: [{ skill: b, error: "lease held" }],
    });
    expect(toast).toEqual({
      type: "warning",
      title: "Removed 1 of 2 skills · 1 failed",
      message: "b: lease held",
    });
  });

  it("uses the error type when nothing changed; fails if a total failure still looks like a warning", () => {
    const plan = { applicable: [a], skipped: [] };
    const toast = bulkActionToast({ kind: "update" }, plan, {
      succeeded: [],
      failed: [{ skill: a, error: "boom" }],
    });
    expect(toast.type).toBe("error");
  });
});

describe("runBulkSequentially", () => {
  it("keeps going after one rejected call and reports it; fails if a rejection stops the rest or is lost", async () => {
    const skills = [fixtureSkill("a"), fixtureSkill("b"), fixtureSkill("c")];
    const run = vi.fn(async (skill: InstalledSkill) => {
      if (skill.name === "b") throw new Error("lease held");
    });
    const result = await runBulkSequentially(skills, run);
    expect(run).toHaveBeenCalledTimes(3);
    expect(names(result.succeeded)).toEqual(["a", "c"]);
    expect(result.failed.map((f) => [f.skill.name, f.error])).toEqual([["b", "lease held"]]);
  });

  it("never overlaps two calls; fails if the runner starts a skill before the previous one settled", async () => {
    let active = 0;
    let peak = 0;
    const run = async () => {
      active += 1;
      peak = Math.max(peak, active);
      await Promise.resolve();
      active -= 1;
    };
    await runBulkSequentially([fixtureSkill("a"), fixtureSkill("b")], run);
    expect(peak).toBe(1);
  });
});

describe("bulkUpdateResult", () => {
  it("marks a skill with an error entry as failed and the rest as updated; fails if batch errors are dropped", () => {
    const a = fixtureSkill("a");
    const b = fixtureSkill("b");
    const result = bulkUpdateResult([a, b], {
      items: [
        { skill: "a", outcome: {} },
        { skill: "b", outcome: null },
      ],
      errors: { b: "network" },
    });
    expect(names(result.succeeded)).toEqual(["a"]);
    expect(result.failed).toEqual([{ skill: b, error: "network" }]);
  });
});

function projectFolder(name: string, projectPath: string, fields: Partial<Deployment> = {}) {
  return globalFolder(name, {
    scope: "project",
    project_path: projectPath,
    path: `${projectPath}/.agents/skills/${name}`,
    owner_id: `owner:v1/project/${projectPath}/${name}`,
    ...fields,
  });
}

describe("bulk targets across locations", () => {
  const twoLocations = (name: string, kind: "manual" | "skills-sh" = "manual") =>
    fixtureSkill(name, {
      source_kind: kind === "manual" ? "manual" : "skills-sh",
      deployments: [
        globalFolder(name, { owner_kind: kind }),
        projectFolder(name, "/work/p", { owner_kind: kind }),
      ],
    });

  it("removes a skill installed globally and in a project from both locations; fails if bulk Remove targets only the first location and the row stays", () => {
    const targets = bulkRemovalTargets(twoLocations("both"));
    expect(targets.map((target) => target.owner_id)).toEqual([
      "owner:v1/global/both",
      "owner:v1/project//work/p/both",
    ]);
  });

  it("updates every location that reports a newer commit; fails if bulk Update targets only the first location", () => {
    const skill = twoLocations("both", "skills-sh");
    skill.update_owner_ids = skill.deployments.flatMap((d) => d.owner_id ?? []);
    skill.update_owners = skill.update_owner_ids.map((owner_id) => ({
      owner_id,
      latest_commit: "abc",
      latest_commit_at: null,
    }));
    expect(bulkUpdateTargets(skill).map((target) => target.owner_id)).toEqual(
      skill.update_owner_ids,
    );
  });

  it("reports a skill as failed when any of its location updates fails; fails if a later item hides an earlier failure", () => {
    const skill = twoLocations("both", "skills-sh");
    const result = bulkUpdateResult([skill], {
      items: [
        { skill: "both", outcome: null },
        { skill: "both", outcome: {} },
      ],
      errors: {},
    });
    expect(names(result.failed.map((failure) => failure.skill))).toEqual(["both"]);
    expect(result.succeeded).toEqual([]);
  });
});

function outdatedSkillsSh(name: string): InstalledSkill {
  const ownerId = `owner:v1/global/${name}`;
  return fixtureSkill(name, {
    source_kind: "skills-sh",
    deployments: [globalFolder(name, { owner_kind: "skills-sh" })],
    update_owner_ids: [ownerId],
    update_owners: [{ owner_id: ownerId, latest_commit: "next", latest_commit_at: null }],
  });
}

function pullResult(conflicts: string[]): PullResult {
  return {
    from_commit: "a",
    to_commit: "b",
    merged: [],
    conflicts,
    added: [],
    removed: [],
    unchanged: 0,
    message: null,
  };
}

describe("runBulkUpdate", () => {
  const edited = outdatedSkillsSh("edited");
  const plain = outdatedSkillsSh("plain");

  function deps(conflicts: string[], calls: string[]) {
    return {
      fork: async (target: { deployment_id?: string | null }) => {
        calls.push(`fork ${target.deployment_id}`);
        // SAFETY: only `deployment_id` is read from the record.
        return { deployment_id: "dep:forked" } as ForkRecord;
      },
      pullFork: async (target: { deployment_id?: string | null }) => {
        calls.push(`pull ${target.deployment_id}`);
        return pullResult(conflicts);
      },
      updatePluginInstall: async (target: { plugin_id: string }) => {
        calls.push(`plugin ${target.plugin_id}`);
        return { outcome: "updated", message: null };
      },
      updateAll: async (targets: { owner_id?: string | null }[]) => {
        calls.push(`update ${targets.map((t) => t.owner_id).join(",")}`);
        const outcome = {
          items: targets.map(() => ({ skill: "plain", outcome: {} })),
          errors: {},
        };
        // SAFETY: bulkUpdateResult reads only `items[].skill/outcome` and `errors`.
        return outcome as never;
      },
    };
  }

  it("forks_then_pulls_the_edited_skill_and_updates_the_rest_or_overwrites_the_edit", async () => {
    const calls: string[] = [];
    const result = await runBulkUpdate(
      [edited, plain],
      new Set(["edited"]),
      deps([], calls),
      () => {},
    );
    expect(calls).toEqual([
      expect.stringMatching(/^fork dep:/),
      "pull dep:forked",
      "update owner:v1/global/plain",
    ]);
    expect(names(result.succeeded)).toEqual(["edited", "plain"]);
    expect(result.conflicted).toBeUndefined();
  });

  it("names_a_skill_whose_pull_left_conflict_markers_or_the_toast_reports_a_clean_update", async () => {
    const calls: string[] = [];
    const result = await runBulkUpdate(
      [edited],
      new Set(["edited"]),
      deps(["SKILL.md"], calls),
      () => {},
    );
    expect(result.conflicted).toEqual(["edited"]);
  });

  /** One skills.sh skill with a global copy and a project copy, each its own update owner. */
  function withProjectCopy(name: string): InstalledSkill {
    const projectOwner = `owner:v1/project/${name}`;
    const base = outdatedSkillsSh(name);
    return {
      ...base,
      deployments: [
        ...base.deployments,
        globalFolder(name, {
          scope: "project",
          project_path: "/p",
          path: `/p/.agents/skills/${name}`,
          owner_kind: "skills-sh",
          owner_id: projectOwner,
          backing: { kind: "canonical" },
        }),
      ],
      update_owner_ids: [...base.update_owner_ids, projectOwner],
      update_owners: [
        ...(base.update_owners ?? []),
        { owner_id: projectOwner, latest_commit: "next", latest_commit_at: null },
      ],
    };
  }

  it("still_updates_the_project_copy_of_a_forked_skill_or_silently_skips_it", async () => {
    const calls: string[] = [];
    const both = withProjectCopy("edited");
    const result = await runBulkUpdate(
      [both],
      new Set(["edited"]),
      {
        ...deps([], calls),
        updateAll: async (targets) => {
          calls.push(`update ${targets.map((t) => t.owner_id).join(",")}`);
          // SAFETY: bulkUpdateResult reads only `items[].skill/outcome` and `errors`.
          return { items: [{ skill: "edited", outcome: {} }], errors: {} } as never;
        },
      },
      () => {},
    );
    expect(calls).toEqual([
      expect.stringMatching(/^fork dep:/),
      "pull dep:forked",
      "update owner:v1/project/edited",
    ]);
    expect(names(result.succeeded)).toEqual(["edited"]);
  });

  it("fails_the_skill_when_only_its_project_copy_fails_or_the_toast_reports_a_false_success", async () => {
    const calls: string[] = [];
    const both = withProjectCopy("edited");
    const result = await runBulkUpdate(
      [both],
      new Set(["edited"]),
      {
        ...deps([], calls),
        // SAFETY: bulkUpdateResult reads only `items[].skill/outcome` and `errors`.
        updateAll: async () =>
          ({ items: [{ skill: "edited", outcome: null }], errors: { edited: "boom" } }) as never,
      },
      () => {},
    );
    expect(result.succeeded).toEqual([]);
    expect(result.failed[0]?.error).toContain("another copy failed: boom");
  });

  it("updates_no_copy_when_the_fork_fails_or_the_edit_is_overwritten_after_all", async () => {
    const calls: string[] = [];
    const both = withProjectCopy("edited");
    const result = await runBulkUpdate(
      [both],
      new Set(["edited"]),
      {
        ...deps([], calls),
        fork: async () => {
          throw new Error("fork refused");
        },
      },
      () => {},
    );
    expect(calls).toEqual([]);
    expect(result.failed[0]?.error).toBe("fork refused");
  });

  it("progress_reaches_its_total_when_a_fork_fails_or_the_bar_stalls", async () => {
    const seen: [number, number][] = [];
    await runBulkUpdate(
      [withProjectCopy("edited")],
      new Set(["edited"]),
      {
        ...deps([], []),
        fork: async () => {
          throw new Error("fork refused");
        },
      },
      (done, total) => seen.push([done, total]),
    );
    expect(seen[seen.length - 1]).toEqual([1, 1]);
  });
});

describe("bulkActionToast conflicts", () => {
  it("warns_and_names_the_conflicted_skills_or_a_conflicted_pull_reads_as_a_clean_success", () => {
    const a = fixtureSkill("a");
    const plan = { applicable: [a], skipped: [] };
    expect(
      bulkActionToast({ kind: "update" }, plan, { succeeded: [a], failed: [], conflicted: ["a"] }),
    ).toEqual({
      type: "warning",
      title: "Updated 1 skill",
      message: "Conflicts to resolve in the editor: a",
    });
  });
});

describe("runBulkUpdate with plugin updates", () => {
  const pluginSkill = (name: string): InstalledSkill => ({
    ...outdatedSkillsSh(name),
    deployments: [],
    update_owner_ids: ["plugin:codex@official"],
    update_owners: [
      {
        owner_id: "plugin:codex@official",
        latest_commit: null,
        latest_commit_at: null,
        plugin_scope: "user",
        plugin_project_path: null,
      },
    ],
  });
  const noop = async () => {
    throw new Error("unused");
  };

  it("bulk_update_runs_a_plugin_shared_by_two_skills_once_and_succeeds_both", async () => {
    const plugins: string[] = [];
    const result = await runBulkUpdate(
      [pluginSkill("one"), pluginSkill("two")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: noop,
        updatePluginInstall: async (target) => {
          plugins.push(target.plugin_id);
          return { outcome: "updated", message: null };
        },
      },
      () => {},
    );
    expect(plugins).toEqual(["codex@official"]);
    expect(result.succeeded).toHaveLength(2);
    expect(result.failed).toEqual([]);
  });

  it("bulk_update_fails_a_skill_whose_plugin_update_fails", async () => {
    const result = await runBulkUpdate(
      [pluginSkill("one")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: noop,
        updatePluginInstall: async () => {
          throw new Error("marketplace unreachable");
        },
      },
      () => {},
    );
    expect(result.succeeded).toEqual([]);
    expect(result.failed[0]?.error).toContain("marketplace unreachable");
  });

  const pluginOwnerEntry = (scope = "user", projectPath: string | null = null) => ({
    owner_id: "plugin:codex@official",
    latest_commit: null,
    latest_commit_at: null,
    plugin_scope: scope,
    plugin_project_path: projectPath,
  });
  /** A skills.sh skill whose managed copy and plugin install are both outdated. */
  const managedAndPlugin = (name: string): InstalledSkill => {
    const base = outdatedSkillsSh(name);
    return {
      ...base,
      update_owner_ids: [...base.update_owner_ids, "plugin:codex@official"],
      update_owners: [...(base.update_owners ?? []), pluginOwnerEntry()],
    };
  };
  const batchOk = async (targets: { owner_id?: string | null }[]) =>
    // SAFETY: bulkUpdateResult reads only `items[].skill/outcome` and `errors`.
    ({
      items: targets.map((t) => ({ skill: t.owner_id?.split("/").pop() ?? "", outcome: {} })),
      errors: {},
    }) as never;

  /** Flow: bulk Update on a skill with an outdated managed copy and an outdated plugin.
   * Expectation: both update and the skill succeeds. Failure: the managed target is dropped as
   * ambiguous while the plugin runs and the skill still counts as updated. */
  it("bulk_update_updates_the_managed_copy_and_the_plugin_of_one_skill", async () => {
    const calls: string[] = [];
    const skill = managedAndPlugin("both");
    expect(planBulkAction([skill], { kind: "update" }).applicable).toEqual([skill]);
    const result = await runBulkUpdate(
      [skill],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: async (targets) => {
          calls.push(`update ${targets.map((t) => t.owner_id).join(",")}`);
          return batchOk(targets);
        },
        updatePluginInstall: async (target) => {
          calls.push(`plugin ${target.plugin_id}`);
          return { outcome: "updated", message: null };
        },
      },
      () => {},
    );
    expect(calls).toEqual(["update owner:v1/global/both", "plugin codex@official"]);
    expect(names(result.succeeded)).toEqual(["both"]);
  });

  /** Flow: two managed owners in one scope are outdated, plus an outdated plugin.
   * Expectation: the skill is skipped as needing a specific location and its plugin is not run.
   * Failure: the skill is reported updated after only its plugin ran. */
  it("bulk_update_skips_a_skill_with_an_ambiguous_managed_copy_and_leaves_its_plugin_alone", async () => {
    const base = managedAndPlugin("amb");
    const skill: InstalledSkill = {
      ...base,
      deployments: [
        ...base.deployments,
        realCopyDeployment(
          { agent: "Claude Code", path: "/home/.claude/skills/amb" },
          { owner_kind: "copy", mutability: "mutable", owner_id: "owner:v1/global/other" },
        ),
      ],
      update_owner_ids: [...base.update_owner_ids, "owner:v1/global/other"],
      update_owners: [
        ...(base.update_owners ?? []),
        { owner_id: "owner:v1/global/other", latest_commit: "next", latest_commit_at: null },
      ],
    };
    expect(planBulkAction([skill], { kind: "update" }).skipped.map((s) => s.reason)).toEqual([
      "needs a specific location",
    ]);
    const updateAll = vi.fn(noop);
    const updatePluginInstall = vi.fn(async () => ({ outcome: "updated", message: null }));
    const result = await runBulkUpdate(
      [skill],
      new Set(),
      { fork: noop, pullFork: noop, updateAll, updatePluginInstall },
      () => {},
    );
    expect(updateAll).not.toHaveBeenCalled();
    expect(updatePluginInstall).not.toHaveBeenCalled();
    expect(result.succeeded).toEqual([]);
    expect(result.failed[0]?.error).toContain("more than one source");
  });

  /** Failure: a forked skill is reported updated while its plugin stays outdated. */
  it("bulk_update_runs_the_plugin_of_a_forked_skill", async () => {
    const plugins: string[] = [];
    const forkedSkill = managedAndPlugin("edited");
    const result = await runBulkUpdate(
      [forkedSkill],
      new Set(["edited"]),
      {
        // SAFETY: only `deployment_id` is read from the record.
        fork: async () => ({ deployment_id: "dep:forked" }) as ForkRecord,
        pullFork: async () => pullResult([]),
        updateAll: noop,
        updatePluginInstall: async (target) => {
          plugins.push(target.plugin_id);
          return { outcome: "updated", message: null };
        },
      },
      () => {},
    );
    expect(plugins).toEqual(["codex@official"]);
    expect(names(result.succeeded)).toEqual(["edited"]);
  });

  /** Failure: the plugin of a skill whose copy failed stays in the total, so the bar stops short. */
  it("bulk_update_progress_reaches_its_total_when_a_copy_fails_and_its_plugin_is_skipped", async () => {
    const seen: [number, number][] = [];
    const updatePluginInstall = vi.fn(async () => ({ outcome: "updated", message: null }));
    const result = await runBulkUpdate(
      [managedAndPlugin("one")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: async () =>
          // SAFETY: bulkUpdateResult reads only `items[].skill/outcome` and `errors`.
          ({ items: [{ skill: "one", outcome: null }], errors: { one: "boom" } }) as never,
        updatePluginInstall,
      },
      (done, total) => seen.push([done, total]),
    );
    expect(updatePluginInstall).not.toHaveBeenCalled();
    expect(result.failed[0]?.error).toBe("boom");
    expect(seen[seen.length - 1]).toEqual([1, 1]);
  });

  /** Flow: a plugin-only skill shares its plugin with a skill whose copy update failed.
   * Expectation: it is failed with a message naming the shared plugin, and the plugin is not run.
   * Failure: the summary shows no reason, or the plugin is updated anyway. */
  it("bulk_update_names_the_shared_plugin_when_a_sharing_skill_is_held_back", async () => {
    const updatePluginInstall = vi.fn(async () => ({ outcome: "updated", message: null }));
    const result = await runBulkUpdate(
      [managedAndPlugin("one"), pluginSkill("sharer")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: async () =>
          // SAFETY: bulkUpdateResult reads only `items[].skill/outcome` and `errors`.
          ({ items: [{ skill: "one", outcome: null }], errors: { one: "boom" } }) as never,
        updatePluginInstall,
      },
      () => {},
    );
    expect(updatePluginInstall).not.toHaveBeenCalled();
    expect(result.failed.find((failure) => failure.skill.name === "sharer")?.error).toBe(
      "Shares plugin codex@official with a skill that failed.",
    );
  });

  /** Failure: a failed project-scope install marks a skill tied only to the user-scope install as failed. */
  it("bulk_update_fails_only_the_skills_of_the_failed_plugin_scope", async () => {
    const withScope = (name: string, scope: string, projectPath: string | null) => ({
      ...pluginSkill(name),
      update_owners: [pluginOwnerEntry(scope, projectPath)],
    });
    const result = await runBulkUpdate(
      [withScope("usr", "user", null), withScope("proj", "project", "/p")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: noop,
        updatePluginInstall: async (target) => {
          if (target.scope === "project") throw new Error("project install broken");
          return { outcome: "updated", message: null };
        },
      },
      () => {},
    );
    expect(names(result.succeeded)).toEqual(["usr"]);
    expect(names(result.failed.map((failure) => failure.skill))).toEqual(["proj"]);
  });

  /** Flow: a fork whose only outdated owner is its plugin. Expectation: no fork pull, the plugin runs.
   * Failure: the pull runs with nothing to pull, or the plugin is skipped. */
  it("bulk_update_runs_only_the_plugin_of_a_fork_with_nothing_to_pull", async () => {
    const pullFork = vi.fn(async () => pullResult([]));
    const plugins: string[] = [];
    const result = await runBulkUpdate(
      [pluginSkill("edited")],
      new Set(["edited"]),
      {
        fork: noop,
        pullFork,
        updateAll: noop,
        updatePluginInstall: async (target) => {
          plugins.push(target.plugin_id);
          return { outcome: "updated", message: null };
        },
      },
      () => {},
    );
    expect(pullFork).not.toHaveBeenCalled();
    expect(plugins).toHaveLength(1);
    expect(names(result.succeeded)).toEqual(["edited"]);
  });

  /** Flow: a plugin step ends as skipped. Expectation: the skill counts as failed with the CLI message.
   * Failure: a skipped plugin is reported as updated. */
  it("bulk_update_counts_a_skipped_plugin_as_failed_and_finishes_progress", async () => {
    const seen: [number, number][] = [];
    const result = await runBulkUpdate(
      [pluginSkill("skip")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: noop,
        updatePluginInstall: async () => ({ outcome: "skipped", message: "not installed" }),
      },
      (done, total) => seen.push([done, total]),
    );
    expect(result.succeeded).toEqual([]);
    expect(result.failed[0]?.error).toContain("not installed");
    expect(seen[seen.length - 1]).toEqual([1, 1]);
  });

  /** Flow: a plugin step throws. Expectation: progress still reaches its total. */
  it("bulk_update_progress_counts_a_plugin_that_throws", async () => {
    const seen: [number, number][] = [];
    const result = await runBulkUpdate(
      [pluginSkill("boom")],
      new Set(),
      {
        fork: noop,
        pullFork: noop,
        updateAll: noop,
        updatePluginInstall: async () => {
          throw new Error("broken");
        },
      },
      (done, total) => seen.push([done, total]),
    );
    expect(names(result.failed.map((failure) => failure.skill))).toEqual(["boom"]);
    expect(seen[seen.length - 1]).toEqual([1, 1]);
  });
});

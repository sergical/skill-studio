// @vitest-environment happy-dom
// ============================================================================
// Skill Studio - mock-tauri tests
// The harness park must keep each copy's scope, as `park_layout.rs` does, or
// the e2e specs that park and unpark prove nothing about project copies.
// ============================================================================

import { invoke } from "@tauri-apps/api/core";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { installMockTauri } from "./mock-tauri";
import { scannerOwnerId } from "./scanned-deployment";
import {
  buildHarnessSnapshot,
  deployment,
  HARNESS_HOME,
  HARNESS_PROJECT,
  skill,
} from "./skill-fixture";

afterEach(() => {
  clearMocks();
});

it("parking commit's global and project copies keeps them apart, and unpark puts each back where it was", async () => {
  const harness = installMockTauri(buildHarnessSnapshot());
  const commit = () => harness.snapshot().skills.find((s) => s.name === "commit")!;
  const before = commit().deployments;
  const scopes = new Set(before.map((d) => d.scope));
  expect(scopes).toEqual(new Set(["global", "project"]));
  // An owner target names the skill, so park takes every folder of it.
  const target = { owner_id: "owner:v1/universal/commit" };

  await invoke("park_skill", { target });

  const parked = commit().deployments;
  expect(parked.every((d) => d.scope === "parked")).toBe(true);
  expect(new Set(parked.map((d) => d.id)).size).toBe(parked.length);
  expect(new Set(parked.map((d) => d.parked_origin?.scope))).toEqual(scopes);

  await invoke("unpark_skill", { target });

  const ids = (list: { id: string }[]) => list.map((d) => d.id).sort();
  expect(ids(commit().deployments)).toEqual(ids(before));
});

// The real updater runs and clears each owner on its own (commands.rs `owners_to_clear`), so a
// skill installed globally and in a project takes one command per scope.
it("Update all of a skill installed globally and in a project runs each install and clears both badges", async () => {
  const name = "sample";
  const globalOwner = `owner:v1/global/${name}`;
  const projectOwner = scannerOwnerId("project", HARNESS_PROJECT, name);
  const copy = (owner: string, projectPath: string | null) => ({
    ...deployment({
      agent: "shared",
      scope: projectPath ? "project" : "global",
      project_path: projectPath,
      path: `${projectPath ?? HARNESS_HOME}/.agents/skills/${name}`,
      owner_kind: "skills-sh",
    }),
    owner_id: owner,
  });
  const base = buildHarnessSnapshot();
  const sample = skill({
    name,
    source_kind: "skills-sh",
    has_update: true,
    deployments: [copy(globalOwner, null), copy(projectOwner, HARNESS_PROJECT)],
    update_owner_ids: [globalOwner, projectOwner],
    update_owners: [globalOwner, projectOwner].map((owner_id) => ({
      owner_id,
      latest_commit: "new",
      latest_commit_at: null,
    })),
  });
  const harness = installMockTauri({ ...base, skills: [...base.skills, sample] });

  const outcome = await invoke<{ items: { outcome: { deployment_path: string } | null }[] }>(
    "update_all_skills",
    { targets: [{ owner_id: globalOwner }, { owner_id: projectOwner }] },
  );

  expect(harness.updateCommandsStarted()).toBe(2);
  expect(outcome.items.map((item) => item.outcome?.deployment_path)).toEqual([
    `${HARNESS_HOME}/.agents/skills/${name}`,
    `${HARNESS_PROJECT}/.agents/skills/${name}`,
  ]);
  const after = harness.snapshot().skills.find((s) => s.name === name)!;
  expect(after.update_owner_ids).toEqual([]);
});

// The real updater picks the package manager per copy: one `dotagents install` per scope, and
// `npx skills update` for skills.sh copies. Failure caught: the mock picked it from the whole
// skill, folded the project skills.sh copy into the project's `dotagents install`, and reported
// three updates after two commands, so a Stop spec could pass on a command that never ran.
it("Update all runs one command per package manager and scope, by each copy's own owner", async () => {
  const copy = (
    name: string,
    owner: string,
    kind: "dotagents" | "skills-sh",
    project: boolean,
  ) => ({
    ...deployment({
      agent: "shared",
      scope: project ? "project" : "global",
      project_path: project ? HARNESS_PROJECT : null,
      path: `${project ? HARNESS_PROJECT : HARNESS_HOME}/.agents/skills/${name}`,
      owner_kind: kind,
    }),
    owner_id: owner,
  });
  const outdatedSkill = (name: string, copies: ReturnType<typeof copy>[]) =>
    skill({
      name,
      source_kind: "dotagents",
      has_update: true,
      deployments: copies,
      update_owner_ids: copies.map((c) => c.owner_id),
      update_owners: copies.map((c) => ({
        owner_id: c.owner_id,
        latest_commit: "new",
        latest_commit_at: null,
      })),
    });
  const mixedOwners = {
    global: scannerOwnerId("global", null, "mixed"),
    project: scannerOwnerId("project", HARNESS_PROJECT, "mixed"),
  };
  const otherOwner = scannerOwnerId("project", HARNESS_PROJECT, "other");
  const base = buildHarnessSnapshot();
  const harness = installMockTauri({
    ...base,
    skills: [
      ...base.skills,
      outdatedSkill("mixed", [
        copy("mixed", mixedOwners.global, "dotagents", false),
        copy("mixed", mixedOwners.project, "skills-sh", true),
      ]),
      outdatedSkill("other", [copy("other", otherOwner, "dotagents", true)]),
    ],
  });

  const outcome = await invoke<{ items: { outcome: unknown }[] }>("update_all_skills", {
    targets: [mixedOwners.global, mixedOwners.project, otherOwner].map((owner_id) => ({
      owner_id,
    })),
  });

  expect(outcome.items.filter((item) => item.outcome !== null)).toHaveLength(3);
  expect(harness.updateCommandsStarted()).toBe(3);
});

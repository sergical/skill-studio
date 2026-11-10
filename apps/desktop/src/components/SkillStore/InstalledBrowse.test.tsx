// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Toaster, toast } from "sonner";
import { afterEach, expect, it } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { SkillStore } from "./SkillStore";
import { HomeView } from "../Home/HomeView";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";

const owner = "owner:v1/global/sample";
const fixture = skill({
  name: "sample",
  source: "owner/repo",
  source_url: "https://github.com/owner/repo",
  source_kind: "skills-sh",
  has_update: true,
  deployments: [
    {
      ...deployment({
        agent: "shared",
        scope: "global",
        path: "/fixture/.agents/skills/sample",
        owner_kind: "skills-sh",
        backing: { kind: "canonical" },
      }),
      owner_id: owner,
    },
  ],
  update_owner_ids: [owner],
  update_owners: [{ owner_id: owner, latest_commit: "new", latest_commit_at: null }],
});

afterEach(() => {
  cleanup();
  clearMocks();
  toast.dismiss();
});
function ipc(afterHash: string) {
  const ids: unknown[] = [];
  const commands: string[] = [];
  mockIPC(
    (command, args) => {
      commands.push(command);
      if (command === "get_installed_skills") return [fixture];
      if (command === "get_popular_skills") return { skills: [], has_more: false };
      if (command === "get_skill_details") {
        ids.push(args);
        if (!args || !("skillId" in args) || args.skillId !== "owner/repo/sample")
          throw new Error("Details need owner/repo/slug");
        return {
          id: "owner/repo/sample",
          slug: "sample",
          source: "owner/repo",
          hash: "same",
          installs: 0,
          skill_md: "Installed sample content",
        };
      }
      if (command === "skill_local_edits") return [{ edited: false, checked: true }];
      if (command === "update_plugin") return { outcome: "up_to_date", message: null };
      if (command === "update_skill")
        return {
          event_id: "update",
          skill: "sample",
          deployment_path: fixture.deployments[0].path,
          tree_hash_before: "same",
          tree_hash_after: afterHash,
        };
      throw new Error(`Unexpected ${command}`);
    },
    { shouldMockEvents: true },
  );
  return { ids, commands };
}
async function openInstalled() {
  const user = userEvent.setup();
  render(
    <>
      <SkillStore />
      <Toaster />
    </>,
  );
  await user.click(await screen.findByRole("tab", { name: /Installed/ }));
  await user.click(await screen.findByText("sample"));
  return user;
}
it("Installed Browse loads owner/repo/slug details or sends a bare name and leaves the panel empty", async () => {
  const { ids } = ipc("new");
  const user = await openInstalled();
  expect(await screen.findByText("Installed sample content")).toBeTruthy();
  expect(ids).toEqual([{ skillId: "owner/repo/sample" }]);
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("an unchanged installed update says already up to date or reports a new installation", async () => {
  ipc("same");
  const user = await openInstalled();
  await user.click(await screen.findByRole("button", { name: "Update Skill" }));
  expect(await screen.findByText("sample is already up to date")).toBeTruthy();
  expect(screen.queryByText("Skill Installed")).toBeNull();
});
it("a changed installed update says updated or reuses the installation result", async () => {
  ipc("new");
  const user = await openInstalled();
  await user.click(await screen.findByRole("button", { name: "Update Skill" }));
  expect(await screen.findByText("Updated sample")).toBeTruthy();
  expect(screen.queryByText("Skill Installed")).toBeNull();
});

it("Home single-skill Update reports an unchanged owner as current or discards the returned hashes", async () => {
  ipc("same");
  const user = userEvent.setup();
  render(
    <>
      <HomeView
        snapshot={{ ...buildHarnessSnapshot(), skills: [fixture], invocations: [] }}
        isLoading={false}
        active
        onSelectSkill={() => {}}
      />
      <Toaster />
    </>,
  );
  await user.click(screen.getByRole("button", { name: "Pull latest" }));
  expect(await screen.findByText("sample is already up to date")).toBeTruthy();
  expect(screen.queryByText("Updated sample")).toBeNull();
});

it("Home reports a managed owner and plugin already current together or loses one no-op count", async () => {
  const { commands } = ipc("same");
  const plugin = deployment({
    agent: "Claude Code",
    scope: "global",
    path: "/fixture/.claude/plugins/sample",
    owner_kind: "plugin",
    plugin: {
      id: "sample@market",
      name: "sample",
      marketplace: "market",
      harness: "Claude Code",
      version: "1",
    },
  });
  const mixed = {
    ...fixture,
    deployments: [...fixture.deployments, plugin],
    update_owner_ids: [owner, "plugin:sample@market"],
    update_owners: [
      ...fixture.update_owners,
      {
        owner_id: "plugin:sample@market",
        latest_commit: null,
        latest_commit_at: null,
        plugin_scope: "user",
      },
    ],
  };
  const user = userEvent.setup();
  render(
    <>
      <HomeView
        snapshot={{ ...buildHarnessSnapshot(), skills: [mixed], invocations: [] }}
        isLoading={false}
        active
        onSelectSkill={() => {}}
      />
      <Toaster />
    </>,
  );
  await user.click(screen.getByRole("button", { name: "Pull latest" }));
  expect(await screen.findByText("sample is already up to date")).toBeTruthy();
  expect(commands.filter((command) => command === "update_skill")).toHaveLength(1);
  expect(commands.filter((command) => command === "update_plugin")).toHaveLength(1);
});

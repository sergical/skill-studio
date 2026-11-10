// @vitest-environment happy-dom
import { useState } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Toaster, toast } from "sonner";
import { HomeInboxGrid } from "../components/Home/HomeInboxGroups";
import { computeHomeGroups } from "../components/Home/home-inbox-data";
import { runHeaderUpdate } from "../components/SkillDetail/skill-page-actions";
import { updatePluginInstall } from "./skillBatchUpdates";
import { Button } from "@skill-studio/ui";
import { deployment, skill } from "../dev/harness/skill-fixture";
import { useGuardedSkillUpdate } from "./useGuardedSkillUpdate";

const shared = {
  ...deployment({
    agent: "shared",
    scope: "global",
    path: "/fixture/.agents/skills/demo",
    owner_kind: "skills-sh",
    backing: { kind: "canonical" },
  }),
  owner_id: "owner:v1/global/demo",
};
const plugin = deployment({
  agent: "Claude Code",
  scope: "global",
  path: "/fixture/.claude/plugins/demo",
  owner_kind: "plugin",
  plugin: {
    id: "demo@market",
    name: "demo",
    marketplace: "market",
    harness: "Claude Code",
    version: "1",
  },
});
const fixture = skill({
  name: "demo",
  source_kind: "skills-sh",
  deployments: [shared, plugin],
  update_owner_ids: [shared.owner_id, "plugin:demo@market"],
  update_owners: [
    { owner_id: shared.owner_id, latest_commit: "new", latest_commit_at: null },
    {
      owner_id: "plugin:demo@market",
      latest_commit: null,
      latest_commit_at: null,
      plugin_scope: "user",
    },
  ],
});

function Update() {
  const { requestUpdate, dialog } = useGuardedSkillUpdate();
  const [message, setMessage] = useState("Waiting");
  return (
    <>
      <Button
        onClick={() =>
          void runHeaderUpdate(
            fixture,
            {
              requestUpdate: (target, options) =>
                requestUpdate(target, {
                  ...options,
                  onFinished: async (result) => {
                    await options.onFinished(result);
                    setMessage(result.success ? "Finished" : (result.error ?? "Failed"));
                  },
                }),
            },
            () => {},
            updatePluginInstall,
          )
        }
      >
        Update
      </Button>
      <p role="status">{message}</p>
      {dialog}
    </>
  );
}

afterEach(() => {
  cleanup();
  clearMocks();
  toast.dismiss();
});

it("fork and update reports a failed pull and still updates the plugin instead of leaving the finish callback waiting", async () => {
  let pluginUpdates = 0;
  mockIPC((command) => {
    if (command === "skill_local_edits") return [{ edited: true, checked: true }];
    if (command === "fork_skill") return { deployment_id: "forked" };
    if (command === "pull_fork_upstream") throw new Error("network unavailable");
    if (command === "update_plugin") {
      pluginUpdates++;
      return { outcome: "updated", message: null };
    }
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<Update />);
  await user.click(screen.getByRole("button", { name: "Update" }));
  await user.click(await screen.findByRole("button", { name: "Fork and update" }));
  expect(await screen.findByText(/network unavailable/)).toBeTruthy();
  expect(pluginUpdates).toBe(1);
});

it("Home Update all displays both plugin failures instead of discarding the second reason", async () => {
  const updates = ["alpha", "beta"].map((name) =>
    skill({
      name,
      source_kind: "plugin",
      has_update: true,
      deployments: [
        {
          ...plugin,
          path: `/fixture/plugins/${name}`,
          plugin: {
            marketplace: "market",
            harness: "Claude Code",
            version: "1",
            id: `${name}@market`,
            name,
          },
        },
      ],
      update_owner_ids: [`plugin:${name}@market`],
      update_owners: [
        {
          owner_id: `plugin:${name}@market`,
          latest_commit: null,
          latest_commit_at: null,
          plugin_scope: "user",
        },
      ],
    }),
  );
  mockIPC((command, args) => {
    if (command === "update_plugin" && args && "pluginId" in args)
      throw new Error(String(args.pluginId).startsWith("alpha") ? "alpha refused" : "beta refused");
    throw new Error(`unexpected ${command}`);
  });
  const noop = () => {};
  render(
    <>
      <HomeInboxGrid
        groups={{ ...computeHomeGroups(undefined), updates, allClear: false }}
        starts={{ broken: 0, warn: 0, upd: 0, unused: 2, rec: 2 }}
        filter={null}
        onClearFilter={noop}
        isGroupVisible={(id) => id === "upd"}
        isGroupExpanded={() => true}
        toggleGroup={noop}
        onSelectSkill={noop}
        onShowAllIssues={noop}
        onShowAllUpdates={noop}
        onShowAllUnused={noop}
        onShowAllRecent={noop}
        openSkill={noop}
        onConvertLinkedRoot={noop}
        containerRef={noop}
        onGridKeyDown={noop}
        rowRef={() => noop}
        tabIndexFor={() => 0}
      />
      <Toaster />
    </>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Update all" }));
  expect(await screen.findByText(/alpha refused.*beta refused/)).toBeTruthy();
});

it("Home Update all refreshes a shared marketplace once instead of refreshing it for each plugin", async () => {
  const updates = ["alpha", "beta"].map((name) =>
    skill({
      name,
      source_kind: "plugin",
      has_update: true,
      deployments: [
        {
          ...plugin,
          path: `/fixture/plugins/${name}`,
          plugin: {
            id: `${name}@market`,
            name,
            marketplace: "market",
            harness: "Claude Code",
            version: "1",
          },
        },
      ],
      update_owner_ids: [`plugin:${name}@market`],
      update_owners: [
        {
          owner_id: `plugin:${name}@market`,
          latest_commit: null,
          latest_commit_at: null,
          plugin_scope: "user",
        },
      ],
    }),
  );
  const skips: boolean[] = [];
  mockIPC((command, args) => {
    if (command === "update_plugin" && args && "pluginId" in args) {
      skips.push("skipMarketplaceRefresh" in args && args.skipMarketplaceRefresh === true);
      return { outcome: "updated", message: null, marketplace_refreshed: true };
    }
    throw new Error(`unexpected ${command}`);
  });
  const noop = () => {};
  render(
    <>
      <HomeInboxGrid
        groups={{ ...computeHomeGroups(undefined), updates, allClear: false }}
        starts={{ broken: 0, warn: 0, upd: 0, unused: 2, rec: 2 }}
        filter={null}
        onClearFilter={noop}
        isGroupVisible={(id) => id === "upd"}
        isGroupExpanded={() => true}
        toggleGroup={noop}
        onSelectSkill={noop}
        onShowAllIssues={noop}
        onShowAllUpdates={noop}
        onShowAllUnused={noop}
        onShowAllRecent={noop}
        openSkill={noop}
        onConvertLinkedRoot={noop}
        containerRef={noop}
        onGridKeyDown={noop}
        rowRef={() => noop}
        tabIndexFor={() => 0}
      />
      <Toaster />
    </>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Update all" }));
  expect(await screen.findByText("Updated 2 of 2 skills")).toBeTruthy();
  expect(skips).toEqual([false, true]);
});

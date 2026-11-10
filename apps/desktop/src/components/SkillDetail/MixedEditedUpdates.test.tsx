// @vitest-environment happy-dom

import { useState } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { Button } from "@skill-studio/ui";
import { Toaster, toast } from "sonner";
import { useGuardedSkillUpdate } from "../../hooks/useGuardedSkillUpdate";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { HomeInboxGrid } from "../Home/HomeInboxGroups";
import { computeHomeGroups } from "../Home/home-inbox-data";
import { useSkillBulkActions } from "../SkillList/useSkillBulkActions";
import { UpdateOverwritesEditsDialog } from "./UpdateOverwritesEditsDialog";

const updates = ["alpha", "beta"].map((name) => {
  const owner = `owner:v1/global/${name}`;
  return skill({
    name,
    source_kind: "skills-sh",
    has_update: true,
    deployments: [
      {
        ...deployment({
          agent: "shared",
          path: `/fixture/.agents/skills/${name}`,
          scope: "global",
          owner_kind: "skills-sh",
          backing: { kind: "canonical" },
        }),
        owner_id: owner,
        destination: "universal",
      },
      ...(name === "beta"
        ? [
            deployment({
              agent: "Codex",
              path: `/fixture/.codex/skills/${name}`,
              scope: "global",
              owner_kind: "manual",
              backing: { kind: "independent" },
              destination: "per-harness",
            }),
          ]
        : []),
    ],
    update_owner_ids: [owner],
    update_owners: [{ owner_id: owner, latest_commit: "new", latest_commit_at: null }],
  });
});

function SelectionUpdate({ skills = updates }: { skills?: typeof updates }) {
  const [finished, setFinished] = useState(false);
  const { run, updatePrompt } = useSkillBulkActions(() => setFinished(true));
  return (
    <>
      <Button onClick={() => void run({ kind: "update" }, skills)}>Update selection</Button>
      {finished && <p role="status">Finished</p>}
      <UpdateOverwritesEditsDialog
        skillNames={updatePrompt?.skillNames ?? []}
        isBulk
        canFork={updatePrompt?.canFork ?? false}
        unforkableSkillNames={updatePrompt?.unforkableSkillNames ?? []}
        onFork={(choice) => updatePrompt?.fork(choice)}
        onOverwrite={() => updatePrompt?.overwrite()}
        onCancel={() => updatePrompt?.cancel()}
      />
      <Toaster />
    </>
  );
}

const noop = () => {};
function HomeUpdate({ skills = updates }: { skills?: typeof updates }) {
  return (
    <>
      <HomeInboxGrid
        groups={{ ...computeHomeGroups(undefined), updates: skills, allClear: false }}
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
    </>
  );
}

afterEach(() => {
  cleanup();
  clearMocks();
  toast.dismiss();
});

it.each(
  ["Home", "Selection"].flatMap((surface) =>
    [true, false].flatMap((skip) =>
      ["{Enter}", " "].map((key) => ({
        surface,
        skip,
        key,
        choice: skip ? "skip" : "overwrite",
        keyName: key === " " ? "Space" : "Enter",
      })),
    ),
  ),
)(
  "$surface mixed edited Update can fork and $choice others with $keyName or loses the edit choice",
  async ({ surface, skip, key }) => {
    const forkTargets: unknown[] = [];
    const updateTargets: unknown[] = [];
    mockIPC((command, args) => {
      if (command === "skill_local_edits")
        return [
          { checked: true, edited: true },
          { checked: true, edited: true },
        ];
      if (command === "fork_skill") {
        forkTargets.push(args);
        return { deployment_id: "forked-alpha" };
      }
      if (command === "pull_fork_upstream")
        return {
          from_commit: "old",
          to_commit: "new",
          merged: ["SKILL.md"],
          conflicts: [],
          added: [],
          removed: [],
          unchanged: 0,
          message: null,
        };
      if (command === "update_all_skills") {
        if (args && "targets" in args && Array.isArray(args.targets))
          updateTargets.push(...args.targets);
        return {
          items: [
            {
              skill: "beta",
              outcome: {
                event_id: "update-beta",
                skill: "beta",
                deployment_path: "/fixture/.codex/skills/beta",
                tree_hash_before: "old",
                tree_hash_after: "new",
              },
            },
          ],
          errors: {},
          not_run: [],
        };
      }
      if (command === "plugin:event|listen") return 1;
      if (command === "plugin:event|unlisten") return;
      throw new Error(`unexpected ${command}`);
    });
    const user = userEvent.setup();
    render(surface === "Home" ? <HomeUpdate /> : <SelectionUpdate />);
    const entry = surface === "Home" ? "Update all" : "Update selection";
    await user.click(screen.getByRole("button", { name: entry }));
    await screen.findByRole("alertdialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(forkTargets).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: entry }));
    const choice = await screen.findByRole("button", {
      name: skip ? "Fork eligible and skip others" : "Fork eligible and overwrite others",
    });
    expect(screen.getByText(/beta cannot be forked here/)).toBeTruthy();
    for (let tabs = 0; tabs < 8 && document.activeElement !== choice; tabs++) await user.tab();
    expect(document.activeElement).toBe(choice);
    await user.keyboard(key);
    await waitFor(() => expect(forkTargets).toHaveLength(1));
    if (surface === "Home") await screen.findByRole("button", { name: "Update all" });
    else await screen.findByText("Finished");
    expect(updateTargets).toEqual(skip ? [] : [{ owner_id: "owner:v1/global/beta" }]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  },
);

function SingleUpdate() {
  const { requestUpdate, dialog } = useGuardedSkillUpdate();
  return (
    <>
      <Button onClick={() => void requestUpdate(updates[1])}>Update copy</Button>
      {dialog}
    </>
  );
}

it("single_copy_update_keeps_edits_on_cancel_and_requires_overwrite_or_fork_loses_them", async () => {
  const writes: string[] = [];
  mockIPC((command) => {
    if (command === "skill_local_edits") return [{ checked: true, edited: true }];
    if (command === "update_skill") {
      writes.push(command);
      return { success: true };
    }
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<SingleUpdate />);
  await user.click(screen.getByRole("button", { name: "Update copy" }));
  await screen.findByRole("alertdialog");
  expect(screen.queryByRole("button", { name: /Fork/ })).toBeNull();
  expect(screen.getByText(/no independent agent copies/)).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(writes).toEqual([]);
  await user.click(screen.getByRole("button", { name: "Update copy" }));
  await user.click(await screen.findByRole("button", { name: "Overwrite edits" }));
  await waitFor(() => expect(writes).toEqual(["update_skill"]));
});

it("Home confirmation updates only the checked batch or a refresh overwrites an unseen edit", async () => {
  const writes: unknown[] = [];
  mockIPC((command, args) => {
    if (command === "skill_local_edits")
      return [
        { checked: true, edited: true },
        { checked: true, edited: true },
      ];
    if (command === "fork_skill") return { deployment_id: "forked-alpha" };
    if (command === "pull_fork_upstream")
      return {
        from_commit: "old",
        to_commit: "new",
        merged: [],
        conflicts: [],
        added: [],
        removed: [],
        unchanged: 0,
        message: null,
      };
    if (command === "update_all_skills") {
      if (args && "targets" in args && Array.isArray(args.targets)) writes.push(...args.targets);
      return { items: [], errors: {}, not_run: [] };
    }
    if (command === "plugin:event|listen") return 1;
    if (command === "plugin:event|unlisten") return;
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  const view = render(<HomeUpdate />);
  await user.click(screen.getByRole("button", { name: "Update all" }));
  await screen.findByRole("alertdialog");
  const unseen = skill({
    ...updates[0],
    name: "unseen",
    update_owner_ids: ["owner:v1/global/unseen"],
    update_owners: [
      { owner_id: "owner:v1/global/unseen", latest_commit: "new", latest_commit_at: null },
    ],
  });
  view.rerender(<HomeUpdate skills={[...updates, unseen]} />);
  await user.click(screen.getByRole("button", { name: "Fork eligible and skip others" }));
  await screen.findByRole("button", { name: "Update all" });
  expect(writes).toEqual([]);
});

it("Home confirmation checks targets again or overwrites edits made while the dialog was open", async () => {
  let checks = 0;
  const writes: unknown[] = [];
  mockIPC((command, args) => {
    if (command === "skill_local_edits") {
      checks++;
      return [
        { checked: true, edited: true },
        { checked: true, edited: checks > 1 },
      ];
    }
    if (command === "update_all_skills") {
      writes.push(args);
      return { items: [], errors: {}, not_run: [] };
    }
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<HomeUpdate />);
  await user.click(screen.getByRole("button", { name: "Update all" }));
  await screen.findByRole("alertdialog");
  await user.click(screen.getByRole("button", { name: "Overwrite all" }));
  await waitFor(() => expect(checks).toBe(2));
  await screen.findByRole("alertdialog");
  expect(screen.getByText(/beta cannot be forked here/)).toBeTruthy();
  expect(writes).toEqual([]);
});

it("Selection confirmation rechecks retained targets or overwrites gamma edited while the dialog was open", async () => {
  let checks = 0;
  const writes: string[] = [];
  const gamma = skill({
    ...updates[0],
    name: "gamma",
    deployments: updates[0].deployments.map((copy) => ({
      ...copy,
      id: "gamma-copy",
      owner_id: "owner:v1/global/gamma",
      path: "/fixture/.agents/skills/gamma",
    })),
    update_owner_ids: ["owner:v1/global/gamma"],
    update_owners: [
      { owner_id: "owner:v1/global/gamma", latest_commit: "new", latest_commit_at: null },
    ],
  });
  mockIPC((command, args) => {
    if (command === "skill_local_edits") {
      checks++;
      if (checks === 1)
        return [
          { checked: true, edited: true },
          { checked: true, edited: true },
          { checked: true, edited: false },
        ];
      expect(args).toEqual({
        targets: [{ owner_id: "owner:v1/global/alpha" }, { owner_id: "owner:v1/global/gamma" }],
      });
      return [
        { checked: true, edited: true },
        { checked: true, edited: true },
      ];
    }
    writes.push(command);
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  render(<SelectionUpdate skills={[...updates, gamma]} />);
  await user.click(screen.getByRole("button", { name: "Update selection" }));
  await screen.findByRole("alertdialog");
  await user.click(screen.getByRole("button", { name: "Fork eligible and skip others" }));
  await waitFor(() => expect(checks).toBe(2));
  await screen.findByRole("alertdialog");
  expect(screen.getByText(/alpha, gamma/)).toBeTruthy();
  expect(writes).toEqual([]);
});

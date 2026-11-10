// @vitest-environment happy-dom

import { useState } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { Button } from "@skill-studio/ui";
import { Toaster, toast } from "sonner";
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
          agent: name === "alpha" ? "shared" : "Codex",
          path:
            name === "alpha" ? `/fixture/.agents/skills/${name}` : `/fixture/.codex/skills/${name}`,
          scope: "global",
          owner_kind: "skills-sh",
          backing: { kind: name === "alpha" ? "canonical" : "independent" },
        }),
        owner_id: owner,
        destination: name === "alpha" ? "universal" : "per-harness",
      },
    ],
    update_owner_ids: [owner],
    update_owners: [{ owner_id: owner, latest_commit: "new", latest_commit_at: null }],
  });
});

function SelectionUpdate() {
  const [finished, setFinished] = useState(false);
  const { run, updatePrompt } = useSkillBulkActions(() => setFinished(true));
  return (
    <>
      <Button onClick={() => void run({ kind: "update" }, updates)}>Update selection</Button>
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
function HomeUpdate() {
  return (
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

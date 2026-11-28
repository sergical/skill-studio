// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { Toaster, toast } from "sonner";
import { z } from "zod";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { UpdateAllButton } from "./UpdateAllButton";

function outdated(name: string) {
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
    ],
    update_owner_ids: [owner],
    update_owners: [{ owner_id: owner, latest_commit: "new", latest_commit_at: null }],
  });
}

afterEach(() => {
  cleanup();
  clearMocks();
  toast.dismiss();
});

it("Update all overwrites only the edited skills the user confirmed, not one that arrived while the dialog was open", async () => {
  const updateTargets: unknown[] = [];
  mockIPC((command, args) => {
    if (command === "skill_local_edits")
      return [
        { checked: true, edited: true },
        { checked: true, edited: true },
      ];
    if (command === "update_all_skills") {
      if (args && "targets" in args && Array.isArray(args.targets))
        updateTargets.push(...args.targets);
      return { items: [], errors: {}, not_run: [] };
    }
    if (command === "plugin:event|listen") return 1;
    if (command === "plugin:event|unlisten") return;
    throw new Error(`unexpected ${command}`);
  });
  const user = userEvent.setup();
  const { rerender } = render(
    <>
      <UpdateAllButton updates={[outdated("alpha"), outdated("beta")]} />
      <Toaster />
    </>,
  );
  await user.click(screen.getByRole("button", { name: "Update all" }));
  await screen.findByRole("alertdialog");

  rerender(
    <>
      <UpdateAllButton updates={[outdated("alpha"), outdated("beta"), outdated("gamma")]} />
      <Toaster />
    </>,
  );
  await user.click(screen.getByRole("button", { name: "Overwrite all" }));

  await waitFor(() => expect(updateTargets).toHaveLength(2));
  expect(updateTargets).toEqual([
    { owner_id: "owner:v1/global/alpha" },
    { owner_id: "owner:v1/global/beta" },
  ]);
});

const TARGETS = z.object({ targets: z.array(z.object({ owner_id: z.string() })).default([]) });

/** IPC where `skill_local_edits` answers from `edited` at call time, and `update_all_skills` records its targets. */
function mockEditsAndUpdates(edited: Set<string>) {
  const updateTargets: unknown[] = [];
  mockIPC((command, args) => {
    const { targets } = TARGETS.parse(args);
    if (command === "skill_local_edits")
      return targets.map(({ owner_id }) => ({ checked: true, edited: edited.has(owner_id) }));
    if (command === "update_all_skills") {
      updateTargets.push(...targets);
      return { items: [], errors: {}, not_run: [] };
    }
    if (command === "update_plugin") {
      updateTargets.push(args);
      return { outcome: "updated", message: "", marketplace_refreshed: false };
    }
    if (command === "plugin:event|listen") return 1;
    if (command === "plugin:event|unlisten") return;
    throw new Error(`unexpected ${command}`);
  });
  return updateTargets;
}

/** A skill that a Claude Code plugin ships and skills.sh also installs globally, both outdated. */
function outdatedWithPlugin(name: string) {
  const base = outdated(name);
  const plugin = { owner_id: `plugin:${name}-pack@market`, plugin_scope: "user" };
  return {
    ...base,
    source_kind: "plugin" as const,
    update_owner_ids: [...base.update_owner_ids, plugin.owner_id],
    update_owners: [
      ...base.update_owners,
      { ...plugin, latest_commit: "new", latest_commit_at: null },
    ],
  };
}

/** `skill` with a project skills.sh copy that also has an update now. */
function withOutdatedProjectCopy(skill: ReturnType<typeof outdated>) {
  const owner = `owner:v1/project/${skill.name}`;
  return {
    ...skill,
    deployments: [
      ...skill.deployments,
      {
        ...deployment({
          agent: "shared",
          path: `/fixture/project/.agents/skills/${skill.name}`,
          scope: "project",
          project_path: "/fixture/project",
          owner_kind: "skills-sh",
          backing: { kind: "canonical" },
        }),
        owner_id: owner,
        destination: "universal" as const,
      },
    ],
    update_owner_ids: [...skill.update_owner_ids, owner],
    update_owners: [
      ...skill.update_owners,
      { owner_id: owner, latest_commit: "new", latest_commit_at: null },
    ],
  };
}

// Failure caught: a plugin also ships beta, so the skill reads as a plugin skill and the
// recheck skipped beta's skills.sh copy; confirming overwrote edits the user never saw.
it("Update all asks again when the skills.sh copy of a skill a plugin also ships gained edits while the dialog was open", async () => {
  const edited = new Set(["owner:v1/global/alpha"]);
  const updateTargets = mockEditsAndUpdates(edited);
  const user = userEvent.setup();
  render(<UpdateAllButton updates={[outdated("alpha"), outdatedWithPlugin("beta")]} />);
  await user.click(screen.getByRole("button", { name: "Update all" }));
  expect((await screen.findByRole("alertdialog")).textContent).not.toContain("beta");

  edited.add("owner:v1/global/beta");
  await user.click(screen.getByRole("button", { name: "Overwrite all" }));

  await waitFor(() => expect(screen.getByRole("alertdialog").textContent).toContain("beta"));
  expect(updateTargets).toEqual([]);
});

// Failure caught: the dialog checked two copies, but confirming sent three, adding a project
// copy that became eligible meanwhile and whose edits the desktop cannot check.
it("Update all sends only the copies it checked, not a project copy that gained an update while the dialog was open", async () => {
  const updateTargets = mockEditsAndUpdates(new Set(["owner:v1/global/alpha"]));
  const user = userEvent.setup();
  const { rerender } = render(<UpdateAllButton updates={[outdated("alpha"), outdated("beta")]} />);
  await user.click(screen.getByRole("button", { name: "Update all" }));
  await screen.findByRole("alertdialog");

  rerender(
    <UpdateAllButton updates={[outdated("alpha"), withOutdatedProjectCopy(outdated("beta"))]} />,
  );
  await user.click(screen.getByRole("button", { name: "Overwrite all" }));

  await waitFor(() => expect(updateTargets).toHaveLength(2));
  expect(updateTargets).toEqual([
    { owner_id: "owner:v1/global/alpha" },
    { owner_id: "owner:v1/global/beta" },
  ]);
});

it("Update all leaves out a skill edited while the edits dialog was open and runs the rest", async () => {
  const edited = new Set(["owner:v1/global/alpha"]);
  const updateTargets = mockEditsAndUpdates(edited);
  const user = userEvent.setup();
  const { rerender } = render(
    <UpdateAllButton updates={[outdated("alpha"), outdated("beta"), outdated("gamma")]} />,
  );
  await user.click(screen.getByRole("button", { name: "Update all" }));
  expect((await screen.findByRole("alertdialog")).textContent).not.toContain("beta");

  // beta is edited now: the list moves it to Warnings, so it leaves Updates.
  edited.add("owner:v1/global/beta");
  rerender(<UpdateAllButton updates={[outdated("alpha"), outdated("gamma")]} />);
  await user.click(screen.getByRole("button", { name: "Overwrite all" }));

  await waitFor(() => expect(updateTargets).toHaveLength(2));
  expect(updateTargets).toEqual([
    { owner_id: "owner:v1/global/alpha" },
    { owner_id: "owner:v1/global/gamma" },
  ]);
});

it("Update all asks again when a skill still in Updates gained edits while the dialog was open", async () => {
  const edited = new Set(["owner:v1/global/alpha"]);
  const updateTargets = mockEditsAndUpdates(edited);
  const user = userEvent.setup();
  render(<UpdateAllButton updates={[outdated("alpha"), outdated("beta"), outdated("gamma")]} />);
  await user.click(screen.getByRole("button", { name: "Update all" }));
  await screen.findByRole("alertdialog");

  edited.add("owner:v1/global/beta");
  await user.click(screen.getByRole("button", { name: "Overwrite all" }));

  await waitFor(() => expect(screen.getByRole("alertdialog").textContent).toContain("beta"));
  expect(updateTargets).toEqual([]);
});

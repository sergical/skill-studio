// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { Toaster, toast } from "sonner";
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

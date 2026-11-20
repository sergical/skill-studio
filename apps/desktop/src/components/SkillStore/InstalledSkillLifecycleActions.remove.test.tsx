// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Toaster, toast } from "sonner";
import { afterEach, expect, it, vi } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { InstalledSkillLifecycleActions } from "./InstalledSkillLifecycleActions";

afterEach(() => {
  toast.dismiss();
  cleanup();
  clearMocks();
});

const installed = skill({
  name: "demo",
  source_kind: "manual",
  deployments: [
    deployment({
      agent: "Codex",
      scope: "global",
      path: "/fixture/.codex/skills/demo",
      owner_id: "owner:v1/global/demo",
    }),
  ],
});

it("a failed Remove shows the backend's reason and keeps the skill, instead of failing silently", async () => {
  mockIPC((command) => {
    if (command === "remove_skill") throw "fixture: permission denied";
    return null;
  });
  const onRemoveComplete = vi.fn();
  render(
    <>
      <InstalledSkillLifecycleActions
        skill={{
          id: "getsentry/skills/demo",
          name: "demo",
          description: null,
          installs: 1,
          top_source: "getsentry/skills",
          author: null,
          tags: null,
          is_installed: true,
          installed_info: installed,
        }}
        onUpdateComplete={vi.fn()}
        onRemoveComplete={onRemoveComplete}
      />
      <Toaster />
    </>,
  );

  await userEvent.click(screen.getByRole("button", { name: "Remove Skill" }));
  await userEvent.click(await screen.findByRole("button", { name: "Confirm Remove" }));

  expect(await screen.findByText("fixture: permission denied")).toBeTruthy();
  expect(onRemoveComplete).not.toHaveBeenCalled();
});

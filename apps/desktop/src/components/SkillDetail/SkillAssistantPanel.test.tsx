// @vitest-environment happy-dom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it, vi } from "vitest";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { SkillAssistantPanel } from "./SkillAssistantPanel";

afterEach(async () => {
  cleanup();
  await Promise.resolve();
  clearMocks();
});

it("blocks competing actions for the whole project preparation phase", async () => {
  const prepare = vi.fn();
  mockIPC(
    (command) => {
      if (command === "prepare_skill_run_target") {
        prepare();
        return new Promise(() => {});
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  const selectedSkill = skill({
    name: "demo",
    deployments: [deployment({ agent: "claude-code", scope: "global", path: "/skills/demo" })],
  });
  render(
    <SkillAssistantPanel
      skill={selectedSkill}
      rawContent="# Demo"
      skillMdPath="/skills/demo/SKILL.md"
      isPluginManaged={false}
      onApplied={() => {}}
      onDiskChanged={() => {}}
      showHistory={false}
      onOpenHistory={() => {}}
      onCloseHistory={() => {}}
    />,
  );
  const user = userEvent.setup();
  await user.type(screen.getByLabelText("Question about this skill"), "Explain this skill");
  await user.click(screen.getByText("Advanced tools"));
  await user.click(screen.getByRole("button", { name: "Test in a project" }));
  await user.type(screen.getByPlaceholderText(/What should the skill do/), "Write a report");
  await user.click(screen.getByRole("button", { name: "Run test" }));
  await waitFor(() => expect(prepare).toHaveBeenCalledOnce());
  expect(screen.queryByRole("button", { name: "Ask" })).toBeNull();
  for (const name of ["Test in a project", "Review skill instructions", "Run test"]) {
    expect(screen.getByRole<HTMLButtonElement>("button", { name }).disabled).toBe(true);
  }
  expect(screen.getByRole<HTMLButtonElement>("combobox", { name: "Agent" }).disabled).toBe(true);
});

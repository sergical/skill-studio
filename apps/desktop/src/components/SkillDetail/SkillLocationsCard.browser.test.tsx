import { afterEach, expect, it } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "vitest-browser-react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { deployment, skill } from "../../dev/harness/skill-fixture";
import { SkillLocationsCard } from "./SkillLocationsCard";
import "../../App.css";

afterEach(() => {
  clearMocks();
  return cleanup();
});

it("keyboard park restores focus to the moved Locations switch after the snapshot instead of the page body", async () => {
  mockIPC((command) => {
    if (command === "park_skill") return {};
    throw new Error(`unexpected ${command}`);
  });
  const live = deployment({ agent: "Codex", scope: "global", path: "/fixture/.codex/skills/demo" });
  const fixture = skill({ name: "demo", source_kind: "manual", deployments: [live] });
  const view = await render(<SkillLocationsCard skill={fixture} />);
  const toggle = page.getByRole("switch", { name: "Global · Codex copy" });
  for (let i = 0; i < 12 && document.activeElement !== toggle.element(); i++)
    await userEvent.keyboard("{Tab}");
  await expect.element(toggle).toHaveFocus();
  await userEvent.keyboard(" ");
  await expect.element(toggle).toHaveAttribute("aria-checked", "false");
  const parked = {
    ...live,
    id: "parked",
    scope: "parked",
    path: "/fixture/.agents/skills-parked/demo",
    parked_origin: { kind: "codex", scope: "global" },
  };
  await view.rerender(<SkillLocationsCard skill={{ ...fixture, deployments: [parked] }} />);
  const restored = page.getByRole("switch", { name: "Global · Codex copy" });
  await expect.element(restored).toHaveFocus();
  await expect.element(restored).toHaveAttribute("aria-checked", "false");
});

it("project park confirmation keeps the switch disabled until the snapshot instead of allowing a second park", async () => {
  mockIPC((command) => {
    if (command === "park_check") return { git_tracked: false };
    if (command === "park_skill") return {};
    throw new Error(`unexpected ${command}`);
  });
  const live = deployment({
    agent: "Codex",
    scope: "project",
    project_path: "/fixture/project",
    path: "/fixture/project/.codex/skills/demo",
  });
  await render(
    <SkillLocationsCard
      skill={skill({ name: "demo", source_kind: "manual", deployments: [live] })}
    />,
  );
  const toggle = page.getByRole("switch");
  await toggle.click();
  await expect.element(page.getByRole("alertdialog")).toBeVisible();
  await page.getByRole("button", { name: "Park", exact: true }).click();
  await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
  await expect.element(toggle).toHaveAttribute("aria-checked", "false");
  await expect.element(toggle).toHaveAttribute("aria-disabled", "true");
});

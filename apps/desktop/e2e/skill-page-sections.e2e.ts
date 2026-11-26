import { expect, test } from "@playwright/test";
import { openApp, openSkillPage } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

test("a long description opens and closes with Show more, and a short one shows whole", async ({
  page,
}) => {
  await openSkillPage(page, "incident-postmortem");
  const description = page.getByText(/^Write a blameless incident postmortem/);
  const height = async () => (await description.boundingBox())?.height ?? 0;
  const more = page.getByRole("button", { name: "Show more" });
  await expect(more).toHaveAttribute("aria-expanded", "false");
  const collapsed = await height();

  await more.click();
  const less = page.getByRole("button", { name: "Show less" });
  await expect(less).toHaveAttribute("aria-expanded", "true");
  await expect.poll(height).toBeGreaterThan(collapsed * 1.3);

  await less.click();
  await expect(more).toBeVisible();
  await expect.poll(height).toBe(collapsed);

  await openSkillPage(page, "commit");
  await expect(page.getByText(/^Create clear, conventional Git commits/)).toBeVisible();
  await expect(page.getByRole("button", { name: /^Show (more|less)$/ })).toHaveCount(0);
});

test("the agent picker for a skill lists only the agents that have a copy of it", async ({
  page,
}) => {
  // Its one copy is in ~/.agents/skills, which Codex and Cursor read and Claude Code does not.
  await openSkillPage(page, "incident-postmortem");
  const section = page.getByRole("region", { name: "What agents see of this skill" });
  await section.getByRole("combobox", { name: "Agent for this skill" }).click();
  await expect(page.getByRole("option")).toHaveText(["Codex", "Cursor"]);
  await page.getByRole("option", { name: "Cursor" }).click();
  await expect(section).toContainText(".agents/skills/incident-postmortem");
  await expect(section).not.toContainText("No matching copy");
});

import { expect, test } from "@playwright/test";
import { openApp, openSkillsPage, skillRow } from "./app-harness";

const PLUGIN_SKILL = "git-worktree-helper";

test("a plugin-only skill with an update is counted and listed the same on Home, the sidebar and Skills", async ({
  page,
}) => {
  await openApp(page);
  // The estate is one skill that only a plugin ships, and its plugin has an update.
  await page.evaluate(async (name) => {
    const harness = window.__harness!;
    const snapshot = harness.snapshot();
    const template = snapshot.skills.find((skill) => skill.update_owner_ids.length)!;
    const skill = snapshot.skills.find((s) => s.name === name)!;
    await harness.publish({
      ...snapshot,
      skills: [
        {
          ...skill,
          has_update: true,
          update_owner_ids: template.update_owner_ids,
          update_owners: template.update_owners,
          update_commit: template.update_commit,
        },
      ],
      update_check: { ...snapshot.update_check, updates_available: 1 },
    });
  }, PLUGIN_SKILL);

  await expect(page.getByRole("button", { name: "Review 1 update" })).toBeVisible();
  const skillsPlace = page.getByRole("navigation").getByRole("button", { name: /^Skills/ });
  await expect(skillsPlace).toHaveText(/^Skills\s*1$/);

  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  await expect(grid.getByRole("button", { name: "Updates, 1 skill" })).toBeVisible();
  await expect(grid.getByRole("checkbox", { name: /^Select / })).toHaveCount(1);
  await expect(skillRow(page, PLUGIN_SKILL)).toHaveCount(1);

  // A search that matches nothing offers to clear it, and clearing it brings the row back.
  await page.getByRole("textbox", { name: "Filter skills" }).fill("no-such-skill");
  await page.getByRole("button", { name: "Clear filters" }).click();
  await expect(skillRow(page, PLUGIN_SKILL)).toHaveCount(1);
});

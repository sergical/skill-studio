import { expect, test } from "@playwright/test";
import {
  askedTitles,
  cancelNextAsk,
  openApp,
  openSidebarPage,
  openSkillPage,
  openSkillsPage,
  skillPageTitle,
  skillRow,
  toasts,
} from "./app-harness";

const SKILL = "code-review";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

test("Remove on the skill page asks first, Cancel keeps the skill, and Remove takes it off the Skills list", async ({
  page,
}) => {
  await openSkillPage(page, SKILL);
  const more = page.getByRole("button", { name: "More actions", exact: true });

  await cancelNextAsk(page);
  await more.click();
  await page.getByRole("menuitem", { name: "Remove" }).click();
  await expect.poll(() => askedTitles(page)).toHaveLength(1);
  await expect(skillPageTitle(page, SKILL)).toBeVisible();

  await more.click();
  await page.getByRole("menuitem", { name: "Remove" }).click();

  await expect(toasts(page).getByText("Removed")).toBeVisible();
  await openSkillsPage(page, "Skills", SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(0);
});

test("a removed skill shows in Activity, and Restore there puts it back on the Skills list", async ({
  page,
}) => {
  await openSkillPage(page, SKILL);
  await page.getByRole("button", { name: "More actions", exact: true }).click();
  await page.getByRole("menuitem", { name: "Remove" }).click();
  await expect(toasts(page).getByText("Removed")).toBeVisible();

  await openSidebarPage(page, "Activity");
  const history = page.getByRole("main");
  const removed = history.getByText("Removed", { exact: true });
  await expect(removed).toBeVisible();
  // A History row shows Restore while the pointer is on the row.
  await removed.hover();
  await history.getByRole("button", { name: "Restore" }).click();

  await expect(toasts(page).getByText("Restored")).toBeVisible();
  await expect(history.getByRole("button", { name: "Restore" })).toHaveCount(0);
  await openSkillsPage(page, "Skills", SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(1);
});

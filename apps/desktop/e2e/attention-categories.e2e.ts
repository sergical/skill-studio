import { expect, test } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";
import { openApp, openSkillsPage } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

/** The skill names a list's rows stand for, top to bottom, each once: a Home issue row repeats
 * its skill when the skill has two issues of one category. */
async function skillNames(rows: Locator): Promise<string[]> {
  const labels = await rows
    .getByRole("checkbox", { name: /^Select / })
    .evaluateAll((boxes) => boxes.map((box) => box.getAttribute("aria-label") ?? ""));
  return [...new Set(labels.map((label) => label.replace(/^Select /, "")))];
}

/** Presses the Home to-do whose button starts with `review` and reads the one group it shows. */
async function homeReview(page: Page, review: RegExp, group: string): Promise<string[]> {
  await page
    .getByRole("list", { name: "Things that need you" })
    .getByRole("button", { name: review })
    .click();
  const shown = page.getByRole("grid", { name: "Home" }).locator(`[data-group="${group}"]`);
  await expect(shown.getByRole("checkbox").first()).toBeAttached();
  return skillNames(shown);
}

/** Picks one Status in the Skills Filter menu and reads the rows it lists, top to bottom. */
async function skillsFilter(page: Page, status: string): Promise<string[]> {
  await openSkillsPage(page);
  await page.getByRole("button", { name: /^Filter, / }).click();
  await page.getByRole("menuitemradio", { name: new RegExp(`^${status} \\d+$`) }).click();
  const grid = page.getByRole("grid", { name: "Skills" });
  await expect(grid.getByRole("button", { name: new RegExp(`^${status}, `) })).toBeVisible();
  return skillNames(grid);
}

for (const { review, group, status } of [
  { review: /^Review \d+ broken skills?$/, group: "broken", status: "Broken" },
  { review: /^Review \d+ warnings?$/, group: "warn", status: "Warnings" },
  { review: /^Review \d+ updates?$/, group: "upd", status: "Updates" },
]) {
  test(`Home's ${status} to-do and the Skills ${status} filter list the same skills in the same order`, async ({
    page,
  }) => {
    const home = await homeReview(page, review, group);
    expect(home.length).toBeGreaterThan(0);

    const skills = await skillsFilter(page, status);
    // Home shows the first few rows of a group and links the rest to Skills.
    expect(skills.slice(0, home.length)).toEqual(home);
  });
}

test("with no Status picked, Skills lists Broken, then Warnings, then Updates", async ({
  page,
}) => {
  await openSkillsPage(page);
  const groups = page
    .getByRole("grid", { name: "Skills" })
    .getByRole("button", { name: /^(Broken|Warnings|Updates|Healthy|Parked), \d+ skills?$/ });
  await expect(groups.first()).toBeVisible();
  const labels = (await groups.allTextContents()).map((text) => text.replace(/\d+$/, ""));
  expect(labels.slice(0, 4)).toEqual(["Broken", "Warnings", "Updates", "Healthy"]);
});

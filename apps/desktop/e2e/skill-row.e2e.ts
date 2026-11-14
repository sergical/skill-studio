import { expect, test, type Page } from "@playwright/test";
import { openApp, reinstallParkedSkill } from "./app-harness";

/** A skill row, not a group header: only skill rows carry `aria-selected`. */
const SKILL_ROW = '[role="row"][aria-selected]';

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

async function openSkillsPage(page: Page) {
  await page
    .getByRole("button", { name: /^Skills/ })
    .first()
    .click();
  await expect(page.getByRole("grid", { name: "Skills" })).toBeVisible();
}

/** The x offsets, from the grid's left edge, of the first group label and the first row's name
 * and harness stack. */
async function rowGeometry(page: Page, gridName: string) {
  const grid = page.getByRole("grid", { name: gridName });
  await expect(grid.locator(SKILL_ROW).first()).toBeVisible();
  return grid.evaluate((el, sel) => {
    const left = el.getBoundingClientRect().left;
    const label = el.querySelector("[data-group-header] > span")!;
    const row = el.querySelector(sel)!;
    const name = row.querySelector('[role="gridcell"]:nth-child(2) span.truncate')!;
    const stack = row.querySelector('[role="gridcell"]:nth-child(4) > *')!;
    return {
      label: Math.round(label.getBoundingClientRect().left - left),
      name: Math.round(name.getBoundingClientRect().left - left),
      stack: Math.round(stack.getBoundingClientRect().left - left),
    };
  }, SKILL_ROW);
}

test("a skill row lines its name up under its group label and uses the same columns on Home and Skills", async ({
  page,
}) => {
  const home = await rowGeometry(page, "Home");
  expect(home.name).toBe(home.label);

  await openSkillsPage(page);
  const skills = await rowGeometry(page, "Skills");
  expect(skills.name).toBe(skills.label);
  expect(skills.name).toBe(home.name);
  expect(skills.stack).toBe(home.stack);
});

test("X on a focused Home row selects its skill and shows the selection bar with its name, and Escape clears it", async ({
  page,
}) => {
  const row = page.getByRole("grid", { name: "Home" }).locator(SKILL_ROW).first();
  const name = (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
  await row.focus();

  await page.keyboard.press("x");
  await expect(row).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("checkbox", { name: `Select ${name}` })).toBeChecked();
  await expect(page.getByRole("button", { name: "Open", exact: true })).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(row).toHaveAttribute("aria-selected", "false");
  await expect(page.getByRole("button", { name: "Open", exact: true })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
});

test("clicking the checkbox in a Home row's slot selects the skill and stays on Home", async ({
  page,
}) => {
  const row = page.getByRole("grid", { name: "Home" }).locator(SKILL_ROW).first();
  const name = (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
  await row.hover();

  await page.getByRole("checkbox", { name: `Select ${name}` }).click();

  await expect(row).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open", exact: true })).toBeVisible();
});

test("Tab from a focused Skills row with a fix reaches its checkbox and then its named fix button", async ({
  page,
}) => {
  const name = await reinstallParkedSkill(page);
  await openSkillsPage(page);
  const row = page.getByRole("grid", { name: "Skills" }).getByRole("row").filter({ hasText: name });
  const fix = row.locator('[role="gridcell"]:last-child button');
  await expect(fix).toHaveCount(1);
  const fixLabel = (await fix.textContent())!.trim();
  expect(fixLabel).not.toBe("");

  await row.focus();
  await page.keyboard.press("Tab");
  await expect(row.getByRole("checkbox", { name: `Select ${name}` })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(row.getByRole("button", { name: fixLabel, exact: true })).toBeFocused();
});

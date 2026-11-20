import { expect, test } from "@playwright/test";
import type { InstalledSkill } from "@skill-studio/lib";
import { HARNESS_HOME } from "../src/dev/harness/skill-fixture";
import { openApp, reinstallParkedSkill } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

test("Check now on the Updates tile runs a check and changes the caption from never checked to checked", async ({
  page,
}) => {
  await page.evaluate(() => {
    const harness = window.__harness;
    if (!harness) throw new Error("dev harness missing");
    const snapshot = harness.snapshot();
    return harness.publish({
      ...snapshot,
      update_check: { ...snapshot.update_check, checked_at: null },
    });
  });
  const tile = page.getByRole("button", { name: /^Updates/ }).first();
  await expect(tile).toContainText("Not checked yet");

  await page.getByRole("button", { name: "Check now" }).click();

  await expect(tile).toContainText("Checked");
  await expect(tile).not.toContainText("Not checked yet");
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("a left-behind skill offers one Resolve menu, opened from the keyboard, whose choice opens the resolve dialog on Home", async ({
  page,
}) => {
  const name = await reinstallParkedSkill(page);
  const row = page.getByRole("row").filter({ hasText: name });
  const resolve = row.getByRole("button", { name: "Resolve" });
  await expect(resolve).toBeVisible();

  await resolve.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menuitem", { name: "Keep live" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Keep parked" })).toBeVisible();
  await page.getByRole("menuitem", { name: "Keep live" }).click();

  await expect(
    page.getByRole("alertdialog", { name: `Keep the live copy of ${name}?` }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
});

test("every Home row action, including Compare and Resolve, shows its whole label inside the action column", async ({
  page,
}) => {
  await reinstallParkedSkill(page);
  await expect(page.getByRole("button", { name: "Resolve" })).toBeVisible();

  const actions = await page
    .locator('[role="row"] > [role="gridcell"]:last-child button')
    .evaluateAll((buttons) =>
      buttons.map((button) => {
        const cell = button.closest('[role="gridcell"]')!.getBoundingClientRect();
        const box = button.getBoundingClientRect();
        return {
          label: button.textContent?.trim() ?? "",
          fits:
            box.left >= cell.left - 0.5 &&
            box.right <= cell.right + 0.5 &&
            button.scrollWidth <= button.clientWidth,
        };
      }),
    );

  expect(actions.map((action) => action.label)).toEqual(
    expect.arrayContaining(["Compare", "Resolve"]),
  );
  expect(actions.filter((action) => !action.fits).map((action) => action.label)).toEqual([]);
});

test("Park on an idle Home row takes the skill out of Who can invoke and the idle prompt cost", async ({
  page,
}) => {
  const invoke = page.getByRole("group", { name: "Who can invoke" });
  const idle = page
    .getByRole("group", { name: "Prompt cost" })
    .getByRole("button", { name: /skills not used in 30 days/ });
  // "12 you or the model", "3 model only", "4 you only": the three segments add up to the total.
  const invokeTotal = async () =>
    (await invoke.getByRole("button").allTextContents())
      .map((text) => Number.parseInt(text, 10))
      .reduce((sum, count) => sum + count, 0);
  const idleCount = async () =>
    Number((await idle.textContent())?.match(/(\d+) skills not used/)?.[1]);
  const invokeBefore = await invokeTotal();
  const idleBefore = await idleCount();
  expect(idleBefore).toBeGreaterThan(0);

  // Park moves one folder and the links into it; a skill whose only folder is the Universal one
  // has no copy left on after it.
  const universalOnly = await page.evaluate(
    () =>
      window.__harness
        ?.snapshot()
        .skills.filter((skill) => skill.deployments.length === 1)
        .map((skill) => skill.name) ?? [],
  );
  // The idle segment shows the group; its first rows are the skills the model can invoke.
  await idle.click();
  const rows = page.locator('[data-group="unused"]').getByRole("row");
  const names = await rows.getByRole("gridcell").allTextContents();
  const target = names.find((name) => universalOnly.includes(name));
  expect(target).toBeDefined();
  await rows
    .filter({ has: page.getByRole("gridcell", { name: target, exact: true }) })
    .getByRole("button", { name: "Park", exact: true })
    .click();

  await expect.poll(idleCount).toBe(idleBefore - 1);
  expect(await invokeTotal()).toBe(invokeBefore - 1);
});

test("a row parked on Home stays selectable at its parked path, and Show all counts the skills still on", async ({
  page,
}) => {
  await page
    .getByRole("group", { name: "Prompt cost" })
    .getByRole("button", { name: /skills not used in 30 days/ })
    .click();
  const group = page.locator('[data-group="unused"]');
  const first = group.locator('[role="row"][aria-selected]').first();
  const name = (await first.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
  const row = group
    .locator('[role="row"][aria-selected]')
    .filter({ has: page.getByRole("checkbox", { name: `Select ${name}`, exact: true }) });
  await row.getByRole("button", { name: "Park", exact: true }).click();
  await expect(row.getByText("Parked", { exact: true })).toBeVisible();

  const header = await group.locator("[data-group-header]").getAttribute("aria-label");
  const stillOn = header?.match(/(\d+) skills?$/)?.[1];
  await expect(group.getByRole("button", { name: /^Show all/ })).toHaveText(`Show all ${stillOn}`);

  // The real park moves the folder; the harness mock leaves the path as it was.
  await page.evaluate(
    ([skillName, parkedPath]) =>
      window.__harness?.updateSkill(skillName, (skill: InstalledSkill) => ({
        ...skill,
        deployments: skill.deployments.map((copy) => ({
          ...copy,
          agent: "parked",
          scope: "parked",
          path: parkedPath,
        })),
      })),
    [name, `${HARNESS_HOME}/.agents/skills-parked/universal/${name}`] as const,
  );
  await row.focus();
  await page.keyboard.press("x");

  // The selection bar shows only when the checked row matches a skill in the snapshot.
  await expect(row).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("button", { name: "Cancel", exact: true })).toBeVisible();
});

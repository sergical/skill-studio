import { expect, test } from "@playwright/test";
import type { InstalledSkill } from "@skill-studio/lib";
import { HARNESS_HOME } from "../src/dev/harness/skill-fixture";
import { openApp, reinstallParkedSkill } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

test("Check for updates on Home runs a check and changes the caption from never checked to checked", async ({
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
  const section = page.getByRole("region", { name: "Your skills" });
  await expect(section).toContainText("Not checked yet");

  await section.getByRole("button", { name: "Check for updates" }).click();

  await expect(section).toContainText("Checked");
  await expect(section).not.toContainText("Not checked yet");
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

test("a row parked on Home stays selectable at its parked path, and Show all counts the skills still on", async ({
  page,
}) => {
  await page
    .getByRole("list", { name: "Things that need you" })
    .getByRole("listitem")
    .filter({ hasText: "no use in 30 days" })
    .getByRole("button", { name: "Review unused" })
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

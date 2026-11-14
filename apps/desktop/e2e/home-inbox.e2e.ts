import { expect, test } from "@playwright/test";
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

import { expect, test } from "@playwright/test";
import { openApp, reinstallParkedSkill } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

test("Check now on the Updates tile runs a check and shows when it ran, with no error", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Check now" }).click();

  await expect(page.getByRole("button", { name: /^Updates/ }).first()).toContainText("Checked");
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

test("Compare on a warning row is a visible button that does not need hover", async ({ page }) => {
  const row = page.getByRole("row").filter({ hasText: "Copies differ" }).first();

  await expect(row.getByRole("button", { name: "Compare" })).toBeVisible();
});

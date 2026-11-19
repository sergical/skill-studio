import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Model listing" }).click();
  await expect(page.getByRole("heading", { name: "Model listing", level: 1 })).toBeVisible();
});

test("a larger budget marks the skills that gain a description and gives the setting to paste", async ({
  page,
}) => {
  const rows = page.getByRole("list", { name: "Listing" }).getByRole("listitem");
  await expect(rows.first()).toBeVisible();
  const nameOnlyBefore = await rows.filter({ hasText: "Name only" }).count();
  expect(nameOnlyBefore).toBeGreaterThan(0);

  const budget = page.getByRole("textbox", {
    name: "Try a budget, in percent of the context window",
  });
  await budget.fill("2");
  await budget.press("Enter");

  await expect(
    page.getByText(/^At 60,000 characters: \d+ gain a description, 0 lose one\.$/),
  ).toBeVisible();
  await expect(page.locator("li[data-changed]")).toHaveCount(nameOnlyBefore);

  await page.getByRole("button", { name: "Show only changes" }).click();
  await expect(rows).toHaveCount(nameOnlyBefore);

  const setting = page.getByRole("region", { name: "Listing setting" });
  await expect(setting).toContainText('"skillListingBudgetFraction": 0.02');
  await expect(setting).toContainText("chezmoi apply");
});

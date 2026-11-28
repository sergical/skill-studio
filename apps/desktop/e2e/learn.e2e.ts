import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test("Learn more on a Home tile opens Learn with that section's heading focused", async ({
  page,
}) => {
  await openApp(page);

  await page.getByRole("button", { name: "About invocation" }).click();
  await page.getByRole("button", { name: /Learn more/ }).click();

  await expect(page.getByRole("heading", { name: "Learn", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Who can invoke a skill" })).toBeFocused();
});

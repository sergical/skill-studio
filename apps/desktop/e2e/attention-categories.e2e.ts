import { expect, test } from "@playwright/test";
import { openApp, openSkillsPage } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

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

import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Sessions" }).click();
  await expect(page.getByRole("heading", { name: "Sessions", level: 1 })).toBeVisible();
});

test("the Sessions page groups skills by finding and lists the sessions behind them", async ({
  page,
}) => {
  await expect(page.getByRole("region", { name: "What the words mean" })).toContainText(
    "Name only",
  );
  await expect(
    page.getByText("3 sessions read: 1 used a skill, 1 possibly missed one."),
  ).toBeVisible();

  const nameOnly = page.getByRole("region", { name: "Used while listed by name only" });
  await expect(nameOnly.getByRole("listitem")).toHaveCount(1);
  const neverUsed = page.getByRole("region", {
    name: "Listed in nearly every session, never used",
  });
  await expect(neverUsed.getByRole("listitem")).toHaveCount(1);
  await expect(
    page.getByRole("region", { name: "Possibly missed" }).getByRole("listitem"),
  ).toHaveCount(1);

  // Only sessions with a finding show first; the toggle shows every session.
  const sessions = page.getByRole("region", { name: "Sessions" }).getByRole("listitem");
  await expect(sessions).toHaveCount(2);
  await page.getByRole("button", { name: "Show all 3" }).click();
  await expect(sessions).toHaveCount(3);

  // An agent that records no listing says so instead of showing empty sessions.
  await page.getByRole("combobox", { name: "Agent" }).click();
  await page.getByRole("option", { name: "Cursor" }).click();
  await expect(page.getByText("This agent does not record its listing.")).toBeVisible();
  await expect(page.getByRole("region", { name: "Sessions" })).toHaveCount(0);
});

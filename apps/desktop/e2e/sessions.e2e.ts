import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Agents" }).click();
  await page.getByRole("button", { name: "What it used", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Agents", level: 1 })).toBeVisible();
});

test("the What it used tab groups skills by finding and lists the sessions behind them", async ({
  page,
}) => {
  await page.getByText("Findings and what the words mean", { exact: true }).click();
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

  // Open the recorded work, then narrow the list to sessions with a finding.
  const sessions = page
    .getByRole("region", { name: "Recent sessions" })
    .locator(":scope > ul > li");
  await expect(sessions).toHaveCount(3);
  await sessions.first().locator("summary").first().click();
  await expect(sessions.first().getByText(/Listed \d+ skills/)).toBeVisible();
  await sessions
    .first()
    .getByText(/Skill list in this session ·/)
    .click();
  await expect(page.getByRole("list", { name: "Recorded skills" })).toContainText(
    "Original wording from the session.",
  );
  const captured = page
    .getByRole("listitem")
    .filter({ has: page.getByText("Original wording from the session.", { exact: true }) })
    .last();
  await expect(captured).toContainText("Shortened");
  await page.getByRole("button", { name: "Show only sessions with findings" }).click();
  await expect(sessions).toHaveCount(2);

  await page.getByText("Findings and what the words mean", { exact: true }).click();

  // An agent that records no listing says so instead of showing empty sessions.
  await page.getByRole("combobox", { name: "Agent" }).click();
  await page.getByRole("option", { name: "Cursor" }).click();
  await expect(
    page.getByText("Skill Studio does not yet read saved listings for Cursor."),
  ).toBeVisible();
  await expect(page.getByRole("region", { name: "Recent sessions" })).toHaveCount(0);
});

import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Agents" }).click();
  await page.getByRole("tab", { name: "What it used" }).click();
  await expect(page.getByRole("heading", { name: "Agents", level: 1 })).toBeVisible();
});

test("the What it used tab groups skills by finding and lists the sessions behind them", async ({
  page,
}) => {
  const used = page.getByRole("region", { name: "What Claude Code used" });
  await expect(used).toContainText("1 of 3");
  await expect(used).toContainText("1 possibly missed one");
  await page.getByText("What the words mean", { exact: true }).click();
  await expect(page.getByRole("region", { name: "What the words mean" })).toContainText(
    "Name only",
  );

  // One row per skill, each naming its findings.
  const findings = page.getByRole("list", { name: "Skills with findings" }).getByRole("listitem");
  await expect(findings).toHaveCount(2);
  await expect(findings.filter({ hasText: "Used by name only" })).toHaveCount(1);
  await expect(findings.filter({ hasText: "Possibly missed" })).toHaveCount(1);

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

  // An agent that records no listing says so instead of showing empty sessions.
  await page.getByRole("combobox", { name: "Agent" }).click();
  await page.getByRole("option", { name: "Cursor" }).click();
  await expect(
    page.getByText("Skill Studio does not yet read saved listings for Cursor."),
  ).toBeVisible();
  await expect(page.getByRole("region", { name: "Recent sessions" })).toHaveCount(0);
});

test("Unused names the sessions read and leaves out a skill the 30-day history shows as used", async ({
  page,
}) => {
  // code-review is listed in all 3 sessions and used in none, but has uses in the last 30 days.
  const findings = page.getByRole("list", { name: "Skills with findings" });
  await expect(findings.getByRole("listitem")).toHaveCount(2);
  await expect(findings.getByRole("listitem", { name: "code-review" })).toHaveCount(0);

  await page.evaluate(async () => {
    const harness = window.__harness!;
    const snapshot = harness.snapshot();
    await harness.publish({
      ...snapshot,
      invocations: snapshot.invocations.filter((stats) => stats.skill !== "code-review"),
    });
  });
  await expect(findings.getByRole("listitem", { name: "code-review" })).toContainText(
    "Unused in these 3 sessions with a skill list",
  );
});

test("Uses in 30 days says no uses, not no readable history, for an agent whose sessions were read", async ({
  page,
}) => {
  // Claude Code's 3 sessions were read, Cursor's none; drop both agents' uses from the history.
  await page.evaluate(async () => {
    const harness = window.__harness!;
    const snapshot = harness.snapshot();
    await harness.publish({
      ...snapshot,
      invocations: snapshot.invocations.map((stats) => ({
        ...stats,
        by_harness_30_days: { ...stats.by_harness_30_days, "claude-code": 0, cursor: 0 },
      })),
    });
  });
  const uses = page.getByRole("region", { name: "Uses in 30 days" });
  await expect(uses).toContainText("No uses in 30 days");
  await expect(uses).not.toContainText("No readable history");

  await page.getByRole("combobox", { name: "Agent" }).click();
  await page.getByRole("option", { name: "Cursor" }).click();
  await expect(uses).toContainText("No readable history");
});

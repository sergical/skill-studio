import { expect, test, type Locator } from "@playwright/test";
import { HISTORY_DAYS } from "@skill-studio/lib";
import { openApp } from "./app-harness";

/** Index of the calendar's active day among its day cells. */
function activeIndex(calendar: Locator): Promise<number> {
  return calendar.evaluate((el) => {
    const id = el.getAttribute("aria-activedescendant");
    return [...el.querySelectorAll('[role="option"]')].findIndex((o) => o.id === id);
  });
}

test("the Activity calendar shows 30 days in weekday columns, and arrows move by day and week", async ({
  page,
}) => {
  await openApp(page);
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  const calendar = page.getByRole("listbox", { name: /^Uses per day/ });
  const days = calendar.getByRole("option");
  await expect(days).toHaveCount(HISTORY_DAYS);

  // A day and the same weekday one week earlier sit in the same column.
  const today = HISTORY_DAYS - 1;
  const [todayBox, weekAgoBox] = await Promise.all([
    days.nth(today).boundingBox(),
    days.nth(today - 7).boundingBox(),
  ]);
  expect(todayBox?.x).toBe(weekAgoBox?.x);
  expect(todayBox!.y).toBeGreaterThan(weekAgoBox!.y);

  await calendar.focus();
  expect(await activeIndex(calendar)).toBe(today);
  await page.keyboard.press("ArrowLeft");
  expect(await activeIndex(calendar)).toBe(today - 1);
  await page.keyboard.press("ArrowUp");
  expect(await activeIndex(calendar)).toBe(today - 8);
  await page.keyboard.press("ArrowDown");
  expect(await activeIndex(calendar)).toBe(today - 1);
  await page.keyboard.press("ArrowRight");
  expect(await activeIndex(calendar)).toBe(today);
});

test("Enter on a day puts that day's skills in place of the 30-day list, and Escape brings the list back", async ({
  page,
}) => {
  await openApp(page);
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  const skills = page.getByRole("heading", { name: "Skills", level: 2 });
  await expect(skills).toBeVisible();

  const calendar = page.getByRole("listbox", { name: /^Uses per day/ });
  await calendar.focus();
  const today = await calendar.evaluate((el) =>
    document.getElementById(el.getAttribute("aria-activedescendant")!)!.getAttribute("aria-label"),
  );
  await page.keyboard.press("Enter");
  // The day's heading is its date, the part of the cell's name before the colon.
  await expect(page.getByRole("heading", { name: today!.split(":")[0], level: 2 })).toBeVisible();
  await expect(skills).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Previous day" })).toBeEnabled();

  await page.keyboard.press("Escape");
  await expect(skills).toBeVisible();
});

test("pressing an agent's row filters the page to that agent, and pressing it again shows every agent", async ({
  page,
}) => {
  await openApp(page);
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  const total = page
    .getByRole("main")
    .getByText(/^[\d,]+ uses$/)
    .first();
  const before = await total.innerText();
  const codex = page.getByRole("button", { name: /^Codex [\d,]+$/ });

  await codex.click();
  await expect(codex).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "Filter by agent" })).toHaveText(/Codex/);
  await expect(total).not.toHaveText(before);

  await codex.click();
  await expect(codex).toHaveAttribute("aria-pressed", "false");
  await expect(total).toHaveText(before);
});

test("Activity names only the agents kept at first run", async ({ page }) => {
  await page.goto("/?persona=two-agents");
  await page.getByRole("checkbox", { name: "Codex", exact: true }).press("Space");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await page.getByRole("button", { name: "Filter by agent" }).click();
  await expect(page.getByRole("menuitemradio", { name: /^Claude Code/ })).toBeVisible();
  await expect(page.getByRole("menuitemradio", { name: /^Codex/ })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByText(/No readable history from/)).toHaveCount(0);
});

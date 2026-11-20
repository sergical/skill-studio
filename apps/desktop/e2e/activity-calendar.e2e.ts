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

import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

const paletteInput = (page: Page) => page.getByRole("combobox", { name: "Command or skill" });

test("pressing ⌘K on Home focuses the palette input and arrow keys move the palette highlight, not the Home rows", async ({
  page,
}) => {
  await page.keyboard.press("ControlOrMeta+k");

  await expect(paletteInput(page)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(paletteInput(page)).toBeFocused();
  await expect(page.getByRole("option").nth(2)).toHaveAttribute("aria-selected", "true");
});

test("pressing ⌘K while a Home row has focus moves focus into the palette, and Escape returns it to that row", async ({
  page,
}) => {
  // The second row, so a ⌘K that leaks into the grid as "k" (up) would move focus off it.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  const row = page.locator('[role="row"]:focus');
  await expect(row).toHaveCount(1);
  const rowText = (await row.textContent()) ?? "";

  await page.keyboard.press("ControlOrMeta+k");
  await expect(paletteInput(page)).toBeFocused();
  await page.keyboard.press("Escape");

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator('[role="row"]:focus')).toHaveText(rowText);
});

test("typing a view name and pressing Enter in the palette opens that view and closes the palette", async ({
  page,
}) => {
  await page.keyboard.press("ControlOrMeta+k");
  await page.keyboard.type("Settings");
  await page.keyboard.press("Enter");

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
});

test("closing the palette with Escape keeps the page scroll where it was, even when the focused row is out of view", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 480 });
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  const scroller = page
    .locator(".overflow-y-scroll")
    .filter({ has: page.locator('[role="row"]:focus') });
  await scroller.evaluate((el) => el.scrollTo({ top: 0 }));
  await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBe(0);

  await page.keyboard.press("ControlOrMeta+k");
  await expect(paletteInput(page)).toBeFocused();
  await page.keyboard.press("Escape");

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator('[role="row"]:focus')).toHaveCount(1);
  expect(await scroller.evaluate((el) => el.scrollTop)).toBe(0);
});

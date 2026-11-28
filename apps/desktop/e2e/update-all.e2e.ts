import { expect, test } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";
import { setLatency } from "./app-harness";

const UPDATES_COUNT = /^\d+ updates?$/;
/** A skill row, not a group header: only skill rows carry `aria-selected`. */
const SKILL_ROW = '[role="row"][aria-selected]';

function home(page: Page): Locator {
  return page.getByRole("region", { name: "Your skills at a glance" });
}

async function updatesOnHome(page: Page): Promise<number> {
  const count = home(page).getByRole("button", { name: UPDATES_COUNT });
  if ((await count.count()) === 0) return 0;
  return Number((await count.textContent())?.match(/\d+/)?.[0]);
}

/** Opens Skills from Home's updates count and returns that count. */
async function openUpdatesFromHome(page: Page): Promise<number> {
  await page.goto("/?persona=everything-wrong");
  await page.getByRole("button", { name: "Continue" }).click();
  const shown = await updatesOnHome(page);
  expect(shown).toBeGreaterThan(2);
  await home(page).getByRole("button", { name: UPDATES_COUNT }).click();
  await expect(page.getByRole("heading", { name: "Skills", level: 1 })).toBeVisible();
  return shown;
}

async function backHome(page: Page) {
  await page.getByRole("navigation").getByRole("button", { name: "Home", exact: true }).click();
  await expect(home(page)).toBeVisible();
}

const grid = (page: Page) => page.getByRole("grid", { name: "Skills" });
const rowUpdateButtons = (page: Page) =>
  grid(page).getByRole("row").getByRole("button", { name: "Update", exact: true });

test("Update all on Skills filtered to Updates updates every row, and Home then counts no updates", async ({
  page,
}) => {
  const shown = await openUpdatesFromHome(page);

  await grid(page).getByRole("button", { name: "Update all" }).click();

  await expect(page.getByText(`Updated ${shown} of ${shown} skills`)).toBeVisible();
  await expect(rowUpdateButtons(page)).toHaveCount(0);
  await expect(grid(page).getByText("Updated", { exact: true }).first()).toBeVisible();

  await backHome(page);
  await expect(home(page).getByRole("button", { name: UPDATES_COUNT })).toHaveCount(0);
});

test("Stop while Update all runs updates the skill it is on and leaves the rest untouched", async ({
  page,
}) => {
  test.slow();
  const shown = await openUpdatesFromHome(page);
  // Each target then takes 400 ms, so the batch is still running when Stop lands.
  await setLatency(page, 400);

  await grid(page).getByRole("button", { name: "Update all" }).click();
  await expect(page.getByRole("status").filter({ hasText: /^Updating 1 of / })).toBeAttached();
  // Only the row being updated spins; the rest wait.
  await expect(grid(page).locator(SKILL_ROW).locator('button[aria-busy="true"]')).toHaveCount(1);
  await grid(page).getByRole("button", { name: "Stop" }).click();

  const toast = page.getByText(new RegExp(`^Stopped\\. Updated \\d+ of ${shown} skills\\.$`));
  await expect(toast).toBeVisible();
  const updated = Number((await toast.textContent())?.match(/Updated (\d+)/)?.[1]);
  expect(updated).toBeGreaterThan(0);
  expect(updated).toBeLessThan(shown);
  await expect(grid(page).getByRole("button", { name: "Update all" })).toBeVisible();
  await expect(rowUpdateButtons(page).first()).toBeVisible();

  await backHome(page);
  await expect.poll(() => updatesOnHome(page)).toBe(shown - updated);
});

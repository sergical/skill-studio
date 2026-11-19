// ============================================================================
// background-refresh - A background refresh never moves the page; only a
// failed one shows, on the sidebar's Sync button.
// ============================================================================

import { expect, test } from "@playwright/test";
import type { BackgroundStatus } from "@skill-studio/lib";
import { openApp } from "./app-harness";

function status(revision: number, running: boolean, lastError: string | null): BackgroundStatus {
  return {
    revision,
    tasks: [
      {
        worker: "invocations",
        running,
        last_started_at: "2026-01-01T00:00:00Z",
        last_finished_at: running ? null : "2026-01-01T00:00:01Z",
        last_error: lastError,
      },
    ],
  };
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page
    .getByRole("button", { name: /^Skills/ })
    .first()
    .click();
  await expect(page.getByRole("grid", { name: "Skills" })).toBeVisible();
});

test("a running background refresh leaves the Skills list where it is", async ({ page }) => {
  const grid = page.getByRole("grid", { name: "Skills" });
  const topBefore = await grid.evaluate((el) => el.getBoundingClientRect().top);

  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(1, true, null),
  );

  // The status event is delivered; the Sync button stays as it is and the list does not move.
  await expect(page.getByRole("button", { name: "Sync installed skills" })).toBeVisible();
  expect(await grid.evaluate((el) => el.getBoundingClientRect().top)).toBe(topBefore);
});

test("a failed background refresh turns Sync into a warning that shows the error and retries", async ({
  page,
}) => {
  const grid = page.getByRole("grid", { name: "Skills" });
  const topBefore = await grid.evaluate((el) => el.getBoundingClientRect().top);

  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(2, false, "transcript index is locked"),
  );

  const warning = page.getByRole("button", { name: "Sync failed, show error" });
  await expect(warning).toBeVisible();
  expect(await grid.evaluate((el) => el.getBoundingClientRect().top)).toBe(topBefore);

  await warning.click();
  const popover = page.getByRole("dialog", { name: "Sync error" });
  await expect(popover).toContainText("transcript index is locked");

  const revisionBefore = await page.evaluate(() => window.__harness!.snapshot().revision);
  await popover.getByRole("button", { name: "Try again" }).click();
  await expect(popover).toBeHidden();
  await expect
    .poll(() => page.evaluate(() => window.__harness!.snapshot().revision))
    .toBeGreaterThan(revisionBefore);

  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(3, false, null),
  );
  await expect(page.getByRole("button", { name: "Sync installed skills" })).toBeVisible();
});

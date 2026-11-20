// ============================================================================
// background-refresh - A background refresh never moves the page; only a
// failed one shows, on the sidebar's Sync button.
// ============================================================================

import { expect, test } from "@playwright/test";
import type { BackgroundStatus } from "@skill-studio/lib";
import { openApp, setLatency } from "./app-harness";

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

test("a failure after a cleared one shows the warning but does not open the error by itself", async ({
  page,
}) => {
  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(2, false, "first failure"),
  );
  await page.getByRole("button", { name: "Sync failed, show error" }).click();
  await expect(page.getByRole("dialog", { name: "Sync error" })).toBeVisible();

  // A background retry succeeds while the error is open, then a later one fails.
  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(3, false, null),
  );
  await expect(page.getByRole("dialog", { name: "Sync error" })).toBeHidden();
  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(4, false, "second failure"),
  );

  await expect(page.getByRole("button", { name: "Sync failed, show error" })).toBeVisible();
  await expect(page.getByRole("dialog", { name: "Sync error" })).toBeHidden();
});

test("Try again from the keyboard leaves focus on the Sync button", async ({ page }) => {
  // Every harness answer takes 800 ms, so the retry shows its spinning state as it does on disk.
  await setLatency(page, 800);
  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(2, false, "index locked"),
  );
  const sync = page.getByRole("button", { name: "Sync failed, show error" });
  await sync.focus();
  await page.keyboard.press("Enter");
  const popover = page.getByRole("dialog", { name: "Sync error" });
  await popover.getByRole("button", { name: "Try again" }).focus();
  const revisionBefore = await page.evaluate(() => window.__harness!.snapshot().revision);
  await page.keyboard.press("Enter");

  await expect(popover).toBeHidden();
  await expect
    .poll(() => page.evaluate(() => window.__harness!.snapshot().revision))
    .toBeGreaterThan(revisionBefore);
  // The retry can end before this check on a slow runner, and the button's name changes with
  // its state. The revision check above proves the retry ran; match the button in every state.
  await expect(
    page.getByRole("button", {
      name: /^(Syncing installed skills|Sync installed skills|Sync failed, show error)$/,
    }),
  ).toBeFocused();
});

test("a long error scrolls with the keyboard", async ({ page }) => {
  const longError = Array.from({ length: 80 }, (_, i) => `line ${i}: permission denied`).join("\n");
  await page.evaluate(
    (next) => window.__harness?.publishBackgroundStatus(next),
    status(2, false, longError),
  );
  await page.getByRole("button", { name: "Sync failed, show error" }).click();
  const details = page.getByRole("region", { name: "Error details" });

  await details.focus();
  await expect(details).toBeFocused();
  await page.keyboard.press("PageDown");
  await expect.poll(() => details.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
});

test("Sync without an error starts a rescan and opens nothing", async ({ page }) => {
  const revisionBefore = await page.evaluate(() => window.__harness!.snapshot().revision);
  const sync = page.getByRole("button", { name: "Sync installed skills" });
  await expect(sync).not.toHaveAttribute("aria-haspopup");
  await sync.click();

  await expect
    .poll(() => page.evaluate(() => window.__harness!.snapshot().revision))
    .toBeGreaterThan(revisionBefore);
  await expect(page.getByRole("dialog", { name: "Sync error" })).toHaveCount(0);
});

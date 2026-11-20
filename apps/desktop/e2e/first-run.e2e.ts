// ============================================================================
// first-run - the first-run screen with slow disk answers. Specs that need a
// slow harness set the delay after the app starts (setLatency), so this spec
// keeps the start itself covered with every answer held.
// ============================================================================

import { expect, test } from "@playwright/test";

test("Continue on the first-run screen reaches Home while every harness answer takes 400 ms", async ({
  page,
}) => {
  test.slow();
  await page.goto("/?latency=400");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
});

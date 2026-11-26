import { expect, test } from "@playwright/test";
import { PERSONAS } from "../src/dev/harness/personas";

// Every persona opens Home, gets the to-do its setup calls for first, and never sees a number the
// app could not compute. `PERSONA_SHOTS=<dir>` also saves Home and What it sees for a review.
for (const [id, persona] of Object.entries(PERSONAS)) {
  test(`Home puts the right thing first for ${persona.label}`, async ({ page }, info) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(`/?persona=${id}`);
    await page.getByRole("button", { name: "Continue" }).click();
    const todos = page.getByRole("region", { name: /need|Nothing needs you|No skills yet/ });
    await expect(todos).toBeVisible();
    if (persona.expectTop === null) {
      await expect(page.getByRole("list", { name: "Things that need you" })).toHaveCount(0);
    } else {
      await expect(
        page.getByRole("list", { name: "Things that need you" }).getByRole("listitem").first(),
      ).toContainText(persona.expectTop);
    }
    await expect(page.locator("body")).not.toContainText(/NaN|undefined|Infinity/);
    // Home claims only what the setup measured: no "unused" without history, no "fit" without an
    // estimate. These are the claims the persona review caught.
    const home = page.getByRole("region", { name: /need|Nothing needs you|No skills yet/ });
    if (id === "no-history" || id === "first-week")
      await expect(page.locator("body")).not.toContainText(
        /no use in 30 days|Not used in the last/,
      );
    if (id === "cursor-only")
      await expect(page.locator("body")).not.toContainText(/descriptions fit/);
    await expect(home).not.toContainText(/All clear/);
    expect(errors).toEqual([]);
    const dir = process.env.PERSONA_SHOTS;
    if (dir && info.project.name === "webkit") {
      await page.waitForTimeout(400);
      await page.screenshot({ path: `${dir}/${id}-home.png`, fullPage: true });
      await page.getByRole("navigation").getByRole("button", { name: "Agents" }).click();
      await page.waitForTimeout(400);
      await page.screenshot({ path: `${dir}/${id}-agents.png`, fullPage: true });
    }
  });
}

test("selecting costly, unused skills on the chart shows what parking them frees and selects them", async ({
  page,
}) => {
  await page.goto("/?persona=unused-pile");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Select costly, unused" }).click();
  await expect(page.getByRole("status").filter({ hasText: "selected" })).toContainText(
    /frees about [\d,]+ tokens in every message/,
  );
  await page.getByRole("button", { name: /^Select \d+ to park$/ }).click();
  await expect(page.getByRole("button", { name: "Cancel", exact: true })).toBeVisible();
});

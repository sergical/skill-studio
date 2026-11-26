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

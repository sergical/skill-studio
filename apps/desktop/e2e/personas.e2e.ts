import { expect, test } from "@playwright/test";
import { PERSONAS } from "../src/dev/harness/personas";

// Every persona opens Home, gets the item its setup calls for first in the prompt, and never sees
// a number the app could not compute. `PERSONA_SHOTS=<dir>` also saves Home and What it sees for a
// review.
for (const [id, persona] of Object.entries(PERSONAS)) {
  test(`Home puts the right thing first for ${persona.label}`, async ({ page }, info) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(`/?persona=${id}`);
    await page.getByRole("button", { name: "Continue" }).click();
    const home = page.getByRole("region", { name: /Your skills at a glance|No skills yet/ });
    await expect(home).toBeVisible();
    const lines = home.getByRole("list", { name: "Prompt lines" }).getByRole("listitem");
    if (persona.expectTop === null) {
      await expect(home.getByRole("button", { name: "Copy prompt for your agent" })).toHaveCount(0);
    } else {
      await home.getByRole("button", { name: "Show the prompt" }).click();
      await expect(lines.first()).toContainText(persona.expectTop);
    }
    await expect(page.locator("body")).not.toContainText(/NaN|undefined|Infinity/);
    // Home claims only what the setup measured: no "unused" without history, no skill list or
    // cut without an estimate. These are the claims the persona review caught.
    if (id === "no-history" || id === "first-week") {
      await expect(home).toContainText("No readable history yet");
      await expect(home).not.toContainText(/unused|not used in 30 days|\buses? this week/);
    }
    if (id === "cursor-only") {
      await expect(home).toContainText("No estimate");
      await expect(home).not.toContainText(/in skill list|name only|not listed/);
    }
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

import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

/** A skill row, not a group header: only skill rows carry `aria-selected`. */
const SKILL_ROW = '[role="row"][aria-selected]';

test("Pack on the selection bar plans the new pack folder, and Export gives the install line", async ({
  page,
}) => {
  await openApp(page);
  const rows = page.getByRole("grid", { name: "Home" }).locator(SKILL_ROW);
  const names: string[] = [];
  for (const index of [0, 1]) {
    const row = rows.nth(index);
    const name = (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
    await row.hover();
    await page.getByRole("checkbox", { name: `Select ${name}` }).click();
    names.push(name);
  }

  await page.getByRole("button", { name: "Pack", exact: true }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("heading", { name: "Pack 2 skills?" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText(
    /^Would write 2 skills and a README to .*my-skills\.$/,
  );
  const listed = dialog.getByRole("list", { name: "Skills in the pack" }).getByRole("listitem");
  await expect(listed).toHaveCount(2);
  await dialog.getByRole("textbox", { name: "Pack repository" }).fill("me/my-skills");

  await dialog.getByRole("button", { name: "Export" }).click();

  await expect(dialog.getByRole("heading", { name: "Pack written" })).toBeVisible();
  await expect(dialog.getByLabel("Install command")).toHaveText(
    `npx skills add me/my-skills --skill ${names[0]} --skill ${names[1]}`,
  );
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(dialog).toHaveCount(0);
});

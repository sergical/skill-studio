import { expect, test } from "@playwright/test";
import { openApp } from "./app-harness";

test("moving through the editor choices with arrow keys shows a focus outline on the focused row", async ({
  page,
}) => {
  await openApp(page);
  await page.getByRole("button", { name: "Settings" }).click();

  const group = page.getByRole("radiogroup", { name: "Open in editor" });
  await group.getByRole("radio", { checked: true }).focus();
  await page.keyboard.press("ArrowDown");

  const focusedRow = group.locator("label:has(:focus-visible)");
  await expect(focusedRow).toHaveCount(1);
  await expect(focusedRow).toHaveCSS("outline-style", "solid");
});

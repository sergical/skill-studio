import { expect, test } from "@playwright/test";
import { openSidebarPage } from "./app-harness";

test("an agent left out at first run gets no Search history from switch in Settings", async ({
  page,
}) => {
  await page.goto("/");
  const cursor = page.getByRole("checkbox", { name: "Cursor", exact: true });
  await expect(cursor).toBeChecked();
  await cursor.press("Space");
  await expect(cursor).not.toBeChecked();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();

  await openSidebarPage(page, "Settings");
  const history = page.getByText("Search history from", { exact: true }).locator("..");
  await expect(history.getByRole("switch", { name: "Claude Code" })).toBeVisible();
  await expect(history.getByRole("switch", { name: "Cursor" })).toHaveCount(0);
});

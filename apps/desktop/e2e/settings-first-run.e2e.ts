import { expect, test } from "@playwright/test";
import { openSidebarPage } from "./app-harness";

test("an agent left out at first run is off in Settings and gets no Find projects switch", async ({
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
  await expect(page.getByRole("switch", { name: "Show Cursor skills" })).not.toBeChecked();
  await expect(
    page.getByRole("switch", { name: "Find projects in Claude Code history" }),
  ).toBeVisible();
  await expect(page.getByRole("switch", { name: "Find projects in Cursor history" })).toHaveCount(
    0,
  );
});

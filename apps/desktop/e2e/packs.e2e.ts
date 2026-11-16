import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Packs" }).click();
  await expect(page.getByRole("heading", { name: "Packs", level: 1 })).toBeVisible();
});

const packRow = (page: Page, source: string) =>
  page.getByRole("list", { name: "Packs" }).getByRole("listitem", { name: source, exact: true });

test("updating a pack shows the plan first, and Confirm runs it and shows what each step did", async ({
  page,
}) => {
  await packRow(page, "getsentry/skills").getByRole("button", { name: "Update" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("heading", { name: "Update getsentry/skills?" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText(/^Would update \d+ skills; 0 skipped\.$/);
  const steps = dialog.getByRole("list", { name: "Steps" }).getByRole("listitem");
  await expect(steps.filter({ hasText: "sentry-triage" })).toHaveCount(2);
  await expect(steps.first()).toContainText("Will update");
  const planned = await steps.count();

  await dialog.getByRole("button", { name: `Update ${planned} skills` }).click();

  await expect(dialog.getByRole("heading", { name: "Done" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText(
    `Updated ${planned} skills; 0 skipped; 0 failed.`,
  );
  await expect(steps.filter({ hasText: "Will update" })).toHaveCount(0);
  await expect(steps.filter({ hasText: "Updated" })).toHaveCount(planned);
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(dialog).toHaveCount(0);
});

test("a skill with local edits is skipped with its reason, and with nothing to run Confirm is off", async ({
  page,
}) => {
  await packRow(page, "openapi-generator/openapi-generator")
    .getByRole("button", { name: "Update" })
    .click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("status")).toHaveText("Would update 0 skills; 2 skipped.");
  await expect(dialog.getByRole("list", { name: "Steps" })).toContainText(
    "has local edits, which the owner tool would overwrite or delete",
  );
  await expect(dialog.getByRole("button", { name: "Update", exact: true })).toBeDisabled();
});

test("a plugin pack's Update and Remove are off, and the tooltip names the agent command", async ({
  page,
}) => {
  const row = packRow(page, "changelog-formatter@community-plugins");
  const update = row.getByRole("button", { name: "Update" });
  await expect(update).toBeDisabled();
  await expect(row.getByRole("button", { name: "Remove" })).toBeDisabled();

  await update.locator("..").hover();
  await expect(
    page.getByText(
      "A plugin. The agent updates it with claude plugin update changelog-formatter@community-plugins.",
    ),
  ).toBeVisible();
});

test("Check upstream offers the pack's new skills, and adding them installs them", async ({
  page,
}) => {
  const row = packRow(page, "getsentry/skills");
  await expect(row.getByRole("button", { name: /^Add \d+ new$/ })).toHaveCount(0);

  await page.getByRole("button", { name: "Check upstream" }).click();
  await row.getByRole("button", { name: "Add 1 new" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("status")).toHaveText("Would install 1 skill; 0 skipped.");
  await expect(dialog.getByRole("list", { name: "Steps" })).toContainText("sentry-setup-logging");
  await dialog.getByRole("button", { name: "Add 1 skill" }).click();

  await expect(dialog.getByRole("status")).toHaveText("Installed 1 skill; 0 skipped; 0 failed.");
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(row.getByRole("button", { name: "Add 1 new" })).toHaveCount(0);
});

test("removing a pack asks first, and Confirm removes its skills and drops the row", async ({
  page,
}) => {
  await packRow(page, "anthropics/skills").getByRole("button", { name: "Remove" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("heading", { name: "Remove anthropics/skills?" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText("Would remove 1 skill; 0 skipped.");
  await expect(dialog.getByRole("list", { name: "Steps" })).toContainText(
    "pdf-processingGlobalWill remove",
  );
  await dialog.getByRole("button", { name: "Remove 1 skill" }).click();

  await expect(dialog.getByRole("status")).toHaveText("Removed 1 skill; 0 skipped; 0 failed.");
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(packRow(page, "anthropics/skills")).toHaveCount(0);
});

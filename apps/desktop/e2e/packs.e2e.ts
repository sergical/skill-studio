import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { openApp } from "./app-harness";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("navigation").getByRole("button", { name: "Packs" }).click();
  await expect(page.getByRole("heading", { name: "Packs", level: 1 })).toBeVisible();
});

const packRow = (page: Page, source: string) =>
  page.getByRole("main").getByRole("listitem", { name: source, exact: true });

test("updating a pack shows the plan first, and Confirm runs it and shows what each step did", async ({
  page,
}) => {
  await packRow(page, "getsentry/skills").getByRole("button", { name: "Update" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("heading", { name: "Update getsentry/skills?" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText(/^\d+ to update$/);
  const steps = dialog.getByRole("list", { name: "Steps" }).getByRole("listitem");
  // The skill is installed globally and in a project, so each step names its scope.
  await expect(steps.filter({ hasText: "sentry-triage" })).toHaveCount(2);
  await expect(steps.filter({ hasText: "sentry-triage" }).first()).toContainText("Global");
  const planned = await steps.count();

  await dialog.getByRole("button", { name: `Update ${planned} skills` }).click();

  await expect(dialog.getByRole("heading", { name: "Done" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText(`${planned} updated`);
  await expect(steps).toHaveCount(planned);
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
  await expect(dialog.getByRole("status")).toHaveText("2 skipped");
  await expect(dialog.getByRole("list", { name: "Steps" })).toContainText(
    "has local edits, which the owner tool would overwrite or delete",
  );
  await expect(dialog.getByRole("button", { name: "Update", exact: true })).toBeDisabled();
});

test("a plugin pack sits under Plugins with no Update or Remove, and offers the agent's own update command", async ({
  page,
}) => {
  const plugins = page.getByRole("region", { name: /^Plugins/ });
  await expect(plugins).toContainText("The agent updates and removes these");
  const row = plugins.getByRole("listitem", {
    name: "changelog-formatter@community-plugins",
    exact: true,
  });
  await expect(row.getByRole("button", { name: "Update", exact: true })).toHaveCount(0);
  await expect(row.getByRole("button", { name: "Remove", exact: true })).toHaveCount(0);

  // The copy button shows on keyboard focus, not only on hover.
  await page.getByRole("button", { name: "Add from a repo" }).focus();
  const copy = row.getByRole("button", {
    name: "Copy update command: claude plugin update changelog-formatter@community-plugins",
  });
  await copy.focus();
  await expect(copy).toHaveCSS("opacity", "1");
});

test("Check upstream offers the pack's new skills, and adding them installs them", async ({
  page,
}) => {
  const row = packRow(page, "getsentry/skills");
  await expect(row.getByRole("button", { name: /^Add \d+ new$/ })).toHaveCount(0);

  await page.getByRole("button", { name: "Check upstream" }).click();
  await expect(page.getByRole("main").getByRole("status")).toContainText("1 new upstream");
  await row.getByRole("button", { name: "Add 1 new" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("status")).toHaveText("1 to install");
  await expect(dialog.getByRole("list", { name: "Steps" })).toContainText("sentry-setup-logging");
  await dialog.getByRole("button", { name: "Add 1 skill" }).click();

  await expect(dialog.getByRole("status")).toHaveText("1 installed");
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(row.getByRole("button", { name: "Add 1 new" })).toHaveCount(0);
});

test("a pack's added skill reaches Claude Code, not only the shared folder", async ({ page }) => {
  const row = packRow(page, "getsentry/skills");
  await page.getByRole("button", { name: "Check upstream" }).click();
  await row.getByRole("button", { name: "Add 1 new" }).click();
  const dialog = page.getByRole("alertdialog");
  await dialog.getByRole("button", { name: "Add 1 skill" }).click();
  await expect(dialog.getByRole("status")).toHaveText("1 installed");
  await dialog.getByRole("button", { name: "Close" }).click();

  await page
    .getByRole("button", { name: /^Skills/ })
    .first()
    .click();
  await page.getByRole("textbox", { name: "Filter skills" }).fill("sentry-setup-logging");
  const skillRow = page
    .getByRole("grid", { name: "Skills" })
    .getByRole("row")
    .filter({ hasText: "sentry-setup-logging" });
  await expect(
    skillRow.getByRole("img", { name: "Claude Code · link to the Universal folder" }),
  ).toBeVisible();
});

test("removing a pack asks first, and Confirm removes its skills and drops the row", async ({
  page,
}) => {
  await packRow(page, "anthropics/skills").getByRole("button", { name: "Remove" }).click();

  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("heading", { name: "Remove anthropics/skills?" })).toBeVisible();
  await expect(dialog.getByRole("status")).toHaveText("1 to remove");
  await expect(dialog.getByRole("list", { name: "Steps" })).toHaveText("pdf-processing");
  await dialog.getByRole("button", { name: "Remove 1 skill" }).click();

  await expect(dialog.getByRole("status")).toHaveText("1 removed");
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(packRow(page, "anthropics/skills")).toHaveCount(0);
});

test("an app shortcut does not leave Packs while a pack dialog is open", async ({ page }) => {
  await packRow(page, "anthropics/skills").getByRole("button", { name: "Remove" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("status")).toHaveText("1 to remove");

  await page.keyboard.press("ControlOrMeta+Comma");

  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("heading", { name: "Packs", level: 1 })).toBeVisible();
});

test("Add from a repo opens the Add Skill sheet, where a skill installed here shows as Installed and is not added again", async ({
  page,
}) => {
  const skillsNav = page.getByRole("navigation").getByRole("button", { name: /^Skills \d+$/ });
  const countBefore = Number((await skillsNav.textContent())?.match(/\d+/)?.[0]);
  await page.getByRole("button", { name: "Add from a repo" }).click();

  const sheet = page.getByRole("dialog");
  await sheet.getByRole("textbox", { name: "Source" }).fill("getsentry/skills");
  await expect(sheet.getByText("3 skills · 1 installed")).toBeVisible();
  const installed = sheet.getByRole("listitem").filter({ hasText: "sentry-triage" });
  await expect(installed).toContainText("Installed");
  await expect(installed.getByRole("checkbox")).toBeDisabled();
  await expect(installed.getByRole("checkbox")).not.toBeChecked();

  await sheet.getByRole("button", { name: "Install 2 skills" }).click();

  await expect(sheet).toHaveCount(0);
  // Only the two new skills were added; `sentry-triage` was not sent again.
  await expect(skillsNav).toHaveText(new RegExp(`${countBefore + 2}$`));
});

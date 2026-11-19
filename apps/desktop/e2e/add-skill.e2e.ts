import { expect, test, type Locator, type Page } from "@playwright/test";
import { openApp, openSkillsPage, skillPageTitle, skillRow } from "./app-harness";

// The harness lists one skill, frontend-design, for any repository but getsentry/skills.
const REPO = "acme/agent-skills";
const SKILL = "frontend-design";

/** Opens Add skill and fills in a dotagents add from a repository the user has not trusted. */
async function startUntrustedAdd(page: Page): Promise<Locator> {
  await page.getByRole("navigation").getByRole("button", { name: "Add skill" }).click();
  const sheet = page.getByRole("dialog", { name: "Add skill" });
  await sheet.getByRole("textbox", { name: "Source" }).fill(REPO);
  await sheet
    .getByRole("group", { name: "Install method" })
    .getByRole("button", { name: "dotagents" })
    .click();
  await sheet.getByRole("button", { name: "Add skill", exact: true }).click();
  await expect(sheet.getByRole("button", { name: "Trust repository and retry" })).toBeVisible();
  await expect(sheet.getByRole("listitem").filter({ hasText: REPO })).toBeVisible();
  return sheet;
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

test("adding from an untrusted repository asks for trust first, and Trust repository and retry installs the skill", async ({
  page,
}) => {
  const sheet = await startUntrustedAdd(page);

  await sheet.getByRole("button", { name: "Trust repository and retry" }).click();

  await expect(sheet).toHaveCount(0);
  await expect(skillPageTitle(page, SKILL)).toBeVisible();
  await openSkillsPage(page, "Skills", SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(1);
});

test("Close on the trust prompt installs nothing", async ({ page }) => {
  const sheet = await startUntrustedAdd(page);

  // The footer's Close comes before the sheet's own close button.
  await sheet.getByRole("button", { name: "Close", exact: true }).first().click();

  await expect(sheet).toHaveCount(0);
  await openSkillsPage(page, "Skills", SKILL);
  await expect(page.getByRole("textbox", { name: "Filter skills" })).toHaveValue(SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(0);
});

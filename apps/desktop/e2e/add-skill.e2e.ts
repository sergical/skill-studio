import { expect, test, type Locator, type Page } from "@playwright/test";
import { openApp, openSkillsPage, skillPageTitle, skillRow } from "./app-harness";

// The harness lists one skill, frontend-design, for any repository but getsentry/skills.
const REPO = "acme/agent-skills";
const SKILL = "frontend-design";

/** Opens Add skill and fills in an add, by `method`, from a repository the user has not trusted. */
async function startUntrustedAdd(page: Page, method = "dotagents"): Promise<Locator> {
  await page.getByRole("navigation").getByRole("button", { name: "Add skill" }).click();
  const sheet = page.getByRole("dialog", { name: "Add skill" });
  await sheet.getByRole("textbox", { name: "Source" }).fill(REPO);
  await sheet
    .getByRole("group", { name: "Install method" })
    .getByRole("button", { name: method, exact: true })
    .click();
  await sheet.getByRole("button", { name: "Add skill", exact: true }).click();
  await expect(sheet.getByRole("button", { name: "Trust repository and retry" })).toBeVisible();
  await expect(sheet.getByRole("listitem").filter({ hasText: REPO })).toBeVisible();
  return sheet;
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

for (const method of ["dotagents", "skills.sh", "Copy"]) {
  test(`adding by ${method} from an untrusted repository asks for trust first, and Trust repository and retry installs the skill`, async ({
    page,
  }) => {
    const sheet = await startUntrustedAdd(page, method);

    await sheet.getByRole("button", { name: "Trust repository and retry" }).click();

    await expect(sheet).toHaveCount(0);
    await expect(skillPageTitle(page, SKILL)).toBeVisible();
    await openSkillsPage(page, "Skills", SKILL);
    await expect(skillRow(page, SKILL)).toHaveCount(1);
  });
}

test("Close on the trust prompt installs nothing", async ({ page }) => {
  const sheet = await startUntrustedAdd(page);

  // The footer's Close comes before the sheet's own close button.
  await sheet.getByRole("button", { name: "Close", exact: true }).first().click();

  await expect(sheet).toHaveCount(0);
  await openSkillsPage(page, "Skills", SKILL);
  await expect(page.getByRole("textbox", { name: "Filter skills" })).toHaveValue(SKILL);
  await expect(skillRow(page, SKILL)).toHaveCount(0);
});

test("pasting a SKILL.md adds a new skill and opens its page", async ({ page }) => {
  await page.getByRole("navigation").getByRole("button", { name: "Add skill" }).click();
  const sheet = page.getByRole("dialog", { name: "Add skill" });
  await sheet.getByRole("tab", { name: "Paste" }).click();

  await sheet
    .getByRole("textbox", { name: "SKILL.md" })
    .fill(
      "---\nname: pasted-review\ndescription: Reviews a diff before it ships.\n---\n\nRead the diff.\n",
    );
  await sheet.getByRole("button", { name: "Add skill", exact: true }).click();

  await expect(sheet).toHaveCount(0);
  await expect(skillPageTitle(page, "pasted-review")).toBeVisible();
  await openSkillsPage(page, "Skills", "pasted-review");
  await expect(skillRow(page, "pasted-review")).toHaveCount(1);
});

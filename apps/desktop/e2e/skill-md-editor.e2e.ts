import { expect, test, type Page } from "@playwright/test";
import { openApp, openSkillPage, openSkillsPage, toasts } from "./app-harness";

// A Claude Code copy that Skill Studio does not manage, so Save writes it without a fork first.
const SKILL = "internal-runbook";

/** The path of the skill's only SKILL.md, as the harness's snapshot lists it. */
function skillMdPath(page: Page): Promise<string> {
  return page.evaluate((name) => {
    const skill = window.__harness?.snapshot().skills.find((each) => each.name === name);
    if (skill?.deployments.length !== 1) throw new Error(`${name} must have one copy`);
    return `${skill.deployments[0].path}/SKILL.md`;
  }, SKILL);
}

function fileText(name: string, line: string): string {
  return `---\nname: ${name}\ndescription: Runbook steps.\n---\n\n# Runbook\n\n${line}\n`;
}

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await openSkillPage(page, SKILL);
});

test("Save writes the edited SKILL.md, and the page shows it after leaving and coming back", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await page
    .getByRole("textbox", { name: "SKILL.md" })
    .fill(fileText(SKILL, "Page the on-call engineer first."));
  await page.getByRole("button", { name: "Save", exact: true }).click();

  await expect(page.getByRole("textbox", { name: "SKILL.md" })).toHaveCount(0);
  await expect(page.getByText("Page the on-call engineer first.")).toBeVisible();

  await openSkillsPage(page);
  await openSkillPage(page, SKILL);
  await expect(page.getByText("Page the on-call engineer first.")).toBeVisible();
});

test("Save refuses when SKILL.md changed on disk after Edit, keeps the draft, and Cancel shows the file on disk", async ({
  page,
}) => {
  const path = await skillMdPath(page);
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  const editor = page.getByRole("textbox", { name: "SKILL.md" });
  await editor.fill(fileText(SKILL, "My draft line."));
  await page.evaluate(([file, text]) => window.__harness?.writeSkillMd(file, text), [
    path,
    fileText(SKILL, "Edited in another app."),
  ] as const);

  await page.getByRole("button", { name: "Save", exact: true }).click();

  await expect(toasts(page).getByText("SKILL.md changed on disk", { exact: true })).toBeVisible();
  await expect(editor).toHaveValue(/My draft line\./);

  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Discard", exact: true }).click();
  await expect(editor).toHaveCount(0);
  await expect(page.getByText("Edited in another app.")).toBeVisible();
  await expect(page.getByText("My draft line.")).toHaveCount(0);
});

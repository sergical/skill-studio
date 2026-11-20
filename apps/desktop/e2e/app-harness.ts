// ============================================================================
// app-harness - opens the full app on the dev harness and shapes its data.
// ============================================================================

import { expect } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";
import type { InstalledSkill } from "@skill-studio/lib";
import { deployment, HARNESS_HOME } from "../src/dev/harness/skill-fixture";

/** Loads the app, passes the first-run screen, and waits for Home. */
export async function openApp(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
}

type SkillList = "Skills" | "Parked";

/** Opens a skill list page (Skills or Parked) from the sidebar; its button also shows a count.
 * With `filter`, types it into Filter skills: the list draws only the rows in view, so a row
 * check without a filter can miss a row that is there. */
export async function openSkillsPage(
  page: Page,
  list: SkillList = "Skills",
  filter?: string,
): Promise<void> {
  await page
    .getByRole("navigation")
    .getByRole("button", { name: new RegExp(`^${list}`) })
    .click();
  await expect(page.getByRole("heading", { name: list, level: 1 })).toBeVisible();
  // The filter stays set between visits; a filter that matches nothing shows no grid at all.
  await page.getByRole("textbox", { name: "Filter skills" }).fill(filter ?? "");
}

/** The Skills page row for `name`: its name cell starts with the name, then any status icons. */
export function skillRow(page: Page, name: string): Locator {
  return page
    .getByRole("grid", { name: "Skills" })
    .getByRole("row")
    .filter({ has: nameCell(page, name) });
}

function nameCell(page: Page, name: string): Locator {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return page.getByRole("gridcell", { name: new RegExp(`^${escaped}( |$)`) });
}

/** Opens the skill page for `name` from a skill list page. */
export async function openSkillPage(
  page: Page,
  name: string,
  list: SkillList = "Skills",
): Promise<void> {
  await openSkillsPage(page, list, name);
  await page.getByRole("grid", { name: "Skills" }).locator(nameCell(page, name)).click();
  await expect(skillPageTitle(page, name)).toBeVisible();
}

/** The skill page's title. A SKILL.md that starts with `# <name>` shows a second such heading. */
export function skillPageTitle(page: Page, name: string): Locator {
  return page.getByRole("heading", { name, level: 1 }).first();
}

/** Opens a page from the sidebar by its exact name. */
export async function openSidebarPage(page: Page, name: string): Promise<void> {
  await page.getByRole("navigation").getByRole("button", { name, exact: true }).click();
  await expect(page.getByRole("heading", { name, level: 1 })).toBeVisible();
}

/** The titles of the native confirms the app showed, oldest first. */
export function askedTitles(page: Page): Promise<(string | null)[]> {
  return page.evaluate(() => window.__harness?.asked().map((dialog) => dialog.title) ?? []);
}

/** Answers the next native confirm with Cancel. */
export function cancelNextAsk(page: Page): Promise<void> {
  return page.evaluate(() => window.__harness?.answerNextAsk(false));
}

/** The toast list. */
export function toasts(page: Page): Locator {
  return page.getByRole("region", { name: /^Notifications/ });
}

/** Puts a live copy back where the parked `log-triage` came from, so Home lists it as left behind. */
export async function reinstallParkedSkill(page: Page): Promise<string> {
  const name = "log-triage";
  const live = deployment({
    agent: "shared",
    scope: "global",
    path: `${HARNESS_HOME}/.agents/skills/${name}`,
  });
  await page.evaluate(
    ([skillName, liveCopy]) =>
      window.__harness?.updateSkill(skillName, (skill: InstalledSkill) => ({
        ...skill,
        parked: false,
        deployments: [liveCopy, ...skill.deployments],
      })),
    [name, live] as const,
  );
  return name;
}

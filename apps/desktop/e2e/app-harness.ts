// ============================================================================
// app-harness - opens the full app on the dev harness and shapes its data.
// ============================================================================

import { expect } from "@playwright/test";
import type { Page } from "@playwright/test";
import type { InstalledSkill } from "@skill-studio/lib";
import { deployment, HARNESS_HOME } from "../src/dev/harness/skill-fixture";

/** Loads the app, passes the first-run screen, and waits for Home. */
export async function openApp(page: Page): Promise<void> {
  await page.goto("/");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
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

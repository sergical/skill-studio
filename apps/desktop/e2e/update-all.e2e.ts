import { expect, test } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";
import { scannerOwnerId, withScannerIdentity } from "../src/dev/harness/scanned-deployment";
import { HARNESS_PROJECT } from "../src/dev/harness/skill-fixture";

const UPDATES_COUNT = /^\d+ updates?$/;
/** A skill row, not a group header: only skill rows carry `aria-selected`. */
const SKILL_ROW = '[role="row"][aria-selected]';

function home(page: Page): Locator {
  return page.getByRole("region", { name: "Your skills at a glance" });
}

async function updatesOnHome(page: Page): Promise<number> {
  const count = home(page).getByRole("button", { name: UPDATES_COUNT });
  if ((await count.count()) === 0) return 0;
  return Number((await count.textContent())?.match(/\d+/)?.[0]);
}

/** Opens Skills from Home's updates count and returns that count. */
async function openUpdatesFromHome(page: Page): Promise<number> {
  await page.goto("/?persona=everything-wrong");
  await page.getByRole("button", { name: "Continue" }).click();
  const shown = await updatesOnHome(page);
  expect(shown).toBeGreaterThan(2);
  await home(page).getByRole("button", { name: UPDATES_COUNT }).click();
  await expect(page.getByRole("heading", { name: "Skills", level: 1 })).toBeVisible();
  return shown;
}

async function backHome(page: Page) {
  await page.getByRole("navigation").getByRole("button", { name: "Home", exact: true }).click();
  await expect(home(page)).toBeVisible();
}

const grid = (page: Page) => page.getByRole("grid", { name: "Skills" });
const rowUpdateButtons = (page: Page) =>
  grid(page).getByRole("row").getByRole("button", { name: "Update", exact: true });

test("Update all on Skills filtered to Updates updates every row, and Home then counts no updates", async ({
  page,
}) => {
  const shown = await openUpdatesFromHome(page);

  await grid(page).getByRole("button", { name: "Update all" }).click();

  await expect(page.getByText(`Updated ${shown} of ${shown} skills`)).toBeVisible();
  await expect(rowUpdateButtons(page)).toHaveCount(0);
  await expect(grid(page).getByText("Updated", { exact: true }).first()).toBeVisible();

  await backHome(page);
  await expect(home(page).getByRole("button", { name: UPDATES_COUNT })).toHaveCount(0);
});

// The real updater refreshes every dotagents skill of a scope with one `dotagents install`, and
// Stop lets the running command finish (crates/skill-studio-core/tests/update.rs,
// `update_all_finishes_the_running_call_...`). Here the Updates group holds the global dotagents
// skills, one command, and a dotagents skill installed only in a project, a second command.
test("Stop while Update all runs lets the running command finish and starts no other", async ({
  page,
}) => {
  test.slow();
  const shown = await openUpdatesFromHome(page);
  const { firstCommand, inProject, globalCopy } = await page.evaluate(() => {
    const dotagents = window
      .__harness!.snapshot()
      .skills.filter(
        (skill) => skill.update_owner_ids.length > 0 && skill.source_kind === "dotagents",
      )
      .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    // The last by name, so the global command runs first.
    const last = dotagents[dotagents.length - 1];
    return {
      firstCommand: dotagents.length - 1,
      inProject: last.name,
      globalCopy: last.deployments[0],
    };
  });
  // The project's own dotagents ledger owns the moved copy: its id and owner id are the ones the
  // scanner derives for a project install, so the desktop would run its command.
  const projectOwner = scannerOwnerId("project", HARNESS_PROJECT, inProject);
  const projectCopy = withScannerIdentity({
    ...globalCopy,
    scope: "project",
    project_path: HARNESS_PROJECT,
    path: `${HARNESS_PROJECT}/.agents/skills/${inProject}`,
    owner_id: projectOwner,
  });
  await page.evaluate(
    async ([name, copy, owner]) => {
      await window.__harness!.updateSkill(name, (skill) => ({
        ...skill,
        deployments: [copy],
        update_owner_ids: [owner],
        update_owners: skill.update_owners.map((update) => ({ ...update, owner_id: owner })),
      }));
    },
    [inProject, projectCopy, projectOwner] as const,
  );
  // The first command stays running until Stop has landed, however slow the machine.
  await page.evaluate(() => window.__harness!.holdUpdateCommands());

  await grid(page).getByRole("button", { name: "Update all" }).click();
  // Only the row being updated spins; the rest wait.
  await expect(grid(page).locator(SKILL_ROW).locator('button[aria-busy="true"]')).toHaveCount(1);
  await expect
    .poll(() => page.evaluate(() => window.__harness!.updateCommandsStarted()), {
      intervals: [20],
    })
    .toBe(1);
  await grid(page).getByRole("button", { name: "Stop" }).click();
  await page.evaluate(() => window.__harness!.releaseUpdateCommands());

  await expect(
    page.getByText(`Stopped. Updated ${firstCommand} of ${shown} skills.`, { exact: true }),
  ).toBeVisible();
  // Nothing was refused: the project copy waited for its command, and Stop kept it from starting.
  // Counted once, while the toast is still up: a retrying check would pass once it closes.
  expect(await page.getByText(/\d+ failed/).count()).toBe(0);
  expect(await page.evaluate(() => window.__harness!.updateCommandsStarted())).toBe(1);
  await expect(grid(page).getByRole("button", { name: "Update all" })).toBeVisible();
  await expect(
    grid(page)
      .locator(SKILL_ROW)
      .filter({ has: page.getByRole("gridcell", { name: inProject, exact: true }) })
      .getByRole("button", { name: "Update", exact: true }),
  ).toBeVisible();

  await backHome(page);
  await expect.poll(() => updatesOnHome(page)).toBe(shown - firstCommand);
});

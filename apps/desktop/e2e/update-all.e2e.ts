import { expect, test } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";
import { setLatency } from "./app-harness";

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
// skills, one command, and a skill of another kind, a second command.
test("Stop while Update all runs lets the running command finish and starts no other", async ({
  page,
}) => {
  test.slow();
  const shown = await openUpdatesFromHome(page);
  const { firstCommand, other } = await page.evaluate(() => {
    const outdated = window
      .__harness!.snapshot()
      .skills.filter(
        (skill) => skill.update_owner_ids.length > 0 && skill.source_kind !== "plugin",
      );
    const dotagents = outdated.filter((skill) => skill.source_kind === "dotagents");
    return {
      firstCommand: dotagents.length,
      other: outdated.find((skill) => skill.source_kind !== "dotagents")?.name,
    };
  });
  if (!other) throw new Error("fixture has no second command");
  // Each command then takes 400 ms, so the first is still running when Stop lands.
  await setLatency(page, 400);

  await grid(page).getByRole("button", { name: "Update all" }).click();
  // Only the row being updated spins; the rest wait.
  await expect(grid(page).locator(SKILL_ROW).locator('button[aria-busy="true"]')).toHaveCount(1);
  await expect
    .poll(() => page.evaluate(() => window.__harness!.updateCommandsStarted()), {
      intervals: [20],
    })
    .toBe(1);
  await grid(page).getByRole("button", { name: "Stop" }).click();

  await expect(
    page.getByText(`Stopped. Updated ${firstCommand} of ${shown} skills.`, { exact: true }),
  ).toBeVisible();
  await expect(grid(page).getByRole("button", { name: "Update all" })).toBeVisible();
  await expect(
    grid(page)
      .locator(SKILL_ROW)
      .filter({ has: page.getByRole("gridcell", { name: other, exact: true }) })
      .getByRole("button", { name: "Update", exact: true }),
  ).toBeVisible();

  await backHome(page);
  await expect.poll(() => updatesOnHome(page)).toBe(shown - firstCommand);
});

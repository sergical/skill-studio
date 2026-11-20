import { expect, test, type Locator, type Page } from "@playwright/test";
import { openApp, setLatency } from "./app-harness";

/** A skill row, not a group header: only skill rows carry `aria-selected`. */
const SKILL_ROW = '[role="row"][aria-selected]';

// After the app starts, every harness answer waits 400 ms, so a running act is on screen long
// enough to measure; the waits add up, so each test gets the slow timeout.
test.beforeEach(async ({ page }) => {
  test.slow();
  await openApp(page);
  await setLatency(page, 400);
});

async function openSkillsPage(page: Page) {
  await page
    .getByRole("button", { name: /^Skills/ })
    .first()
    .click();
  await expect(page.getByRole("grid", { name: "Skills" })).toBeVisible();
}

function rowNamed(grid: Locator, name: string): Locator {
  return grid.locator(SKILL_ROW).filter({
    has: grid.page().locator('[role="gridcell"]:nth-child(2) span.truncate', { hasText: name }),
  });
}

async function nameOf(row: Locator): Promise<string> {
  return (await row.locator('[role="gridcell"]:nth-child(2) span.truncate').textContent())!;
}

async function top(row: Locator): Promise<number> {
  return Math.round((await row.boundingBox())!.y);
}

interface BusyBox {
  width: number;
  top: number;
}

/** Records the button's width and its row's top in the page, in the same task that sets
 * `aria-busy="true"` on the button. Two separate reads after the act starts can miss it: on a
 * loaded runner the act can end between them. Call it before the click; await the result after. */
async function measureWhenBusy(button: Locator, row: Locator): Promise<() => Promise<BusyBox>> {
  const rowElement = await row.elementHandle();
  const pending = await button.evaluateHandle(
    // Wrapped in an object: evaluateHandle would wait for a returned promise, before the click.
    (element, rowEl) => ({
      box: new Promise<BusyBox>((resolve) => {
        const observer = new MutationObserver(() => {
          if (element.getAttribute("aria-busy") !== "true") return;
          observer.disconnect();
          resolve({
            width: element.getBoundingClientRect().width,
            top: Math.round(rowEl!.getBoundingClientRect().y),
          });
        });
        observer.observe(element, { attributes: true, attributeFilter: ["aria-busy"] });
      }),
    }),
    rowElement,
  );
  // Called right after the click: the button must turn busy within expect's default 5 s, as the
  // `toHaveAttribute("aria-busy", "true")` check that this replaces required.
  return () =>
    pending.evaluate(
      (handle, ms) =>
        Promise.race([
          handle.box,
          new Promise<BusyBox>((_, reject) => {
            setTimeout(() => reject(new Error(`the button did not turn busy within ${ms} ms`)), ms);
          }),
        ]),
      5000,
    );
}

test("Update on a Skills row spins in place, then the row stays in its slot and says Updated", async ({
  page,
}) => {
  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  const row = grid
    .locator(SKILL_ROW)
    .filter({ has: page.getByRole("button", { name: "Update", exact: true }) })
    .first();
  const target = rowNamed(grid, await nameOf(row));
  const running = target.getByRole("button", { name: "Update", exact: true });
  const before = await top(target);
  const width = (await running.boundingBox())!.width;
  const busy = await measureWhenBusy(running, target);

  await running.click();

  expect(await busy()).toEqual({ width, top: before });

  await expect(target.getByText("Updated", { exact: true })).toBeVisible();
  expect(await top(target)).toBe(before);
});

test("Park from the selection bar keeps the row in its slot as Parked, and Undo unparks it", async ({
  page,
}) => {
  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  const live = grid.locator(`${SKILL_ROW}:not(.text-text-tertiary)`).first();
  const name = await nameOf(live);
  const target = rowNamed(grid, name);
  const before = await top(target);

  await target.focus();
  await page.keyboard.press("x");
  await page.getByRole("button", { name: "Park", exact: true }).click();

  await expect(target.getByText("Parked", { exact: true })).toBeVisible();
  expect(await top(target)).toBe(before);

  await page.getByRole("button", { name: "Undo" }).click();
  await expect(target.getByText("Parked", { exact: true })).toHaveCount(0);
  await expect(target).not.toHaveClass(/text-text-tertiary/);
});

test("a row with no fix of its own spins in its fix cell while a selection-bar Park runs on it", async ({
  page,
}) => {
  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  const quiet = grid
    .locator(`${SKILL_ROW}:not(.text-text-tertiary)`)
    .filter({ hasNot: page.locator('[role="gridcell"]:last-child button') })
    .first();
  const target = rowNamed(grid, await nameOf(quiet));

  await target.focus();
  await page.keyboard.press("x");
  await page.getByRole("button", { name: "Park", exact: true }).click();

  await expect(target.locator('[role="gridcell"]:last-child [aria-busy="true"]')).toBeVisible();
  await expect(target.getByText("Parked", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Undo" }).click();
  await expect(target.getByText("Parked", { exact: true })).toHaveCount(0);
  await expect(target).not.toHaveClass(/text-text-tertiary/);
});

test("Update all on Home keeps every Updates row in its slot and marks each one Updated", async ({
  page,
}) => {
  const group = page.locator('[data-group="upd"]');
  const rows = group.locator(SKILL_ROW);
  await expect(rows.first()).toBeVisible();
  const names = await rows.evaluateAll((els) =>
    els.map((el) => el.querySelector('[role="gridcell"]:nth-child(2) span.truncate')!.textContent),
  );
  const tops = await rows.evaluateAll((els) =>
    els.map((el) => Math.round(el.getBoundingClientRect().top)),
  );

  await group.getByRole("button", { name: "Update all" }).click();

  await expect(group.getByText("Updated", { exact: true })).toHaveCount(names.length);
  expect(
    await rows.evaluateAll((els) =>
      els.map(
        (el) => el.querySelector('[role="gridcell"]:nth-child(2) span.truncate')!.textContent,
      ),
    ),
  ).toEqual(names);
  expect(
    await rows.evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().top))),
  ).toEqual(tops);
});

test("a selection-bar Park leaves a row that is still updating to its Update, and parks the rest", async ({
  page,
}) => {
  // The Update must still run when Park is clicked; a slow answer keeps that true on a loaded machine.
  await setLatency(page, 2000);
  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  const updating = rowNamed(
    grid,
    await nameOf(
      grid
        .locator(SKILL_ROW)
        .filter({ has: page.getByRole("button", { name: "Update", exact: true }) })
        .first(),
    ),
  );
  const updatingName = await nameOf(updating);
  const other = rowNamed(
    grid,
    await nameOf(
      grid
        .locator(`${SKILL_ROW}:not(.text-text-tertiary)`)
        .filter({ hasNotText: updatingName })
        .first(),
    ),
  );

  await updating.focus();
  await page.keyboard.press("x");
  await other.focus();
  await page.keyboard.press("x");
  await updating.getByRole("button", { name: "Update", exact: true }).click();
  await page.getByRole("button", { name: "Park 2" }).click();

  await expect(other.getByText("Parked", { exact: true })).toBeVisible();
  await expect(page.getByText(/1 still running/)).toBeVisible();
  await expect(updating.getByText("Updated", { exact: true })).toBeVisible();
  await expect(updating).not.toHaveClass(/text-text-tertiary/);
});

test("a row an action held leaves the list when a new search leaves it out", async ({ page }) => {
  await openSkillsPage(page);
  const grid = page.getByRole("grid", { name: "Skills" });
  const target = rowNamed(
    grid,
    await nameOf(
      grid
        .locator(SKILL_ROW)
        .filter({ has: page.getByRole("button", { name: "Update", exact: true }) })
        .first(),
    ),
  );
  const targetName = await nameOf(target);
  const otherName = await nameOf(
    grid.locator(SKILL_ROW).filter({ hasNotText: targetName }).first(),
  );

  await target.getByRole("button", { name: "Update", exact: true }).click();
  await expect(target.getByText("Updated", { exact: true })).toBeVisible();

  await page.getByRole("textbox", { name: "Filter skills" }).fill(otherName);
  await expect(rowNamed(grid, otherName)).toBeVisible();
  await expect(target).toHaveCount(0);
});

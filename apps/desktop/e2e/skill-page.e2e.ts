import { expect, test, type Locator, type Page } from "@playwright/test";

// Every harness answer waits 400 ms, so SKILL.md is still loading when the page first paints.
test.beforeEach(async ({ page }) => {
  test.slow();
  await page.goto("/?latency=400");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
});

async function openFirstSkill(page: Page) {
  await page
    .getByRole("button", { name: /^Skills/ })
    .first()
    .click();
  const grid = page.getByRole("grid", { name: "Skills" });
  await grid.locator('[role="row"][aria-selected]').first().click();
}

/** The SKILL.md card's own header row: the title, the copy label or picker, and Edit. */
function markdownHeader(page: Page): Locator {
  return page.getByText("SKILL.md", { exact: true }).first().locator("..");
}

/** Where the element sits in the page's content, so a scroll before a click does not count as a move. */
async function box(locator: Locator) {
  return locator.evaluate((el) => {
    const rect = el.getBoundingClientRect();
    let scrolled = 0;
    for (let node = el.parentElement; node; node = node.parentElement) scrolled += node.scrollTop;
    return {
      x: Math.round(rect.x),
      y: Math.round(rect.y + scrolled),
      height: Math.round(rect.height),
    };
  });
}

test("the SKILL.md header keeps its size and Edit keeps its place while the file loads and while it is edited", async ({
  page,
}) => {
  await openFirstSkill(page);
  const header = markdownHeader(page);
  const edit = header.getByRole("button", { name: "Edit" });
  await expect(edit).toBeDisabled();
  const loading = await box(header);
  const editWhileLoading = await box(edit);

  await expect(edit).toBeEnabled();
  expect(await box(header)).toEqual(loading);
  expect(await box(edit)).toEqual(editWhileLoading);

  await edit.click();
  await expect(page.getByRole("textbox", { name: "SKILL.md" })).toBeVisible();
  expect(await box(header)).toEqual(loading);
  await expect(header.getByRole("button", { name: "Edit" })).toHaveCount(0);
});

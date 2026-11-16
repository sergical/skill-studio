import { expect, test, type Locator } from "@playwright/test";
import { openApp, reinstallParkedSkill } from "./app-harness";

const AA_BODY_TEXT = 4.5;

/** WCAG contrast of the element's text against its painted background, alpha layers included. */
function textContrast(element: Locator): Promise<number> {
  return element.evaluate((el) => {
    const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    if (!ctx) throw new Error("no 2d canvas");
    const pixel = () => Array.from(ctx.getImageData(0, 0, 1, 1).data);
    const paint = (color: string) => {
      ctx.fillStyle = color;
      ctx.fillRect(0, 0, 1, 1);
    };
    const isOpaque = (color: string) => {
      ctx.clearRect(0, 0, 1, 1);
      paint(color);
      return pixel()[3] === 255;
    };

    const layers: string[] = [];
    for (let node: Element | null = el; node; node = node.parentElement) {
      const color = getComputedStyle(node).backgroundColor;
      layers.unshift(color);
      if (isOpaque(color)) break;
    }
    ctx.clearRect(0, 0, 1, 1);
    layers.forEach(paint);
    const background = pixel();
    paint(getComputedStyle(el).color);
    const text = pixel();

    const luminance = ([r, g, b]: number[]) => {
      const [lr, lg, lb] = [r, g, b].map((channel) => {
        const c = channel / 255;
        return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
    };
    const [dark, light] = [luminance(background), luminance(text)].sort((a, b) => a - b);
    return (light + 0.05) / (dark + 0.05);
  });
}

/** Waits for color transitions to end, so the measure reads the final state. */
async function settled(element: Locator): Promise<void> {
  await element.evaluate((el) =>
    Promise.all(el.getAnimations({ subtree: true }).map((animation) => animation.finished)),
  );
}

for (const theme of ["dark", "light"] as const) {
  test(`the destructive Keep live button text reaches ${AA_BODY_TEXT}:1 at rest and on hover in ${theme} mode`, async ({
    page,
  }) => {
    await openApp(page);
    await page.evaluate((value) => {
      document.documentElement.dataset.theme = value;
    }, theme);
    const name = await reinstallParkedSkill(page);
    await page
      .getByRole("row")
      .filter({ hasText: name })
      .getByRole("button", { name: "Resolve" })
      .click();
    await page.getByRole("menuitem", { name: "Keep live" }).click();

    const keepLive = page
      .getByRole("alertdialog", { name: `Keep the live copy of ${name}?` })
      .getByRole("button", { name: "Keep live" });
    await expect(keepLive).toBeVisible();
    await settled(keepLive);
    expect(await textContrast(keepLive)).toBeGreaterThanOrEqual(AA_BODY_TEXT);
    await keepLive.hover();
    await settled(keepLive);
    expect(await textContrast(keepLive)).toBeGreaterThanOrEqual(AA_BODY_TEXT);
  });
}

for (const theme of ["dark", "light"] as const) {
  test(`a destructive menu item reaches ${AA_BODY_TEXT}:1 under the pointer and under keyboard focus in ${theme} mode`, async ({
    page,
  }) => {
    await openApp(page);
    await page.evaluate((value) => {
      document.documentElement.dataset.theme = value;
    }, theme);
    await page
      .getByRole("button", { name: /^Skills/ })
      .first()
      .click();
    await page
      .getByRole("grid", { name: "Skills" })
      .locator('[role="row"][aria-selected]')
      .first()
      .click();
    await page
      .getByRole("button", { name: /^More actions for / })
      .first()
      .click();

    const item = page.locator('[role="menuitem"][data-variant="destructive"]').first();
    await item.hover();
    await settled(item);
    expect(await textContrast(item)).toBeGreaterThanOrEqual(AA_BODY_TEXT);
    await page.mouse.move(0, 0);
    await item.focus();
    await settled(item);
    expect(await textContrast(item)).toBeGreaterThanOrEqual(AA_BODY_TEXT);
  });
}

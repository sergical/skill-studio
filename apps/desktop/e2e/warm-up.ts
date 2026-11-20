// ============================================================================
// warm-up - loads the app once before any test runs, so Vite has transformed
// every module first. On a cold server under load, WebKit's first page took
// over 30 s, and the first test in each WebKit worker timed out in page.goto.
// ============================================================================

import { chromium, type FullConfig } from "@playwright/test";

export default async function warmUp(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0]?.use.baseURL;
  if (!baseURL) throw new Error("warm-up: the Playwright config sets no baseURL");
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    await page.goto(baseURL, { timeout: 180_000 });
    await page.getByRole("button", { name: "Continue" }).waitFor({ timeout: 180_000 });
  } finally {
    await browser.close();
  }
}

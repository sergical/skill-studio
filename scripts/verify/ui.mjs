import { chromium } from "@playwright/test";
import assert from "node:assert/strict";

const [url, out] = process.argv.slice(2);
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage({
    ignoreHTTPSErrors: true,
    viewport: { width: 1280, height: 900 },
  });
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.getByRole("heading", { name: "Home", exact: true }).waitFor();
  process.stdout.write("Question: Can a GitHub source offer the unavailable Pack method?");
  await page.getByRole("button", { name: "Add skill", exact: true }).first().click();
  await page.getByPlaceholder("owner/repo, a URL, or a local path").fill("fixture/skills");
  const methods = page.locator('[aria-label="Install method"]');
  await methods.waitFor();
  assert.equal(await methods.getByText(/pack/i).count(), 0);
  await page
    .getByRole("dialog", { name: "Add skill" })
    .screenshot({ path: `${out}/add-methods.png` });
  await page.keyboard.press("Escape");
  process.stdout.write(
    "Question: Does Settings omit the misleading Doctor and Command health cards?",
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("radiogroup", { name: "Open in editor" }).waitFor();
  assert.equal(await page.getByRole("heading", { name: /^(Doctor|Command health)$/ }).count(), 0);
  await page.screenshot({ path: `${out}/settings.png`, fullPage: true });
  assert.deepEqual(errors, []);
  process.stdout.write("Observed no Pack method, no obsolete health cards, and no page errors.");
} finally {
  await browser.close();
}

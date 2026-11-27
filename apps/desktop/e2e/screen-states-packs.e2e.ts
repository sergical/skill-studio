import { mkdirSync, writeFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

// Renders the Packs screen's states that need clicks (an upstream check, each plan dialog, the
// export dialog) at the smallest and the default window, and writes a contact sheet. Runs only
// when SCREEN_STATES names an output folder; see .agents/skills/verify-screen-states/SKILL.md.
const out = process.env.SCREEN_STATES;
const SIZES = [
  { name: "min", width: 900, height: 600 },
  { name: "default", width: 1200, height: 800 },
];
// "" is the plain harness, which has a pack with local edits and one with a new upstream skill.
const PERSONAS = ["", "plugin-heavy", "six-agents"];

async function openPacks(page: Page, persona: string) {
  await page.goto(persona ? `/?persona=${persona}` : "/");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home", exact: true })).toBeVisible();
  await page.getByRole("navigation").getByRole("button", { name: "Packs" }).click();
  await expect(page.getByRole("heading", { name: "Packs", level: 1 })).toBeVisible();
}

const row = (page: Page, source: string) =>
  page.getByRole("listitem", { name: source, exact: true });

interface PacksState {
  personas: string[];
  run: (page: Page) => Promise<void>;
}

const STATES = {
  checked: {
    personas: PERSONAS,
    run: async (page) => {
      await page.getByRole("button", { name: "Check upstream" }).click();
      await page.waitForTimeout(300);
    },
  },
  "update-one": {
    personas: [""],
    run: async (page) => {
      await row(page, "getsentry/skills").getByRole("button", { name: "Update" }).click();
      await expect(page.getByRole("alertdialog").getByRole("list")).toBeVisible();
    },
  },
  "update-skipped": {
    personas: [""],
    run: async (page) => {
      await row(page, "openapi-generator/openapi-generator")
        .getByRole("button", { name: "Update" })
        .click();
      await expect(page.getByRole("alertdialog").getByRole("list")).toBeVisible();
    },
  },
  "update-all": {
    personas: ["", "plugin-heavy"],
    run: async (page) => {
      await page.getByRole("button", { name: "Update all" }).click();
      await expect(page.getByRole("alertdialog").getByRole("list")).toBeVisible();
    },
  },
  "update-done": {
    personas: [""],
    run: async (page) => {
      await row(page, "getsentry/skills").getByRole("button", { name: "Update" }).click();
      const dialog = page.getByRole("alertdialog");
      await dialog.getByRole("button", { name: /^Update \d+ skills?$/ }).click();
      await expect(dialog.getByRole("button", { name: "Close" })).toBeVisible();
    },
  },
  remove: {
    personas: [""],
    run: async (page) => {
      await row(page, "anthropics/skills").getByRole("button", { name: "Remove" }).click();
      await expect(page.getByRole("alertdialog").getByRole("list")).toBeVisible();
    },
  },
  "add-new": {
    personas: [""],
    run: async (page) => {
      await page.getByRole("button", { name: "Check upstream" }).click();
      await row(page, "getsentry/skills").getByRole("button", { name: "Add 1 new" }).click();
      await expect(page.getByRole("alertdialog").getByRole("list")).toBeVisible();
    },
  },
  export: {
    personas: [""],
    run: async (page) => {
      await exportTwo(page);
    },
  },
  "export-done": {
    personas: [""],
    run: async (page) => {
      const dialog = await exportTwo(page);
      await dialog.getByRole("textbox").fill("me/my-skills");
      await dialog.getByRole("button", { name: "Export" }).click();
      await expect(dialog.getByRole("button", { name: "Close" })).toBeVisible();
    },
  },
} satisfies Record<string, PacksState>;

async function exportTwo(page: Page) {
  await page
    .getByRole("navigation")
    .getByRole("button", { name: /^Skills/ })
    .click();
  const rows = page.getByRole("grid", { name: "Skills" }).locator('[role="row"][aria-selected]');
  for (const index of [0, 1]) {
    await rows.nth(index).hover();
    await rows.nth(index).getByRole("checkbox").click();
  }
  await page.getByRole("button", { name: "Pack", exact: true }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog.getByRole("list")).toBeVisible();
  return dialog;
}

test.describe.configure({ mode: "parallel" });

for (const [state, spec] of Object.entries(STATES))
  for (const persona of spec.personas)
    for (const size of SIZES)
      test(`${state} ${persona || "harness"} at ${size.name}`, async ({ page }, info) => {
        test.skip(!out || info.project.name !== "webkit", "set SCREEN_STATES; WebKit only");
        const errors: string[] = [];
        page.on("pageerror", (error) => errors.push(error.message));
        await page.emulateMedia({ colorScheme: "dark" });
        await page.setViewportSize({ width: size.width, height: size.height });
        await openPacks(page, persona);
        await spec.run(page);
        await page.waitForTimeout(400);
        mkdirSync(`${out}/shots`, { recursive: true });
        const name = `${state}-${persona || "harness"}-${size.name}`;
        await page.screenshot({ path: `${out}/shots/${name}.png` });
        writeFileSync(
          `${out}/shots/${name}.json`,
          JSON.stringify({ state, persona, size, errors }),
        );
        expect(errors).toEqual([]);
      });

test.afterAll(() => {
  if (!out) return;
  const rows = Object.entries(STATES)
    .flatMap(([state, spec]) =>
      spec.personas.map(
        (persona) =>
          `<section><h2>${state} <small>${persona || "harness"}</small></h2><div>${SIZES.map(
            (size) =>
              `<figure><img src="shots/${state}-${persona || "harness"}-${size.name}.png" loading="lazy"><figcaption>${size.name} ${size.width}×${size.height}</figcaption></figure>`,
          ).join("")}</div></section>`,
      ),
    )
    .join("");
  writeFileSync(
    `${out}/index.html`,
    `<!doctype html><meta charset="utf-8"><title>Packs states</title><style>body{background:#111;color:#ddd;font:13px -apple-system,sans-serif;margin:24px}h2{font-size:15px;margin:28px 0 8px}small{color:#888;font-weight:400}div{display:flex;gap:12px;align-items:flex-start}figure{margin:0}img{display:block;width:520px;border:1px solid #333;border-radius:6px}figcaption{color:#888;margin-top:4px}</style><h1>Packs states</h1>${rows}`,
  );
});

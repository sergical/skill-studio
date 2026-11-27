import { mkdirSync, writeFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { PERSONAS } from "../src/dev/harness/personas";

// Renders one screen for every persona at every window size and writes a contact sheet, so a
// person or a reviewer can look at all of them side by side. Runs only when SCREEN_STATES names
// an output folder; see .agents/skills/verify-screen-states/SKILL.md.
const out = process.env.SCREEN_STATES;
const query = process.env.SCREEN_STATES_QUERY ?? "v=1";
const SIZES = [
  { name: "min", width: 900, height: 600 },
  { name: "default", width: 1200, height: 800 },
  { name: "wide", width: 1600, height: 1000 },
];

test.describe.configure({ mode: "parallel" });

for (const [id, persona] of Object.entries(PERSONAS))
  for (const size of SIZES)
    test(`${id} at ${size.name}`, async ({ page }, info) => {
      test.skip(!out || info.project.name !== "webkit", "set SCREEN_STATES; WebKit only");
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.emulateMedia({ colorScheme: "dark" });
      await page.setViewportSize({ width: size.width, height: size.height });
      await page.goto(`/?persona=${id}&${query}`);
      await page.getByRole("button", { name: "Continue" }).click();
      await expect(page.getByRole("heading", { name: "Home", exact: true })).toBeVisible();
      await page.waitForTimeout(500);
      mkdirSync(`${out}/shots`, { recursive: true });
      await page.screenshot({ path: `${out}/shots/${id}-${size.name}.png` });
      writeFileSync(
        `${out}/shots/${id}-${size.name}.json`,
        JSON.stringify({ id, label: persona.label, agents: persona.agents, size, errors }),
      );
      expect(errors).toEqual([]);
    });

test.afterAll(() => {
  if (!out) return;
  const rows = Object.entries(PERSONAS)
    .map(
      ([id, persona]) =>
        `<section><h2>${id} <small>${persona.label} · agents: ${persona.agents.join(", ")}</small></h2><div>${SIZES.map(
          (size) =>
            `<figure><img src="shots/${id}-${size.name}.png" loading="lazy"><figcaption>${size.name} ${size.width}×${size.height}</figcaption></figure>`,
        ).join("")}</div></section>`,
    )
    .join("");
  writeFileSync(
    `${out}/index.html`,
    `<!doctype html><meta charset="utf-8"><title>Screen states</title><style>body{background:#111;color:#ddd;font:13px -apple-system,sans-serif;margin:24px}h2{font-size:15px;margin:28px 0 8px}small{color:#888;font-weight:400}div{display:flex;gap:12px;align-items:flex-start}figure{margin:0}img{display:block;width:420px;border:1px solid #333;border-radius:6px}figcaption{color:#888;margin-top:4px}</style><h1>${query}</h1>${rows}`,
  );
});

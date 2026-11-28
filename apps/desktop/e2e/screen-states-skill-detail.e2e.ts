import { mkdirSync, writeFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { PERSONAS } from "../src/dev/harness/personas";

// Renders the skill page for every persona (its first Skills row) at every window size, and the
// default estate's skills that carry each state (broken, warning, update, parked, plugin,
// project, drift, long text) at the default size and at a tall size that shows the whole page.
// Runs only when SCREEN_STATES names an output folder; see
// .agents/skills/verify-screen-states/SKILL.md.
const out = process.env.SCREEN_STATES;
const SIZES = [
  { name: "min", width: 900, height: 600 },
  { name: "default", width: 1200, height: 800 },
  { name: "wide", width: 1600, height: 1000 },
];
const CASE_SIZES = [
  { name: "default", width: 1200, height: 800 },
  { name: "tall", width: 1200, height: 2200 },
];
const CASES: { name: string; list?: "Parked" | "Plugins"; why: string }[] = [
  { name: "commit", why: "global and project, three agents" },
  { name: "release-notes", why: "broken link" },
  { name: "readme-audit", why: "broken frontmatter" },
  { name: "commit-message-lint", why: "spec warning" },
  { name: "docker-multistage", why: "update" },
  { name: "log-triage", list: "Parked", why: "parked" },
  { name: "git-worktree-helper", list: "Plugins", why: "plugin" },
  { name: "sdk-migration", why: "project only" },
  { name: "api-client-codegen", why: "copies differ" },
  { name: "incident-postmortem", why: "long description" },
  { name: "worktree-cleanup", why: "no description" },
  { name: "dependency-upgrade-plan", why: "six agents" },
  { name: "sql-explain", why: "off for one agent" },
];

test.describe.configure({ mode: "parallel" });

async function start(page: Page, query: string, size: { width: number; height: number }) {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.setViewportSize({ width: size.width, height: size.height });
  await page.goto(`/?${query}`);
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Home", exact: true })).toBeVisible();
}

interface ShotMeta {
  id: string;
  label: string;
  size: { name: string; width: number; height: number };
}

async function shoot(page: Page, file: string, meta: ShotMeta, errors: string[]) {
  await page.addStyleTag({ content: ".proto-picker { display: none !important; }" });
  await page.waitForTimeout(700);
  mkdirSync(`${out}/shots`, { recursive: true });
  await page.screenshot({ path: `${out}/shots/${file}.png` });
  writeFileSync(`${out}/shots/${file}.json`, JSON.stringify({ ...meta, errors }));
  expect(errors).toEqual([]);
}

for (const [id, persona] of Object.entries(PERSONAS))
  for (const size of SIZES)
    test(`${id} at ${size.name}`, async ({ page }, info) => {
      test.skip(!out || info.project.name !== "webkit", "set SCREEN_STATES; WebKit only");
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await start(page, `persona=${id}`, size);
      await page
        .getByRole("navigation")
        .getByRole("button", { name: /^Skills/ })
        .click();
      const rows = page
        .getByRole("grid", { name: "Skills" })
        .locator('[role="row"][aria-selected]');
      if ((await rows.count()) > 0) await rows.first().click();
      await shoot(page, `${id}-${size.name}`, { id, label: persona.label, size }, errors);
    });

for (const c of CASES)
  for (const size of CASE_SIZES)
    test(`case ${c.name} at ${size.name}`, async ({ page }, info) => {
      test.skip(!out || info.project.name !== "webkit", "set SCREEN_STATES; WebKit only");
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await start(page, "", size);
      const list = c.list ?? "Skills";
      await page
        .getByRole("navigation")
        .getByRole("button", { name: new RegExp(`^${list}`) })
        .click();
      if (list === "Plugins")
        await page
          .getByRole("main")
          .getByRole("button", { name: new RegExp(c.name) })
          .first()
          .click();
      else {
        await page.getByRole("textbox", { name: "Filter skills" }).fill(c.name);
        await page
          .getByRole("grid", { name: "Skills" })
          .getByRole("gridcell", { name: new RegExp(`^${c.name}( |$)`) })
          .first()
          .click();
      }
      await expect(
        page.getByRole("heading", { name: c.name, level: 1, exact: true }).first(),
      ).toBeVisible();
      await shoot(page, `case-${c.name}-${size.name}`, { id: c.name, label: c.why, size }, errors);
    });

test.afterAll(() => {
  if (!out) return;
  const figure = (file: string, caption: string, width = 420) =>
    `<figure><img src="shots/${file}.png" loading="lazy" style="width:${width}px"><figcaption>${caption}</figcaption></figure>`;
  const cases = CASES.map(
    (c) =>
      `<section><h2>${c.name} <small>${c.why}</small></h2><div>${CASE_SIZES.map((size) =>
        figure(`case-${c.name}-${size.name}`, `${size.name} ${size.width}×${size.height}`),
      ).join("")}</div></section>`,
  ).join("");
  const personas = Object.entries(PERSONAS)
    .map(
      ([id, persona]) =>
        `<section><h2>${id} <small>${persona.label} · agents: ${persona.agents.join(", ")}</small></h2><div>${SIZES.map(
          (size) => figure(`${id}-${size.name}`, `${size.name} ${size.width}×${size.height}`),
        ).join("")}</div></section>`,
    )
    .join("");
  writeFileSync(
    `${out}/index-skill-detail.html`,
    `<!doctype html><meta charset="utf-8"><title>Skill page states</title><style>body{background:#111;color:#ddd;font:13px -apple-system,sans-serif;margin:24px}h2{font-size:15px;margin:28px 0 8px}small{color:#888;font-weight:400}div{display:flex;gap:12px;align-items:flex-start}figure{margin:0}img{display:block;border:1px solid #333;border-radius:6px}figcaption{color:#888;margin-top:4px}</style><h1>Skill page: states</h1>${cases}<h1>Skill page: each persona's first skill</h1>${personas}`,
  );
});

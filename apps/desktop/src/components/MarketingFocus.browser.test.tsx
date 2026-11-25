import { afterAll, afterEach, beforeAll, expect, it } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "vitest-browser-react";
import * as stylex from "@stylexjs/stylex";
import { lightSiteTheme } from "../../../../packages/marketing/src/SiteTheme.stylex";
import { FaqSection } from "../../../../packages/marketing/src/home/FaqSection";
import { HeroSurfaceShowcase } from "../../../../packages/marketing/src/home/listing-calc/HeroSurfaceShowcase";
import "../../../../packages/marketing/src/marketing-site.css";

const componentStyles = document.createElement("style");
beforeAll(async () => {
  const response = await fetch("/virtual:stylex.css");
  if (!response.ok) throw new Error("StyleX component CSS did not load");
  componentStyles.textContent = `@layer reset;\n${await response.text()}`;
  document.head.prepend(componentStyles);
});
afterAll(() => componentStyles.remove());
afterEach(cleanup);

it.each(["dark", "light"] as const)(
  "component focus rings survive the global fallback in %s theme instead of losing their offset",
  async (theme) => {
    document.documentElement.dataset.siteTheme = theme;
    await render(
      <div {...stylex.props(theme === "light" && lightSiteTheme)}>
        <FaqSection />
        <HeroSurfaceShowcase theme={theme} onToggleTheme={() => {}} />
      </div>,
    );
    const summary = page.getByText("Which agents does it work with?", { exact: true });
    await summary.click();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(summary.element()).outlineOffset).toBe("4px");
    await page.getByRole("tab", { name: "CLI" }).click();
    const replay = page.getByRole("button", { name: "Replay" });
    await replay.click();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(replay.element()).outlineOffset).toBe("1px");
  },
);

it.each(["dark", "light"] as const)(
  "CLI copy command keeps its component ring in %s theme instead of the global color",
  async (theme) => {
    document.documentElement.dataset.siteTheme = theme;
    await render(
      <div {...stylex.props(theme === "light" && lightSiteTheme)}>
        <HeroSurfaceShowcase theme={theme} onToggleTheme={() => {}} />
      </div>,
    );
    await page.getByRole("tab", { name: "CLI" }).click();
    const copy = page.getByRole("button", { name: /copy command/i });
    await copy.click();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(copy.element()).outlineOffset).toBe("2px");
  },
);

it.each(["dark", "light"] as const)(
  "MCP undo action keeps a visible focus ring in %s theme",
  async (theme) => {
    document.documentElement.dataset.siteTheme = theme;
    await render(
      <div {...stylex.props(theme === "light" && lightSiteTheme)}>
        <HeroSurfaceShowcase theme={theme} onToggleTheme={() => {}} />
      </div>,
    );
    await page.getByRole("tab", { name: "MCP Server" }).click();
    const undo = page.getByRole("button", { name: "Undo park" });
    await expect.element(undo).toBeVisible();
    await undo.click();
    const restored = page.getByRole("button", { name: "Restored 12 skills" });
    await expect.element(restored).toBeVisible();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(restored.element()).outlineColor).toBe(
      getComputedStyle(restored.element()).color,
    );
  },
);

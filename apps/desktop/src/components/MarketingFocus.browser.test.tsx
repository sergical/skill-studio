import { afterAll, afterEach, beforeAll, expect, it } from "vitest";
import { page, userEvent } from "vitest/browser";
import { cleanup, render } from "vitest-browser-react";
import * as stylex from "@stylexjs/stylex";
import { lightSiteTheme } from "../../../../packages/marketing/src/SiteTheme.stylex";
import { FaqSection } from "../../../../packages/marketing/src/home/FaqSection";
import { UsageSection } from "../../../../packages/marketing/src/home/UsageSection";
import { SidecarWalkthrough } from "../../../../packages/marketing/src/walkthrough/SidecarWalkthrough";
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
        <UsageSection />
        <SidecarWalkthrough theme={theme} />
      </div>,
    );
    const summary = page.getByText("Which agents does it work with?", { exact: true });
    await summary.click();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(summary.element()).outlineOffset).toBe("4px");
    const replay = page.getByRole("button", { name: "Replay" });
    await replay.click();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(replay.element()).outlineOffset).toBe("1px");
  },
);

it.each(["dark", "light"] as const)(
  "copy and walkthrough controls keep their component ring in %s theme instead of the global color",
  async (theme) => {
    document.documentElement.dataset.siteTheme = theme;
    await render(
      <div {...stylex.props(theme === "light" && lightSiteTheme)}>
        <UsageSection />
        <SidecarWalkthrough theme={theme} />
      </div>,
    );
    await page.getByRole("tab", { name: /CLI/ }).click();
    const copy = page.getByRole("button", { name: /copy/i });
    await copy.click();
    await userEvent.keyboard("{Tab}{Shift>}{Tab}{/Shift}");
    expect(getComputedStyle(copy.element()).outlineOffset).toBe("-4px");
    const expand = page.getByRole("button", { name: "Expand walkthrough" });
    await expand.click();
    await userEvent.keyboard("{Tab}");
    const dialog = page.getByRole("dialog").element();
    const video = dialog.querySelector("video");
    if (!video || !video.parentElement) throw new Error("expanded video is missing");
    const ring = getComputedStyle(video);
    expect(parseFloat(getComputedStyle(video.parentElement).paddingLeft)).toBeGreaterThanOrEqual(
      parseFloat(ring.outlineWidth) + parseFloat(ring.outlineOffset),
    );
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    expect(getComputedStyle(expand.element()).outlineOffset).toBe("2px");
  },
);

it.each(["dark", "light"] as const)(
  "Undo keeps a visible ring and toast edge in %s theme instead of blending into the window",
  async (theme) => {
    document.documentElement.dataset.siteTheme = theme;
    await render(
      <div {...stylex.props(theme === "light" && lightSiteTheme)}>
        <UsageSection />
      </div>,
    );
    await page.getByRole("tab", { name: /^App/ }).click();
    await page.getByRole("button", { name: "Replay" }).click();
    const undo = page.getByRole("button", { name: "Undo" });
    await expect.element(undo).toBeVisible();
    await undo.click();
    await page.getByRole("button", { name: "Replay" }).click();
    await expect.element(undo).toBeVisible();
    await userEvent.keyboard("{Tab}");
    expect(getComputedStyle(undo.element()).outlineColor).toBe(
      getComputedStyle(undo.element()).color,
    );
    const toast = undo.element().parentElement;
    if (!toast) throw new Error("toast is missing");
    expect(getComputedStyle(toast).boxShadow).toContain("1px");
  },
);

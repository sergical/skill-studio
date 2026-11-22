// ============================================================================
// Skill Studio - Docs client
// The docs and 404 pages are static HTML. This script adds the copy buttons
// and the theme toggle with delegated listeners, and ships no React.
// ============================================================================

import type { SiteTheme } from "../SiteTheme.stylex";
import {
  applySiteTheme,
  currentSiteTheme,
  onSystemThemeChange,
  rememberSiteTheme,
  rememberedSiteTheme,
  transitionSiteTheme,
} from "../site-theme";
import { bufferEarlyErrors, startSiteSentry } from "../site-sentry";
import "../marketing-site.css";
import "./docs-prose.css";
// Imported for its StyleX rules, which the build collects into the shared CSS file.
import "./docs-page-styles";

// The SDK chunk loads after this script finishes, so a setup error below is still buffered.
void startSiteSentry(bufferEarlyErrors());

const COPY_LABEL_RESET_MS = 2000;
const copyResetTimers = new WeakMap<HTMLElement, number>();

function showCopyLabel(button: HTMLElement, text: string, state?: "copied") {
  const label = button.querySelector<HTMLElement>("[data-copy-label]");
  const original = button.hasAttribute("data-copy-page") ? "Copy page as Markdown" : "Copy";
  if (label) label.textContent = text;
  if (state) button.dataset.state = state;
  window.clearTimeout(copyResetTimers.get(button));
  copyResetTimers.set(
    button,
    window.setTimeout(() => {
      delete button.dataset.state;
      if (label) label.textContent = original;
    }, COPY_LABEL_RESET_MS),
  );
}

async function copyCode(button: HTMLElement) {
  const code = button.parentElement?.querySelector("code");
  if (!code) return;
  try {
    await navigator.clipboard.writeText(code.textContent ?? "");
    showCopyLabel(button, "Copied", "copied");
  } catch {
    // Clipboard access can be blocked. Selecting the code still lets the reader copy it.
    window.getSelection()?.selectAllChildren(code);
    showCopyLabel(button, "Press Cmd+C");
  }
}

async function copyPage(button: HTMLElement, url: string) {
  try {
    // Safari requires the clipboard write to start during the click.
    const markdown = fetch(url).then((response) => {
      if (!response.ok) throw new Error("Could not load Markdown");
      return response.text().then((body) => new Blob([body], { type: "text/plain" }));
    });
    await navigator.clipboard.write([new ClipboardItem({ "text/plain": markdown })]);
    showCopyLabel(button, "Copied", "copied");
  } catch {
    showCopyLabel(button, "Use View Markdown to copy");
  }
}

const pageCopy = document.querySelector<HTMLElement>("[data-copy-page]");
if (
  pageCopy &&
  navigator.clipboard &&
  "write" in navigator.clipboard &&
  "ClipboardItem" in window
) {
  pageCopy.hidden = false;
  pageCopy.addEventListener("click", () => {
    if (pageCopy.dataset.copyPage) void copyPage(pageCopy, pageCopy.dataset.copyPage);
  });
}

const docsSearch = document.querySelector<HTMLInputElement>("#docs-search");
if (docsSearch) {
  const groups = document.querySelectorAll<HTMLElement>("[data-docs-group]");
  const empty = document.querySelector<HTMLElement>("[data-docs-search-empty]");
  const toggle = document.querySelector<HTMLButtonElement>(".docs-nav-toggle");
  toggle?.removeAttribute("hidden");
  toggle?.addEventListener("click", () => {
    toggle.setAttribute("aria-expanded", String(toggle.getAttribute("aria-expanded") === "false"));
  });
  document.querySelector("[data-docs-search-label]")?.removeAttribute("hidden");
  docsSearch.hidden = false;
  docsSearch.addEventListener("input", () => {
    const query = docsSearch.value.toLowerCase().trim();
    const words = query.split(/\s+/);
    toggle?.setAttribute("aria-expanded", String(query.length > 0));
    let matches = 0;
    for (const group of groups) {
      let groupMatches = 0;
      for (const link of group.querySelectorAll<HTMLElement>("[data-docs-headings]")) {
        const text = `${link.textContent} ${link.dataset.docsHeadings}`.toLowerCase();
        const match = words.every((word) => text.includes(word));
        link.hidden = !match;
        if (match) groupMatches++;
      }
      group.hidden = groupMatches === 0;
      matches += groupMatches;
    }
    if (empty) empty.hidden = matches > 0;
  });
}

let stopFollowingSystem: (() => void) | undefined =
  rememberedSiteTheme() === null ? onSystemThemeChange(applySiteTheme) : undefined;

function toggleTheme(button: HTMLElement) {
  const nextTheme: SiteTheme = currentSiteTheme() === "dark" ? "light" : "dark";
  const bounds = button.getBoundingClientRect();
  stopFollowingSystem?.();
  stopFollowingSystem = undefined;
  rememberSiteTheme(nextTheme);
  transitionSiteTheme(
    { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 },
    () => applySiteTheme(nextTheme),
  );
}

document.addEventListener("click", (event) => {
  if (!(event.target instanceof Element)) return;
  const copyButton = event.target.closest<HTMLElement>("[data-copy]");
  if (copyButton) {
    void copyCode(copyButton);
    return;
  }
  const themeButton = event.target.closest<HTMLElement>("[data-theme-toggle]");
  if (themeButton) toggleTheme(themeButton);
});

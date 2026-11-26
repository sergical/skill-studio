// ============================================================================
// Skill Studio - Docs client
// The docs and 404 pages are static HTML. This script adds the copy buttons
// and the theme toggle with delegated listeners.
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
import { rememberDocsNavScroll } from "./docs-nav-scroll";
import { setupDocsOutline } from "./docs-outline";
import { setupDocsSearch } from "./docs-search";
// Imported for its StyleX rules, which the build collects into the shared CSS file.
import "./docs-page-styles";

// The SDK chunk loads after this script finishes, so a setup error below is still buffered.
void startSiteSentry(bufferEarlyErrors());

const COPY_LABEL_RESET_MS = 1500;
const copyResetTimers = new WeakMap<HTMLElement, number>();

function showCopyLabel(button: HTMLElement, text: string, state?: "copied") {
  const label = button.querySelector<HTMLElement>("[data-copy-label]");
  const original = button.hasAttribute("data-copy-page") ? "Copy page" : "Copy";
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
  const code = button.closest(".docs-code")?.querySelector("code");
  if (!code) return;
  try {
    await navigator.clipboard.writeText(code.textContent ?? "");
    showCopyLabel(button, "Copied", "copied");
  } catch {
    // Clipboard access can be blocked. Selecting the code still lets the reader copy it.
    window.getSelection()?.selectAllChildren(code);
    const shortcut = /Mac|iPhone|iPad/.test(navigator.userAgent) ? "⌘C" : "Ctrl+C";
    showCopyLabel(button, shortcut);
  }
}

async function copyPage(button: HTMLElement, url: string) {
  try {
    const markdown = fetch(url).then((response) => {
      if (!response.ok) throw new Error("Could not load Markdown");
      return response.text();
    });
    if ("ClipboardItem" in window) {
      // Safari requires the clipboard write to start during the click.
      const blob = markdown.then((body) => new Blob([body], { type: "text/plain" }));
      await navigator.clipboard.write([new ClipboardItem({ "text/plain": blob })]);
    } else {
      await navigator.clipboard.writeText(await markdown);
    }
    showCopyLabel(button, "Copied", "copied");
  } catch {
    showCopyLabel(button, "Copy failed");
  }
}

const pageCopy = document.querySelector<HTMLElement>("[data-copy-page]");
pageCopy?.addEventListener("click", () => {
  if (pageCopy.dataset.copyPage) void copyPage(pageCopy, pageCopy.dataset.copyPage);
});

setupDocsSearch();
setupDocsOutline();
rememberDocsNavScroll();

const agentMenu = document.querySelector<HTMLElement>("[data-docs-agent-menu]");
if (agentMenu?.dataset.docsAgentMenu) {
  const markdownPath = agentMenu.dataset.docsAgentMenu;
  void import("./DocsAgentMenu").then(({ mountDocsAgentMenu }) => {
    mountDocsAgentMenu(agentMenu, markdownPath);
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

// ============================================================================
// Skill Studio - Docs markdown
// Turns a docs page's markdown into HTML at prerender time
// ============================================================================

import { Marked } from "marked";

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function headingId(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

// Lucide's copy and check icons. Markdown renders to a string, so they are inlined here.
const COPY_ICONS = [
  '<svg class="docs-copy-icon" data-copy-icon="idle" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>',
  '<svg class="docs-copy-icon" data-copy-icon="done" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>',
].join("");

const docsMarked = new Marked({
  gfm: true,
  renderer: {
    // The toolbar sits above the scrollport so it never covers code.
    code({ text, lang }) {
      const language = lang ? ` class="language-${escapeHtml(lang)}"` : "";
      const label = lang ? `<span class="docs-code-lang">${escapeHtml(lang)}</span>` : "";
      return `<figure class="docs-code"><div class="docs-code-toolbar">${label}<button type="button" class="docs-button docs-code-copy" data-copy="">${COPY_ICONS}<span data-copy-label="" aria-live="polite">Copy</span></button></div><pre tabindex="0" aria-label="Scrollable code example"><code${language}>${escapeHtml(text)}</code></pre></figure>\n`;
    },
    heading({ tokens, depth, text }) {
      const content = this.parser.parseInline(tokens);
      if (depth === 1) return `<h1>${content}</h1>\n`;
      return `<h${depth} id="${headingId(text)}">${content}</h${depth}>\n`;
    },
  },
});

export interface DocsOutlineEntry {
  id: string;
  text: string;
}

export interface RenderedDocsPage {
  /** The page title and the lead paragraph under it. */
  intro: string;
  body: string;
  outline: DocsOutlineEntry[];
}

/** Splits the title and lead paragraph from the rest so the page actions can sit between them. */
export function renderDocsPage(markdown: string): RenderedDocsPage {
  const tokens = docsMarked.lexer(markdown);
  const titleIndex = tokens.findIndex((token) => token.type === "heading" && token.depth === 1);
  const leadIndex = tokens.findIndex(
    (token, index) => index > titleIndex && token.type === "paragraph",
  );
  const introEnd = titleIndex === -1 ? 0 : (leadIndex === -1 ? titleIndex : leadIndex) + 1;
  return {
    intro: docsMarked.parser(tokens.slice(0, introEnd)),
    body: docsMarked.parser(tokens.slice(introEnd)),
    outline: tokens.flatMap((token) =>
      token.type === "heading" && token.depth === 2
        ? [{ id: headingId(token.text), text: token.text }]
        : [],
    ),
  };
}

// Use the Markdown lexer so lines inside code examples never become search terms.
export function docsSearchHeadings(markdown: string): string {
  return docsMarked
    .lexer(markdown)
    .filter((token) => token.type === "heading")
    .map((token) => token.raw.trim().replace(/^#+\s*/, ""))
    .join(" ");
}

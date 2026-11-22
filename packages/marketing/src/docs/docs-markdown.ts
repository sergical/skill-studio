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

const docsMarked = new Marked({
  gfm: true,
  renderer: {
    // The toolbar sits above the scrollport so it never covers code.
    code({ text, lang }) {
      const language = lang ? ` class="language-${escapeHtml(lang)}"` : "";
      return `<figure class="docs-code"><div class="docs-code-toolbar"><button type="button" class="docs-action docs-code-copy" data-copy=""><span data-copy-label="" aria-live="polite">Copy</span></button></div><pre tabindex="0" aria-label="Scrollable code example"><code${language}>${escapeHtml(text)}</code></pre></figure>\n`;
    },
    heading({ tokens, depth, text }) {
      const content = this.parser.parseInline(tokens);
      if (depth === 1) return `<h1>${content}</h1>\n`;
      return `<h${depth} id="${headingId(text)}">${content}</h${depth}>\n`;
    },
  },
});

export function renderDocsMarkdown(markdown: string): string {
  return docsMarked.parse(markdown, { async: false });
}

// Use the Markdown lexer so lines inside code examples never become search terms.
export function docsSearchHeadings(markdown: string): string {
  return docsMarked
    .lexer(markdown)
    .filter((token) => token.type === "heading")
    .map((token) => token.raw.trim().replace(/^#+\s*/, ""))
    .join(" ");
}

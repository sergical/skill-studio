// ============================================================================
// Skill Studio - Site prerender
// Server entry that turns each HTML template into a finished static page.
// The build runs it from scripts/prerender-docs.mjs; the dev server runs it
// through the dev-only plugin in vite.config.ts.
// ============================================================================

import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { DocsPage } from "../docs/DocsPage";
import { DOCS_PAGES, type DocsPageEntry } from "../docs/docs-pages";
import { NotFoundPage } from "../docs/NotFoundPage";
import { MarketingSite } from "../MarketingSite";
import { SHARE_CARD_ALT } from "../share-card";
import { themeBootScript } from "../site-theme";

import { DOCS_TEMPLATE } from "./site-templates";

const SITE_ORIGIN = "https://useskillstudio.com";
const OG_IMAGE_ALT_TAG = `<meta property="og:image:alt" content="${escapeAttribute(SHARE_CARD_ALT)}" />`;

interface SiteRoute {
  /** Output path relative to dist. */
  file: string;
  template: string;
  /** Written to dist next to the page: the plain markdown copy of a docs page. */
  markdownFile?: string;
  markdown?: string;
}

type RouteKind = { kind: "home" } | { kind: "docs"; page: DocsPageEntry } | { kind: "not-found" };

function routeKind(file: string): RouteKind | undefined {
  if (file === "index.html") return { kind: "home" };
  if (file === "404.html") return { kind: "not-found" };
  const page = DOCS_PAGES.find((entry) => `${entry.path.slice(1)}index.html` === file);
  return page ? { kind: "docs", page } : undefined;
}

export const SITE_ROUTES: SiteRoute[] = [
  { file: "index.html", template: "index.html" },
  ...DOCS_PAGES.map((page) => ({
    file: `${page.path.slice(1)}index.html`,
    template: DOCS_TEMPLATE,
    markdownFile: page.markdownPath,
    markdown: page.markdown,
  })),
  { file: "404.html", template: "404.html" },
];

function escapeAttribute(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");
}

interface PageHead {
  title: string;
  description: string;
  path?: string;
  markdownPath?: string;
}

function renderHead({ title, description, path, markdownPath }: PageHead): string {
  const tags = [
    `<title>${escapeAttribute(title)}</title>`,
    `<meta name="description" content="${escapeAttribute(description)}" />`,
  ];
  if (!path) {
    tags.push(`<meta name="robots" content="noindex" />`);
    return tags.join("\n    ");
  }

  const url = `${SITE_ORIGIN}${path}`;
  tags.push(
    `<link rel="canonical" href="${url}" />`,
    ...(markdownPath
      ? [`<link rel="alternate" type="text/markdown" href="${SITE_ORIGIN}/${markdownPath}" />`]
      : []),
    `<meta property="og:type" content="article" />`,
    `<meta property="og:site_name" content="Skill Studio" />`,
    `<meta property="og:title" content="${escapeAttribute(title)}" />`,
    `<meta property="og:description" content="${escapeAttribute(description)}" />`,
    `<meta property="og:url" content="${url}" />`,
    `<meta property="og:image" content="${SITE_ORIGIN}/og.png" />`,
    `<meta property="og:image:width" content="1200" />`,
    `<meta property="og:image:height" content="630" />`,
    OG_IMAGE_ALT_TAG,
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${escapeAttribute(title)}" />`,
    `<meta name="twitter:description" content="${escapeAttribute(description)}" />`,
    `<meta name="twitter:image" content="${SITE_ORIGIN}/og.png" />`,
  );
  return tags.join("\n    ");
}

function fillTemplate(template: string, head: string, body: ReactElement, rootAttributes = "") {
  const markup = renderToStaticMarkup(body);
  return template
    .replace("<!--site-theme-->", `<script>${themeBootScript()}</script>`)
    .replace("<!--site-head-->", head)
    .replace('<div id="root"></div>', `<div id="root"${rootAttributes}>${markup}</div>`);
}

/** Fills one HTML template. `file` is the template path relative to the package root. */
export function renderSitePage(file: string, template: string): string {
  const route = routeKind(file);
  if (!route) throw new Error(`No prerender route for ${file}`);

  switch (route.kind) {
    case "home":
      // The prerender cannot know the visitor's theme, so it draws the dark one and
      // marketing-site.css hides it from light visitors until the client render lands.
      return fillTemplate(
        template,
        OG_IMAGE_ALT_TAG,
        <MarketingSite prerenderTheme="dark" />,
        ' data-prerendered="dark"',
      );
    case "docs":
      return fillTemplate(
        template,
        renderHead({
          title: route.page.title,
          description: route.page.description,
          path: route.page.path,
          markdownPath: route.page.markdownPath,
        }),
        <DocsPage page={route.page} />,
      );
    case "not-found":
      return fillTemplate(
        template,
        renderHead({
          title: "Page not found | Skill Studio",
          description: "This page does not exist.",
        }),
        <NotFoundPage />,
      );
  }
}

export function renderLlmsText(): string {
  const first = ["/docs/", "/docs/agent-setup/", "/docs/requirements/"];
  const pages = [
    ...first.map((path) => DOCS_PAGES.find((page) => page.path === path)),
    ...DOCS_PAGES.filter((page) => !first.includes(page.path)),
  ];
  const links = pages.map((page) => {
    if (!page) throw new Error("llms.txt is missing a starting page");
    return `- [${page.navLabel}](${SITE_ORIGIN}/${page.markdownPath}): ${page.description}`;
  });
  return [
    "# Skill Studio",
    "",
    "> A macOS app, CLI and MCP server for finding and managing agent skills.",
    "",
    "## Docs",
    "",
    ...links,
    "",
  ].join("\n");
}

export function renderSitemap(): string {
  const paths = ["/", ...DOCS_PAGES.map((page) => page.path)];
  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
    ...paths.map((path) => `  <url><loc>${SITE_ORIGIN}${path}</loc></url>`),
    "</urlset>",
    "",
  ].join("\n");
}

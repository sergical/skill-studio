// ============================================================================
// Skill Studio - Docs page
// Prerendered to static HTML; the client adds search, copy and theme controls
// ============================================================================

import * as stylex from "@stylexjs/stylex";

import { getPaletteTheme } from "../PaletteThemes.stylex";
import { SiteFooter, SiteHeader, siteLayout } from "../SiteChrome";
import { docsPageStyles as styles } from "./docs-page-styles";
import { docsSearchHeadings, renderDocsMarkdown } from "./docs-markdown";
import { DOCS_GROUPS, type DocsPageEntry } from "./docs-pages";

interface DocsPageProps {
  page: DocsPageEntry;
}

export function DocsPage({ page }: DocsPageProps) {
  const nav = stylex.props(styles.nav, styles.colors);

  return (
    <div id="top" {...stylex.props(getPaletteTheme("violet"), siteLayout.page)}>
      <SiteHeader page="docs" showThemeToggle />
      <div {...stylex.props(siteLayout.container, styles.layout)}>
        <nav {...nav} aria-label="Docs">
          <label
            {...stylex.props(styles.navHeading)}
            htmlFor="docs-search"
            data-docs-search-label
            hidden
          >
            Search docs
          </label>
          <input id="docs-search" type="search" {...stylex.props(styles.search)} hidden />
          <p data-docs-search-empty hidden role="status">
            No matching pages.
          </p>
          <button
            className="docs-copy docs-nav-toggle"
            type="button"
            aria-controls="docs-links"
            aria-expanded="false"
            hidden
          >
            Browse docs
          </button>
          <div id="docs-links" className="docs-links">
            {DOCS_GROUPS.map(({ group, pages }) => (
              <section key={group} aria-label={group} data-docs-group>
                <div {...stylex.props(styles.navHeading)}>{group}</div>
                {pages.map((entry) => (
                  <a
                    key={entry.slug}
                    href={entry.path}
                    aria-current={entry.slug === page.slug ? "page" : undefined}
                    data-docs-headings={docsSearchHeadings(entry.markdown)}
                    {...stylex.props(
                      styles.navLink,
                      entry.slug === page.slug && styles.navLinkCurrent,
                    )}
                  >
                    {entry.navLabel}
                  </a>
                ))}
              </section>
            ))}
          </div>
        </nav>
        <main {...stylex.props(styles.article, styles.colors)}>
          <div className="docs-page-actions">
            <button
              type="button"
              className="docs-copy"
              data-copy-page={`/${page.markdownPath}`}
              hidden
            >
              <span data-copy-label="" aria-live="polite">
                Copy page as Markdown
              </span>
            </button>
            <a href={`/${page.markdownPath}`} {...stylex.props(styles.markdownLink)}>
              View Markdown
            </a>
          </div>
          <article
            className="docs-prose"
            dangerouslySetInnerHTML={{ __html: renderDocsMarkdown(page.markdown) }}
          />
        </main>
      </div>
      <SiteFooter page="docs" />
    </div>
  );
}

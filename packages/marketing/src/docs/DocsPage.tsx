// ============================================================================
// Skill Studio - Docs page
// Prerendered to static HTML; the agent menu mounts as a separate React island.
// ============================================================================

import * as stylex from "@stylexjs/stylex";
import { ChevronDown, Search } from "lucide-react";

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
          <div className="docs-nav-controls">
            <form className="docs-search" role="search" hidden>
              <label className="docs-sr-only" htmlFor="docs-search">
                Search docs
              </label>
              <Search aria-hidden="true" size={16} className="docs-search-icon" />
              <input
                id="docs-search"
                type="search"
                placeholder="Search docs"
                autoComplete="off"
                spellCheck={false}
                aria-controls="docs-links"
                aria-keyshortcuts="/"
                {...stylex.props(styles.search)}
              />
              <kbd aria-hidden="true">/</kbd>
            </form>
            <p className="docs-search-empty" data-docs-search-empty hidden role="status">
              No matching pages.
            </p>
            <button
              className="docs-action docs-nav-toggle"
              type="button"
              aria-controls="docs-links"
              aria-expanded="false"
              hidden
            >
              Browse docs <ChevronDown aria-hidden="true" size={14} />
            </button>
          </div>
          <div id="docs-links" className="docs-links">
            {DOCS_GROUPS.map(({ group, pages }) => (
              <section className="docs-nav-group" key={group} aria-label={group} data-docs-group>
                <div {...stylex.props(styles.navHeading)}>{group}</div>
                {pages.map((entry) => (
                  <a
                    key={entry.slug}
                    href={entry.path}
                    aria-current={entry.slug === page.slug ? "page" : undefined}
                    data-docs-headings={docsSearchHeadings(entry.markdown)}
                    data-docs-title={entry.title}
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
              className="docs-action docs-page-copy"
              data-copy-page={`/${page.markdownPath}`}
              hidden
            >
              <span data-copy-label="" aria-live="polite">
                Copy page as Markdown
              </span>
            </button>
            <a href={`/${page.markdownPath}`} className="docs-action">
              View Markdown
            </a>
            <div data-docs-agent-menu={`/${page.markdownPath}`} />
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

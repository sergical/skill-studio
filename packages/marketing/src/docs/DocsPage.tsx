// ============================================================================
// Skill Studio - Docs page
// Prerendered to static HTML; the agent menu mounts as a separate React island.
// ============================================================================

import * as stylex from "@stylexjs/stylex";
import { Check, ChevronDown, Copy, Search } from "lucide-react";

import { getPaletteTheme } from "../PaletteThemes.stylex";
import { SiteFooter, SiteHeader, siteLayout } from "../SiteChrome";
import { DocsAgentTriggerContent } from "./DocsAgentTrigger";
import { docsPageStyles as styles } from "./docs-page-styles";
import { docsSearchHeadings, renderDocsPage } from "./docs-markdown";
import { docsNavScrollRestoreScript } from "./docs-nav-scroll";
import { DOCS_GROUPS, type DocsPageEntry } from "./docs-pages";

interface DocsPageProps {
  page: DocsPageEntry;
}

// Without scripts the search, the copy button and the menu cannot work, so they hide
// and the page list stays open on small screens.
const NO_SCRIPT_CSS =
  ".docs-search,.docs-page-actions,.docs-nav-toggle{display:none!important}.docs-links{display:block!important}";

export function DocsPage({ page }: DocsPageProps) {
  const nav = stylex.props(styles.nav, styles.colors);
  const { intro, body, outline } = renderDocsPage(page.markdown);

  return (
    <div id="top" {...stylex.props(getPaletteTheme("violet"), siteLayout.page)}>
      <noscript>
        <style>{NO_SCRIPT_CSS}</style>
      </noscript>
      <SiteHeader page="docs" showThemeToggle />
      <div {...stylex.props(siteLayout.container, styles.layout)}>
        <nav {...nav} aria-label="Docs">
          <div className="docs-nav-controls">
            <form className="docs-search" role="search">
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
              className="docs-button docs-nav-toggle"
              type="button"
              aria-controls="docs-links"
              aria-expanded="false"
            >
              Browse docs <ChevronDown aria-hidden="true" size={14} className="docs-chevron" />
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
          <script dangerouslySetInnerHTML={{ __html: docsNavScrollRestoreScript() }} />
        </nav>
        <main id="main" {...stylex.props(styles.article, styles.colors)}>
          <article className="docs-prose">
            <header className="docs-page-header">
              <div dangerouslySetInnerHTML={{ __html: intro }} />
              <div className="docs-page-actions">
                <button
                  type="button"
                  className="docs-button docs-page-copy"
                  data-copy-page={`/${page.markdownPath}`}
                >
                  <Copy aria-hidden="true" size={14} data-copy-icon="idle" />
                  <Check aria-hidden="true" size={14} data-copy-icon="done" />
                  <span data-copy-label="" aria-live="polite">
                    Copy page
                  </span>
                </button>
                {/* The menu mounts here after load; the same trigger is drawn first so nothing moves. */}
                <div className="docs-agent-slot" data-docs-agent-menu={`/${page.markdownPath}`}>
                  <button
                    type="button"
                    className="docs-button docs-agent-trigger"
                    aria-haspopup="menu"
                    aria-expanded="false"
                    aria-label="Open page in"
                  >
                    <DocsAgentTriggerContent />
                  </button>
                </div>
              </div>
            </header>
            <div dangerouslySetInnerHTML={{ __html: body }} />
          </article>
        </main>
        {outline.length > 1 && (
          <aside {...stylex.props(styles.outline, styles.colors)}>
            <nav className="docs-outline" aria-label="On this page">
              <p className="docs-outline-heading">On this page</p>
              <ul>
                {outline.map(({ id, text }) => (
                  <li key={id}>
                    <a href={`#${id}`} data-outline-link={id}>
                      {text}
                    </a>
                  </li>
                ))}
              </ul>
            </nav>
          </aside>
        )}
      </div>
      <SiteFooter page="docs" />
    </div>
  );
}

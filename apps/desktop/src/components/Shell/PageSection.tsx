// ============================================================================
// PageSection - One titled part of a page. The title sits on the page's left
// edge with its controls or a note on the right, and the content starts 12px
// below it. Every section title on a page uses this, so titles share one edge
// and one gap whether the content is a table, a card or a list.
// ============================================================================

import { useId } from "react";
import type { ReactNode } from "react";

interface PageSectionProps {
  title: ReactNode;
  /** The right side of the title row: a range control, a link, or a short note. */
  meta?: ReactNode;
  children: ReactNode;
}

export function PageSection({ title, meta, children }: PageSectionProps) {
  const titleId = useId();
  return (
    <section aria-labelledby={titleId}>
      <div className="mb-3 flex min-h-7 items-center justify-between gap-3">
        <h2 id={titleId} className="text-heading font-semibold text-text-primary">
          {title}
        </h2>
        {meta && (
          <div className="flex items-center gap-2 text-small text-text-secondary">{meta}</div>
        )}
      </div>
      {children}
    </section>
  );
}

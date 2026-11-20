// ============================================================================
// PageShell - The slim header bar (breadcrumb, title, actions) and optional
// toolbar row every view sits under, plus the scrolling content area. The
// sidebar holds places; a page's own filter bar (if any) is the `toolbar`.
// ============================================================================

import type { ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { Button } from "@skill-studio/ui";

interface PageShellProps {
  title: string;
  /** Parent crumb, shown before the title as "Skills ›". Clicking it runs `onClick`. */
  parent?: { label: string; onClick: () => void };
  subtitle?: string;
  /** Right side of the header: buttons or a few filter menus. Controls inside are 28px tall. */
  actions?: ReactNode;
  /** Thin row under the header, for a filter bar too wide for `actions`. Controls inside are 28px tall. */
  toolbar?: ReactNode;
  children: ReactNode;
  /** "narrow" (720px) for a detail-style page, e.g. a pack's detail view. */
  width?: "default" | "narrow";
}

/**
 * The first word in the header: a top-level page's title, or the parent crumb on a child page.
 * One disabled-or-enabled button for both, so "Skills" doesn't move when it becomes a link.
 */
function RootCrumb({ label, onClick }: { label: string; onClick?: () => void }) {
  return (
    <Button
      variant="ghost"
      size="xs"
      disabled={!onClick}
      // -7px = the 6px hover padding plus the button's 1px transparent border, so the text starts
      // at the header's own padding edge.
      className="-ml-[7px] h-6 min-w-0 rounded-sm px-1.5 text-body text-text-tertiary hover:text-text-primary disabled:font-medium disabled:text-text-primary disabled:opacity-100"
      onClick={onClick}
    >
      <span className="truncate">{label}</span>
    </Button>
  );
}

/**
 * A `<section>` with a one-line header (parent crumb, title, actions on the
 * right), an optional toolbar row, and a scrolling content area below.
 */
export function PageShell({
  title,
  parent,
  subtitle,
  actions,
  toolbar,
  children,
  width = "default",
}: PageShellProps) {
  // Header, toolbar and content share one column, so the title, the filters and the content
  // start on the same left edge (and end on the same right edge) at every window width.
  const column = `mx-auto flex w-full px-6 ${width === "narrow" ? "max-w-180" : "max-w-300"}`;
  return (
    <section className="flex min-h-0 flex-1 flex-col">
      {/* The header and toolbar reserve the scrollbar's width on the right, as the content's
          scroll container does, so all three columns centre in the same width. One line runs
          under the last fixed row only. The bar's empty space drags the window. */}
      <header
        data-tauri-drag-region
        className={`shrink-0 pr-(--scrollbar-size) ${toolbar ? "" : "border-b border-border-subtle"}`}
      >
        <div data-tauri-drag-region className={`h-11 items-center justify-between gap-3 ${column}`}>
          <div data-tauri-drag-region className="flex min-w-0 items-center gap-1.5 text-body">
            {parent ? (
              <>
                <RootCrumb label={parent.label} onClick={parent.onClick} />
                <ChevronRight size={12} className="shrink-0 text-text-quaternary" />
                <h1 className="m-0 truncate text-body font-medium text-text-primary">{title}</h1>
              </>
            ) : (
              <h1 className="m-0 flex min-w-0">
                <RootCrumb label={title} />
              </h1>
            )}
            {subtitle && (
              <span className="min-w-0 truncate pl-1 text-small text-text-tertiary">
                {subtitle}
              </span>
            )}
          </div>
          {actions && (
            <div className="flex shrink-0 items-center gap-1.5 [--control-height:28px]">
              {actions}
            </div>
          )}
        </div>
      </header>
      {toolbar && (
        <div className="shrink-0 border-b border-border-subtle pr-(--scrollbar-size)">
          <div className={`h-11 items-center gap-2 pb-1 [--control-height:28px] ${column}`}>
            {toolbar}
          </div>
        </div>
      )}
      {/* The app's styled scrollbar takes width, so the page always shows its (transparent) track to keep content from shifting when it starts to scroll. */}
      <div className="min-h-0 flex-1 overflow-y-scroll">
        <div className={`flex-col gap-5 pt-5 pb-7 ${column}`}>{children}</div>
      </div>
    </section>
  );
}

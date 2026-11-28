// ============================================================================
// SummaryCard - The card a report opens with: Home's Skill use, an agent's
// What it sees and What it used. Its title is a PageSection title on the
// page's left edge, like every other section; only the answer sits in the
// card. One surface and one number size, so the same kind of answer looks the
// same on every page.
// ============================================================================

import type { ReactNode } from "react";
import { PageSection } from "../Shell/PageSection";

interface SummaryCardProps {
  title: ReactNode;
  /** Controls on the title's row, such as a date range. */
  actions?: ReactNode;
  children: ReactNode;
}

export function SummaryCard({ title, actions, children }: SummaryCardProps) {
  return (
    <PageSection title={title} meta={actions}>
      <div className="@container rounded-lg border border-border-subtle bg-bg-secondary p-4">
        {children}
      </div>
    </PageSection>
  );
}

/** The card's one number and what it counts, read as a sentence: "12 of 35 skills keep a description". */
export function SummaryMetric({ value, unit }: { value: ReactNode; unit: ReactNode }) {
  return (
    <p className="flex flex-wrap items-baseline gap-x-2">
      <strong className="text-display leading-none font-semibold text-text-primary tabular-nums">
        {value}
      </strong>
      <span className="text-body text-text-secondary">{unit}</span>
    </p>
  );
}

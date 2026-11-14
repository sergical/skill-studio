// ============================================================================
// skill-row-format - non-component helpers shared by the row cells: the
// row frame, column grid and fix-button classes, the selected-row treatment,
// and the table's sort order. Split out of
// SkillRowCells.tsx so that file exports components only (react-refresh).
// ============================================================================

import type { InstalledSkill, SkillInvocationStats } from "@skill-studio/lib";
import { buttonVariants, cn } from "@skill-studio/ui";
import type { SortMode } from "../../lib/skill-list-sort";

/** One row's frame: 32px, hairline below, no radius of its own. The named group keeps an outer
 * `.group` from lighting up the row's hover-only parts. */
export const ROW_CLASS =
  "group/row grid h-8 w-full min-w-0 items-center rounded-none border-0 border-b border-border-subtle text-left last:border-b-0";

/** The one column grid every skill row uses: slot, name, location, harness stack, note, fix. */
export const SKILL_ROW_GRID =
  "[grid-template-columns:14px_minmax(0,1fr)_160px_148px_240px_88px] gap-x-3 px-3";

/** The row's fix button: quiet text at rest, a full button while the row is hovered or focused. */
export const FIX_CLASS = cn(
  buttonVariants({ variant: "ghost", size: "sm" }),
  "max-w-full justify-self-end text-small text-text-secondary hover:bg-bg-hover hover:text-text-primary group-hover/row:bg-bg-tertiary group-hover/row:text-text-primary group-focus-within/row:bg-bg-tertiary group-focus-within/row:text-text-primary",
);

/** Selected-row treatment shared by the row: an accent border, softer fill, and a left accent bar. */
export function selectedRowClass(selected: boolean): string {
  return selected
    ? "border-accent bg-accent-softer shadow-[inset_2px_0_0_var(--color-accent)]"
    : "";
}

/** Rows in the table's sort order: `name` and `used` order as the Sort select says; `size`
 * (the "largest" option) orders by the full SKILL.md token count, ties broken by name. */
export function sortRows(
  skills: InstalledSkill[],
  sort: SortMode,
  statsBySkill: Map<string, SkillInvocationStats>,
): InstalledSkill[] {
  const rows = [...skills];
  if (sort === "name") {
    rows.sort((a, b) => a.name.localeCompare(b.name));
  } else if (sort === "used") {
    rows.sort(
      (a, b) =>
        (statsBySkill.get(b.name)?.last_30_days ?? 0) -
        (statsBySkill.get(a.name)?.last_30_days ?? 0),
    );
  } else {
    rows.sort((a, b) => b.skill_md_tokens - a.skill_md_tokens || a.name.localeCompare(b.name));
  }
  return rows;
}

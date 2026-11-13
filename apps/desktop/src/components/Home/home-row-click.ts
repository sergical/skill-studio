// ============================================================================
// Skill Studio - Home row click
// Decides whether a click on a Home inbox row opens the skill, and styles the
// row's own action buttons that keep a click on Home
// ============================================================================

import { buttonVariants, cn } from "@skill-studio/ui";

/** One inbox row's trailing action: a quiet ghost button that still reads as a button. */
export const ROW_ACTION_CLASS = cn(
  buttonVariants({ variant: "ghost", size: "sm" }),
  "text-small text-text-secondary hover:bg-bg-hover hover:text-text-primary",
);

/**
 * True when a click on `row` should open its skill. React bubbles clicks from
 * portaled dialogs through the component tree, so a click inside a row
 * action's dialog reaches the row even though the dialog is not in the row's
 * DOM. Those clicks, and clicks on the row's own buttons, belong to the action.
 */
export function rowClickOpensSkill(row: Node, target: Element | null): boolean {
  if (target === null || !row.contains(target)) return false;
  return target.closest("button, a, [role='button']") === null;
}

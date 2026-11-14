// ============================================================================
// RowFixButton - the last cell of every skill row on Home and Skills. One
// shape for the whole act: the fix's name, a spinner over that name while it
// runs, then the past tense ("Updated") until the list is laid out again.
// The cell never changes width, so the row never moves.
// ============================================================================

import type { ReactNode } from "react";
import { Check } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { findLeftBehindPairs } from "@skill-studio/lib";
import type { InstalledSkill } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { ResolveLeftBehindMenu } from "./ResolveLeftBehindMenu";
import { FIX_CLASS } from "./skill-row-format";

interface RowFixButtonProps {
  skill: InstalledSkill;
  /** The fix's verb, as `useSkillListAct` names it ("Update", "Park"); `null` when the row has
   * no fix of its own - it still shows what a bar action did to it, and a left-behind copy still
   * offers its Resolve menu. */
  label: string | null;
  onAct: (label: string, skill: InstalledSkill) => void;
  /** Whether this held row's step is the one running. Defaults to true; a batch that holds many
   * rows at once passes it so only the row it is on spins and the rest wait. */
  busy?: boolean;
  /** A fix with its own control (a menu, a dialog opener), shown in place of the button. */
  children?: ReactNode;
}

export function RowFixButton({ skill, label, onAct, busy, children }: RowFixButtonProps) {
  const act = useAppStore((state) => state.rowActs.get(skill.name));
  if (act) {
    return (
      <span className="inline-flex h-7 items-center gap-1 justify-self-end px-2.5 text-small text-text-tertiary">
        <Check size={14} aria-hidden />
        {act}
      </span>
    );
  }
  if (act === null) {
    const spinning = busy ?? true;
    // A row with no fix of its own shows nothing while it waits its turn in a batch.
    if (label === null && !spinning) return null;
    return (
      <Button variant="ghost" className={FIX_CLASS} busy={spinning} disabled>
        {label ?? "Working"}
      </Button>
    );
  }
  if (children) return children;
  if (label === null) {
    const [pair] = findLeftBehindPairs(skill);
    return pair ? <ResolveLeftBehindMenu skillName={skill.name} pair={pair} /> : null;
  }
  return (
    <Button
      variant="ghost"
      className={FIX_CLASS}
      onClick={(e) => {
        e.stopPropagation();
        // The button disables while it runs, which would drop focus to the page; the row keeps it.
        e.currentTarget.closest<HTMLElement>('[role="row"]')?.focus({ preventScroll: true });
        onAct(label, skill);
      }}
    >
      {label}
    </Button>
  );
}

// ============================================================================
// SkillListRow - One row of SkillListTable: the slot, name, disk location,
// harness stack, note, and fix for one skill, absolutely positioned within
// its group by the caller's virtualizer.
// ============================================================================

import type { CSSProperties } from "react";
import type { InstalledSkill } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { HarnessStack } from "./HarnessStack";
import { RowFixButton } from "./RowFixButton";
import { NoteCell, SkillNameCell, SlotCell } from "./SkillRowCells";
import { ROW_CLASS, SKILL_ROW_GRID, selectedRowClass } from "./skill-row-format";
import { SkillLocationCell } from "./SkillLocationCell";
import { rowClickOpensSkill } from "./skill-row-click";
import { DEFAULT_HARNESS_LIST, whereFacts } from "./skill-row-state";
import type { RowState } from "./skill-row-state";

interface SkillListRowProps {
  skill: InstalledSkill;
  /** This row's position in the grouped `rows` array - `aria-rowindex` and shift-click. */
  index: number;
  /** The virtualizer's absolute positioning within the row's group container. */
  style: CSSProperties;
  state: RowState | null;
  checked: boolean;
  /** Whether any row in the table is checked. */
  hasSelection: boolean;
  /** Whether this row is the skill currently open in the detail panel. */
  isOpenSkill: boolean;
  tabIndex: number;
  rowRef: (el: HTMLDivElement | null) => void;
  onOpen: () => void;
  onAct: (label: string, skill: InstalledSkill) => void;
  onCheckedChange: (shiftKey: boolean) => void;
}

export function SkillListRow({
  skill,
  index,
  style,
  state,
  checked,
  hasSelection,
  isOpenSkill,
  tabIndex,
  rowRef,
  onOpen,
  onAct,
  onCheckedChange,
}: SkillListRowProps) {
  const note = state ? [state.label, state.detail].filter(Boolean).join(" · ") : undefined;
  const busy = useAppStore((s) => s.rowActs.get(skill.name) === null);
  return (
    <div
      ref={rowRef}
      role="row"
      aria-rowindex={index + 1}
      aria-selected={checked}
      aria-busy={busy}
      tabIndex={tabIndex}
      style={style}
      // `scroll-mt-7` (28px, `HEADER_HEIGHT`) keeps a row scrolled to by `scrollIntoView` from
      // surfacing under its group's sticky header.
      className={`${ROW_CLASS} ${SKILL_ROW_GRID} scroll-mt-7 hover:bg-bg-secondary focus-visible:outline-2 focus-visible:outline-accent -outline-offset-2 ${selectedRowClass(
        isOpenSkill,
      )} ${skill.parked ? "text-text-tertiary" : ""}`}
      onClick={(e) => {
        // SAFETY: a click's target is always an Element; browsers retarget clicks on text
        // to the parent element.
        if (rowClickOpensSkill(e.currentTarget, e.target as Element)) onOpen();
      }}
    >
      <div role="gridcell" className="flex items-center justify-center">
        <SlotCell
          skill={skill}
          state={state}
          checked={checked}
          hasSelection={hasSelection}
          onCheckedChange={(_checked, eventDetails) => {
            // SAFETY: the underlying event is a pointer or keyboard event, both of which carry `shiftKey`.
            const shiftKey = (eventDetails.event as MouseEvent | KeyboardEvent).shiftKey;
            onCheckedChange(shiftKey);
          }}
        />
      </div>
      <div role="gridcell" className="contents">
        <SkillNameCell skill={skill} />
      </div>
      <div role="gridcell" className="contents">
        <SkillLocationCell locations={whereFacts(skill, DEFAULT_HARNESS_LIST).locations} />
      </div>
      <div role="gridcell" className="contents">
        <HarnessStack skill={skill} harnessList={DEFAULT_HARNESS_LIST} />
      </div>
      <div role="gridcell" className="contents">
        <NoteCell skill={skill} note={note} />
      </div>
      <div role="gridcell" className="flex min-w-0 justify-end">
        <RowFixButton skill={skill} label={state?.action ?? null} onAct={onAct} />
      </div>
    </div>
  );
}

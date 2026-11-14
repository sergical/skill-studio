// ============================================================================
// SkillListRow - One row of SkillListTable: the slot, name, disk location,
// harness stack, note, and fix for one skill, absolutely positioned within
// its group by the caller's virtualizer.
// ============================================================================

import type { CSSProperties } from "react";
import { Button } from "@skill-studio/ui";
import type { InstalledSkill } from "@skill-studio/lib";
import { HarnessStack } from "./HarnessStack";
import { NoteCell, SkillNameCell, SlotCell } from "./SkillRowCells";
import { FIX_CLASS, ROW_CLASS, SKILL_ROW_GRID, selectedRowClass } from "./skill-row-format";
import { SkillLocationCell } from "./SkillLocationCell";
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
  /** What the row is doing now ("Parking…"), or undefined when idle. */
  busyLabel?: string;
  onAct: (label: string) => void;
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
  busyLabel,
  onAct,
  onCheckedChange,
}: SkillListRowProps) {
  const note = state ? [state.label, state.detail].filter(Boolean).join(" · ") : undefined;
  const fix = state?.action;
  return (
    <div
      ref={rowRef}
      role="row"
      aria-rowindex={index + 1}
      aria-selected={checked}
      aria-busy={busyLabel !== undefined}
      tabIndex={tabIndex}
      style={style}
      // `scroll-mt-7` (28px, `HEADER_HEIGHT`) keeps a row scrolled to by `scrollIntoView` from
      // surfacing under its group's sticky header.
      className={`${ROW_CLASS} ${SKILL_ROW_GRID} scroll-mt-7 hover:bg-bg-secondary focus-visible:outline-2 focus-visible:outline-accent -outline-offset-2 ${selectedRowClass(
        isOpenSkill,
      )} ${skill.parked ? "text-text-tertiary" : ""}`}
      onClick={onOpen}
    >
      <div role="gridcell" className="flex items-center justify-center">
        <SlotCell
          skill={skill}
          state={state}
          checked={checked}
          hasSelection={hasSelection}
          busyLabel={busyLabel}
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
        {fix && (
          <Button
            variant="ghost"
            className={FIX_CLASS}
            disabled={busyLabel !== undefined}
            onClick={(e) => {
              e.stopPropagation();
              onAct(fix);
            }}
          >
            {fix}
          </Button>
        )}
      </div>
    </div>
  );
}

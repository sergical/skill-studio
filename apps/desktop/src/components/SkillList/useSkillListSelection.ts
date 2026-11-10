// ============================================================================
// useSkillListSelection - Row selection state for SkillListTable: which
// deployments are checked, and shift-click range selection. The list is in
// selection mode while at least one row is checked; no other flag holds it.
// ============================================================================

import { useRef } from "react";
import type { InstalledSkill } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";

interface UseSkillListSelection {
  selectedPaths: Set<string>;
  clearSkillSelection: () => void;
  selectSkills: (paths: string[]) => void;
  handleRowCheckboxClick: (index: number, shiftKey: boolean) => void;
}

export function useSkillListSelection(
  rows: InstalledSkill[],
  rowPath: (skill: InstalledSkill) => string | undefined,
): UseSkillListSelection {
  const selectedPaths = useAppStore((state) => state.selectedSkillPaths);
  const toggleSkillSelection = useAppStore((state) => state.toggleSkillSelection);
  const clearSkillSelection = useAppStore((state) => state.clearSkillSelection);
  const selectSkills = useAppStore((state) => state.selectSkills);
  /** Index of the last row checked by click (not shift-click), for shift-click range-select. */
  const lastCheckedIndexRef = useRef<number | null>(null);

  /** Checkbox click for one row - shift-click selects every row between it and the last clicked one, in visible (grouped) order. */
  function handleRowCheckboxClick(index: number, shiftKey: boolean) {
    if (shiftKey && lastCheckedIndexRef.current !== null) {
      const [from, to] = [lastCheckedIndexRef.current, index].sort((a, b) => a - b);
      const range = rows.slice(from, to + 1).map((s) => rowPath(s));
      const next = new Set(selectedPaths);
      range.forEach((path) => path && next.add(path));
      selectSkills([...next]);
    } else {
      const path = rowPath(rows[index]);
      if (path) toggleSkillSelection(path);
    }
    lastCheckedIndexRef.current = index;
  }

  return {
    selectedPaths,
    clearSkillSelection,
    selectSkills,
    handleRowCheckboxClick,
  };
}

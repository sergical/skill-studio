// ============================================================================
// useSkillListAct - Row fix dispatch for Home and Skills: Park/Unpark, Fix,
// and Update act on the deployment target `HomeView` uses, Compare opens the
// skill's compare dialog, and any other label opens the skill's own detail,
// since those flows live there. Fix now covers the repairable spec
// violations too (invalid YAML frontmatter); any issue `fix_skill` leaves
// unrepaired falls through to the detail page, which has its own card for
// a link issue and shows the raw message otherwise. A fix that does work holds
// its row in place and leaves the past tense there ("Updated"); see `rowActs`.
// ============================================================================

import { useAppStore } from "../../store/appStore";
import { useGuardedSkillUpdate } from "../../hooks/useGuardedSkillUpdate";
import { fixSkill, openConflictPaths, parkSkill, unparkSkill } from "../../lib/skill-api";
import { lifecycleTargetForPark } from "../../lib/skill-lifecycle-target";
import type { InstalledSkill, Toast } from "@skill-studio/lib";

/** The two IPC calls `reportFixOutcome` makes, as a real interface rather
 * than an import a test would have to mock: production code gets the
 * default `skill-api.ts` wrappers below, a test hands in its own faithful
 * implementation. */
export interface FixSkillDeps {
  fixSkill: typeof fixSkill;
  openConflictPaths: typeof openConflictPaths;
}

const defaultFixSkillDeps: FixSkillDeps = { fixSkill, openConflictPaths };

/** Reports what `fix_skill` actually did as a toast, and opens the skill's
 * detail page whenever an issue is left unrepaired: a link issue lands on
 * the `SkillRepairCard` that already knows how to fix it, and every other
 * unrepaired issue (a spec violation `fix_skill` has no repair for, a
 * frontmatter repair that failed to apply) lands on the same page so the
 * user can edit the file directly instead of getting a dead-end toast. A
 * conflict opens the editor rather than writing anything, so it gets its
 * own message. Resolves `true` only when a repair was written. */
export async function reportFixOutcome(
  skill: InstalledSkill,
  addToast: (toast: Omit<Toast, "id">) => string,
  openDetail: () => void,
  deps: FixSkillDeps = defaultFixSkillDeps,
): Promise<boolean> {
  const outcome = await deps.fixSkill(skill.name);
  if (outcome.conflicts.length > 0) {
    const first = outcome.conflicts[0];
    await deps.openConflictPaths([first.path_a, first.path_b]);
    addToast({
      type: "info",
      title: `${skill.name} has a conflict`,
      message: `${first.message} Opened both copies in your editor.`,
    });
    return false;
  }
  if (outcome.applied.length > 0) {
    addToast({ type: "success", title: `Fixed ${skill.name}` });
    return true;
  }
  if (outcome.unrepaired.length > 0) {
    openDetail();
    return false;
  }
  addToast({
    type: "error",
    title: `Couldn't fix ${skill.name}`,
    message: "Nothing to repair.",
  });
  return false;
}

/** The past tense a row shows once its fix is done; `null` for a verb that only opens the skill. */
export function pastTenseOf(label: string): string | null {
  switch (label) {
    case "Park":
      return "Parked";
    case "Unpark":
      return "Unparked";
    case "Fix":
      return "Fixed";
    case "Update":
      return "Updated";
    default:
      return null;
  }
}

/**
 * Runs `task` with the skill's row held where it is: busy while it runs, then the past tense
 * on success, or back to normal on failure. A second act on a row that is already running is
 * dropped.
 */
export async function actInPlace(
  names: readonly string[],
  pastTense: string,
  task: () => Promise<boolean>,
): Promise<boolean> {
  const store = useAppStore.getState();
  const held = store.holdSkills(names);
  if (held.length < names.length) {
    store.releaseSkills(held);
    return false;
  }
  const ok = await task().catch(() => false);
  if (ok) store.settleSkills(names, pastTense);
  else store.releaseSkills(names);
  return ok;
}

type ParkVerb = "Park" | "Unpark";

/** Parks or unparks one skill and toasts the result, with an Undo that runs the reverse. */
async function parkWithUndo(
  verb: ParkVerb,
  skill: InstalledSkill,
  addToast: (toast: Omit<Toast, "id">) => string,
): Promise<boolean> {
  const target = lifecycleTargetForPark(skill);
  const run = (v: ParkVerb) => (v === "Park" ? parkSkill(target) : unparkSkill(target));
  const reverse: ParkVerb = verb === "Park" ? "Unpark" : "Park";
  try {
    await run(verb);
  } catch (err) {
    addToast({
      type: "error",
      title: verb === "Park" ? "Couldn't park skill" : "Couldn't unpark skill",
      message: err instanceof Error ? err.message : "Unknown error",
    });
    return false;
  }
  addToast({
    type: "success",
    title: `${pastTenseOf(verb)} ${skill.name}`,
    action: {
      label: "Undo",
      onClick: () => {
        void actInPlace([skill.name], pastTenseOf(reverse) ?? reverse, async () => {
          try {
            await run(reverse);
          } catch (err) {
            addToast({
              type: "error",
              title: `Couldn't undo for ${skill.name}`,
              message: err instanceof Error ? err.message : "Unknown error",
            });
            return false;
          }
          return true;
        }).then((ok) => {
          // Undone, the skill is back where it was, so its row has nothing left to say.
          if (ok) useAppStore.getState().releaseSkills([skill.name]);
        });
      },
    },
  });
  return true;
}

export function useSkillListAct(
  onSelectSkill: (name: string, deploymentPath?: string) => void,
  deploymentPathForSkill: ((skill: InstalledSkill) => string | undefined) | undefined,
) {
  const addToast = useAppStore((state) => state.addToast);
  const openSkill = useAppStore((state) => state.openSkill);
  const guard = useGuardedSkillUpdate();

  async function handleAct(label: string, skill: InstalledSkill): Promise<void> {
    const past = pastTenseOf(label);
    if (past === null) {
      if (label === "Compare") openSkill(skill.name, deploymentPathForSkill?.(skill), "compare");
      else onSelectSkill(skill.name, deploymentPathForSkill?.(skill));
      return;
    }
    await actInPlace([skill.name], past, () => runAct(label, skill));
  }

  async function runAct(label: string, skill: InstalledSkill): Promise<boolean> {
    if (label === "Update") {
      return new Promise<boolean>((settle) => void guard.pullLatest(skill, settle));
    }
    if (label === "Fix") {
      try {
        return await reportFixOutcome(skill, addToast, () =>
          onSelectSkill(skill.name, deploymentPathForSkill?.(skill)),
        );
      } catch (err) {
        addToast({
          type: "error",
          title: `Couldn't fix ${skill.name}`,
          message: err instanceof Error ? err.message : "Unknown error",
        });
        return false;
      }
    }
    return parkWithUndo(label === "Park" ? "Park" : "Unpark", skill, addToast);
  }

  return { handleAct, dialog: guard.dialog };
}

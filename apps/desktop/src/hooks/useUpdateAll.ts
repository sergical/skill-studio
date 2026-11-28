// ============================================================================
// useUpdateAll - runs "Update all" for the Skills list's Updates group: checks
// for local edits first, holds the rows while the batch runs, settles the
// updated ones as "Updated", and toasts the tally. Stop asks the running batch
// to finish the step it is on.
// ============================================================================

import { useState } from "react";
import type { InstalledSkill } from "@skill-studio/lib";
import { skillLocalEdits } from "../lib/skill-api";
import {
  conflictedSkillsNote,
  partitionForkableSkills,
  skillsWithLocalEdits,
} from "../lib/skill-lifecycle-target";
import { useAppStore } from "../store/appStore";
import { startUpdateAll, stopUpdateAll, useActiveUpdateAll } from "./skillBatchUpdates";
import { updateAllFailureMessage, updateAllTitle } from "./update-all";

/** `updates` are the skills that still have an update: what Update all runs on. */
export function useUpdateAll(updates: readonly InstalledSkill[]) {
  const addToast = useAppStore((state) => state.addToast);
  const [editedSkills, setEditedSkills] = useState<InstalledSkill[]>([]);
  // The skills whose edits were checked: a confirm runs on these, not on updates that arrived since.
  const [checkedUpdates, setCheckedUpdates] = useState<readonly InstalledSkill[]>([]);
  const [isCheckingEdits, setIsCheckingEdits] = useState(false);
  const activeBatch = useActiveUpdateAll();

  const runUpdateAll = async (
    targets: readonly InstalledSkill[],
    forkNames?: ReadonlySet<string>,
    skippedNames: ReadonlySet<string> = new Set(),
  ) => {
    const { holdSkills, settleSkills, releaseSkills } = useAppStore.getState();
    // A row already updating on its own stays with that act.
    const names = holdSkills(
      targets.flatMap((skill) => (skippedNames.has(skill.name) ? [] : [skill.name])),
    );
    const held = new Set(names);
    const batch = targets.filter((skill) => held.has(skill.name));
    // `updateAllOutdatedSkills` catches every `pullFork`/`updateAllOwners`
    // rejection itself and folds it into `failures`, so this never throws.
    const tally = await startUpdateAll(batch, forkNames);
    if (!tally) {
      releaseSkills(names);
      return;
    }
    const updated = new Set(tally.updated);
    settleSkills(tally.updated, "Updated");
    releaseSkills(names.filter((name) => !updated.has(name)));
    const conflictNote = conflictedSkillsNote(tally.conflicted ?? []);
    addToast({
      type: tally.failures > 0 || conflictNote || tally.stopped ? "warning" : "success",
      title: updateAllTitle(tally),
      message:
        [
          updateAllFailureMessage(tally),
          conflictNote,
          skippedNames.size > 0 ? `Skipped ${[...skippedNames].join(", ")}` : null,
        ]
          .filter(Boolean)
          .join(". ") || undefined,
    });
  };

  const stop = async () => {
    try {
      await stopUpdateAll();
    } catch (error) {
      addToast({
        type: "error",
        title: "Could not stop Update all",
        message: error instanceof Error ? error.message : "Unknown error",
      });
    }
  };

  const start = async () => {
    setIsCheckingEdits(true);
    const targets = [...updates];
    // `skillsWithLocalEdits` treats a failed check as "no edits", so it never rejects.
    const edited = await skillsWithLocalEdits(targets, skillLocalEdits);
    setIsCheckingEdits(false);
    if (edited.length > 0) {
      setCheckedUpdates(targets);
      setEditedSkills(edited);
      return;
    }
    await runUpdateAll(targets);
  };

  const closeEditsDialog = () => setEditedSkills([]);
  const editedChoices = partitionForkableSkills(editedSkills);
  const forkNames = editedChoices.forkable.map((skill) => skill.name);
  const unforkableNames = editedChoices.unforkable.map((skill) => skill.name);

  /**
   * Runs the confirmed batch on what is true now: only the checked skills that
   * still have an update, and only if no other one gained edits while the
   * dialog was open. A skill with edits the user did not approve asks again.
   */
  const confirm = async (choice: { fork?: ReadonlySet<string>; skip?: ReadonlySet<string> }) => {
    closeEditsDialog();
    const approved = new Set(editedSkills.map((skill) => skill.name));
    const latest = new Map(updates.map((skill) => [skill.name, skill]));
    const targets = checkedUpdates.flatMap((skill) => latest.get(skill.name) ?? []);
    setIsCheckingEdits(true);
    const edited = await skillsWithLocalEdits(targets, skillLocalEdits);
    setIsCheckingEdits(false);
    if (edited.some((skill) => !approved.has(skill.name))) {
      setCheckedUpdates(targets);
      setEditedSkills(edited);
      return;
    }
    await runUpdateAll(targets, choice.fork, choice.skip);
  };

  return {
    activeBatch,
    isCheckingEdits,
    start,
    stop,
    /** The "Update will replace your edits" choice, open while `skillNames` is not empty. */
    editsPrompt: {
      skillNames: editedSkills.map((skill) => skill.name),
      canFork: forkNames.length > 0,
      unforkableSkillNames: unforkableNames,
      fork: (choice: "skip" | "overwrite") =>
        void confirm({
          fork: new Set(forkNames),
          skip: choice === "skip" ? new Set(unforkableNames) : undefined,
        }),
      overwrite: () => void confirm({}),
      cancel: closeEditsDialog,
    },
  };
}

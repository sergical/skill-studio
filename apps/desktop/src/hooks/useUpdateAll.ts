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
  editedUpdateCopies,
  partitionForkableSkills,
  updateCopyKeys,
  withUpdateCopies,
} from "../lib/skill-lifecycle-target";
import { useAppStore } from "../store/appStore";
import { startUpdateAll, stopUpdateAll, useActiveUpdateAll } from "./skillBatchUpdates";
import { updateAllFailureMessage, updateAllTitle } from "./update-all";

/** `updates` are the skills that still have an update: what Update all runs on. */
export function useUpdateAll(updates: readonly InstalledSkill[]) {
  const addToast = useAppStore((state) => state.addToast);
  /**
   * The batch the edits dialog is asking about, fixed when it opened: `copies`
   * are the update copies checked for edits (by `updateCopyKey`), `edited` the
   * ones the dialog names. A confirm runs on no copy outside `copies`.
   */
  const [prompt, setPrompt] = useState<{
    copies: ReadonlySet<string>;
    edited: ReadonlySet<string>;
    skills: InstalledSkill[];
  } | null>(null);
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

  /** Checks `batch` copy by copy and runs it, or opens the dialog on the copies that have edits. */
  const checkAndRun = async (
    batch: InstalledSkill[],
    approved: ReadonlySet<string>,
    choice: { fork?: ReadonlySet<string>; skip?: ReadonlySet<string> } = {},
  ) => {
    setIsCheckingEdits(true);
    // `editedUpdateCopies` treats a failed check as "no edits", so it never rejects.
    const edited = await editedUpdateCopies(batch, skillLocalEdits);
    setIsCheckingEdits(false);
    if ([...edited].some((key) => !approved.has(key))) {
      setPrompt({
        copies: updateCopyKeys(batch),
        edited,
        skills: batch.filter((skill) =>
          [...updateCopyKeys([skill])].some((key) => edited.has(key)),
        ),
      });
      return;
    }
    await runUpdateAll(batch, choice.fork, choice.skip);
  };

  const start = () => checkAndRun([...updates], new Set());

  const closeEditsDialog = () => setPrompt(null);
  const editedSkills = prompt?.skills ?? [];
  const editedChoices = partitionForkableSkills(editedSkills);
  const forkNames = editedChoices.forkable.map((skill) => skill.name);
  const unforkableNames = editedChoices.unforkable.map((skill) => skill.name);

  /**
   * Runs the confirmed batch on what is true now: only the checked copies that
   * still have an update, checked for edits again. A copy with edits the user
   * did not approve keeps the whole batch back and the dialog asks again.
   */
  const confirm = async (choice: { fork?: ReadonlySet<string>; skip?: ReadonlySet<string> }) => {
    if (!prompt) return;
    closeEditsDialog();
    await checkAndRun(withUpdateCopies(updates, prompt.copies), prompt.edited, choice);
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

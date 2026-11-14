// ============================================================================
// useSkillBulkActions - Runs one bulk action from the selection bar: park,
// unpark and invocation go to the backend as one batched call, remove goes
// skill by skill, and one toast reports the outcome. Each row it touches holds
// its place, spins while the action runs and then shows the past tense, as a
// single-row fix does; see `rowActs`. Park and unpark toast with an Undo. The
// selection stays only when a skill failed, so the user can retry.
// ============================================================================

import { useState } from "react";
import type { InstalledSkill } from "@skill-studio/lib";
import { removeSkill, skillLocalEdits } from "../../lib/skill-api";
import { runListUpdate } from "../../hooks/skillBatchUpdates";
import { partitionForkableSkills, skillsWithLocalEdits } from "../../lib/skill-lifecycle-target";
import { useAppStore } from "../../store/appStore";
import type { UpdatePrompt } from "../SkillDetail/UpdateOverwritesEditsDialog";
import {
  bulkActionToast,
  bulkDisabledReason,
  bulkProgressLabel,
  bulkRowPastTense,
  bulkUpdateProgressLabel,
  bulkRemovalTargets,
  planBulkAction,
  runBulkSequentially,
  withoutRunningRows,
} from "./skill-bulk-actions";
import type { BulkAction, BulkPlan, BulkRunResult } from "./skill-bulk-actions";
import { runBatchAction } from "./skill-bulk-run";

async function runRemoval(skill: InstalledSkill): Promise<void> {
  const targets = bulkRemovalTargets(skill);
  if (targets.length === 0) throw new Error("No removable copy.");
  for (const target of targets) {
    // react-doctor-disable-next-line react-doctor/async-await-in-loop -- each core op takes an exclusive lease, so the calls must not overlap
    await removeSkill(target);
  }
}

interface UseSkillBulkActions {
  /** The kind of the action running, otherwise `null`. */
  running: BulkAction["kind"] | null;
  /** "Parking 5 skills…" while an action runs, for screen readers; otherwise `null`. */
  progress: string | null;
  run: (action: BulkAction, skills: InstalledSkill[]) => Promise<void>;
  /** Confirm step before an update replaces local edits; `null` while none is pending. */
  updatePrompt: UpdatePrompt | null;
}

type ParkAction = Extract<BulkAction, { kind: "park" | "unpark" }>;

interface PendingEditedUpdate {
  plan: BulkPlan;
  edited: InstalledSkill[];
}

/** `onFinished(hadFailures)` lets the caller clear the selection unless something failed. */
export function useSkillBulkActions(
  onFinished: (hadFailures: boolean) => void,
): UseSkillBulkActions {
  const addToast = useAppStore((state) => state.addToast);
  const [progress, setProgress] = useState<string | null>(null);

  const [pending, setPending] = useState<PendingEditedUpdate | null>(null);

  const [running, setRunning] = useState<BulkAction["kind"] | null>(null);

  const undoPark = (action: ParkAction, skills: InstalledSkill[]) => {
    const reverse: ParkAction = { kind: action.kind === "park" ? "unpark" : "park" };
    const store = useAppStore.getState();
    const held = new Set(store.holdSkills(skills.map((skill) => skill.name)));
    const free = skills.filter((skill) => held.has(skill.name));
    if (free.length === 0) return;
    void runBatchAction(reverse, free)
      .catch((error) => ({
        succeeded: [],
        failed: free.map((skill) => ({
          skill,
          error: error instanceof Error ? error.message : "Unknown error",
        })),
      }))
      .then((result) => {
        // Undone, a skill is back where it was, so its row has nothing left to say.
        store.releaseSkills(result.succeeded.map((skill) => skill.name));
        store.settleSkills(
          result.failed.map(({ skill }) => skill.name),
          bulkRowPastTense(action),
        );
        if (result.failed.length > 0)
          addToast(bulkActionToast(reverse, { applicable: free, skipped: [] }, result));
      });
  };

  const execute = async (
    action: BulkAction,
    requested: BulkPlan,
    forkNames: ReadonlySet<string> = new Set(),
  ) => {
    const store = useAppStore.getState();
    const names = store.holdSkills(requested.applicable.map((skill) => skill.name));
    const plan = withoutRunningRows(requested, new Set(names));
    const nothingToRun = bulkDisabledReason(action, plan);
    if (nothingToRun) {
      addToast({ type: "info", title: nothingToRun });
      return;
    }
    setRunning(action.kind);
    setProgress(bulkProgressLabel(action, 1, plan.applicable.length));
    let result: BulkRunResult;
    try {
      if (action.kind === "update")
        result = await runListUpdate(plan.applicable, forkNames, (done, total) =>
          setProgress(bulkUpdateProgressLabel(done, total)),
        );
      else if (action.kind === "remove")
        result = await runBulkSequentially(plan.applicable, runRemoval, (current, total) =>
          setProgress(bulkProgressLabel(action, current, total)),
        );
      else result = await runBatchAction(action, plan.applicable);
    } catch (error) {
      // A batched call can reject as a whole; every skill in it failed.
      const message = error instanceof Error ? error.message : "Unknown error";
      result = {
        succeeded: [],
        failed: plan.applicable.map((skill) => ({ skill, error: message })),
      };
    }
    // A removed skill has no row left to hold; it leaves the list as the user confirmed.
    if (action.kind === "remove") store.releaseSkills(names);
    else {
      store.settleSkills(
        result.succeeded.map((skill) => skill.name),
        bulkRowPastTense(action),
      );
      store.releaseSkills(result.failed.map(({ skill }) => skill.name));
    }
    setRunning(null);
    setProgress(null);
    const toast = bulkActionToast(action, plan, result);
    const undoable = result.succeeded;
    addToast(
      (action.kind === "park" || action.kind === "unpark") && undoable.length > 0
        ? { ...toast, action: { label: "Undo", onClick: () => undoPark(action, undoable) } }
        : toast,
    );
    onFinished(result.failed.length > 0);
  };

  const run = async (action: BulkAction, skills: InstalledSkill[]) => {
    const plan = planBulkAction(skills, action);
    if (plan.applicable.length === 0) return;
    if (action.kind === "update") {
      setRunning(action.kind);
      // `skillsWithLocalEdits` treats a failed check as "no edits", so it never rejects.
      const edited = await skillsWithLocalEdits(plan.applicable, skillLocalEdits);
      if (edited.length > 0) {
        setRunning(null);
        setPending({ plan, edited });
        return;
      }
    }
    await execute(action, plan);
  };

  const updateAction: BulkAction = { kind: "update" };
  const editedChoices = partitionForkableSkills(pending?.edited ?? []);
  const updatePrompt: UpdatePrompt | null = pending && {
    skillNames: pending.edited.map((skill) => skill.name),
    canFork: editedChoices.forkable.length > 0,
    unforkableSkillNames: editedChoices.unforkable.map((skill) => skill.name),
    fork: (unforkable = "overwrite") => {
      setPending(null);
      const forkNames = new Set(editedChoices.forkable.map((skill) => skill.name));
      const skipped = unforkable === "skip" ? editedChoices.unforkable : [];
      const skippedSkills = new Set(skipped);
      const plan: BulkPlan = {
        applicable: pending.plan.applicable.filter((skill) => !skippedSkills.has(skill)),
        skipped: [
          ...pending.plan.skipped,
          ...skipped.map((skill) => ({ skill, reason: "edited copy skipped" })),
        ],
      };
      void execute(updateAction, plan, forkNames);
    },
    overwrite: () => {
      setPending(null);
      void execute(updateAction, pending.plan);
    },
    cancel: () => setPending(null),
  };

  return { running, progress, run, updatePrompt };
}

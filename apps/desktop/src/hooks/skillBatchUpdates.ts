// ============================================================================
// skillBatchUpdates - the batch update commands, wired to the IPC layer. Both
// callers run `skillsWithLocalEdits` and the edits dialog first.
// ============================================================================

import { useSyncExternalStore } from "react";
import type { PluginUpdateResult } from "../lib/skill-api";
import {
  cancelUpdateAll,
  forkSkill,
  pullForkUpstream,
  updateAllSkillsWithProgress,
  updatePlugin,
} from "../lib/skill-api";
import { setUpdateAllRunning } from "../lib/skill-busy-message";
import type { PluginUpdateTarget } from "../lib/skill-lifecycle-target";
import { updateAllOutdatedSkills } from "../components/Home/home-inbox-data";
import { runBulkUpdate } from "../components/SkillList/skill-bulk-actions";
import type { InstalledSkill } from "@skill-studio/lib";

/** Runs `claude plugin update` for one install and resolves to the CLI's outcome and message. */
export function updatePluginInstall(target: PluginUpdateTarget): Promise<PluginUpdateResult> {
  return updatePlugin(target.plugin_id, "Claude Code", target.scope, target.project_path);
}

function pluginBatchUpdater() {
  const refreshed = new Set<string>();
  return async (target: PluginUpdateTarget): Promise<PluginUpdateResult> => {
    const marketplace = target.plugin_id.slice(target.plugin_id.lastIndexOf("@") + 1);
    const key = JSON.stringify([marketplace, target.project_path]);
    const result = await updatePlugin(
      target.plugin_id,
      "Claude Code",
      target.scope,
      target.project_path,
      refreshed.has(key),
    );
    if (result.marketplace_refreshed) refreshed.add(key);
    return result;
  };
}

/** Stop request for one `runHomeUpdateAll` batch. */
export interface UpdateAllControl {
  /** Names this batch to the backend, so a Cancel that lands before the batch starts still reaches it. */
  batchId: string;
  /**
   * The latest Cancel: `null` before any, then a promise that resolves `true`
   * once the backend has it or `false` if it never got there. A phase boundary
   * waits for it, so a Cancel still in flight neither stops nor clears the batch.
   */
  stopRequest: Promise<boolean> | null;
}

export function newUpdateAllControl(): UpdateAllControl {
  return { batchId: crypto.randomUUID(), stopRequest: null };
}

/** Stops the batch after the step it is on, in the frontend phases and in the backend's own loop. */
export function requestUpdateAllStop(control: UpdateAllControl): Promise<void> {
  const sent = cancelUpdateAll(control.batchId);
  control.stopRequest = sent.then(
    () => true,
    () => false,
  );
  return sent;
}

/** Home's "Update all"; `forkNames` are the edited skills to fork and merge instead of overwrite. `onProgress`'s third argument names the skill that starts next. */
export async function runHomeUpdateAll(
  updates: InstalledSkill[],
  onProgress: (done: number, total: number, current: string | null) => void,
  forkNames?: ReadonlySet<string>,
  control?: UpdateAllControl,
) {
  setUpdateAllRunning(true);
  try {
    return await updateAllOutdatedSkills(
      updates,
      pullForkUpstream,
      (targets, onOwnerDone) =>
        updateAllSkillsWithProgress(
          targets,
          ({ done, skill_name }) => onOwnerDone(done, skill_name),
          control?.batchId,
        ),
      onProgress,
      forkNames && { names: forkNames, fork: forkSkill },
      pluginBatchUpdater(),
      control && (() => control.stopRequest ?? false),
    );
  } finally {
    setUpdateAllRunning(false);
  }
}

/** The Home "Update all" batch that is running, kept here so it outlives the component that started it. */
export interface ActiveUpdateAll {
  control: UpdateAllControl;
  progress: { done: number; total: number; current: string | null };
  isStopping: boolean;
}

let activeBatch: ActiveUpdateAll | null = null;
const batchListeners = new Set<() => void>();

function setActiveBatch(next: ActiveUpdateAll | null) {
  activeBatch = next;
  batchListeners.forEach((listener) => listener());
}

export function getActiveUpdateAll(): ActiveUpdateAll | null {
  return activeBatch;
}

function subscribeActiveUpdateAll(listener: () => void) {
  batchListeners.add(listener);
  return () => void batchListeners.delete(listener);
}

/** The running batch, or null; a remounted Home group reads it again and shows Cancel. */
export function useActiveUpdateAll(): ActiveUpdateAll | null {
  return useSyncExternalStore(subscribeActiveUpdateAll, getActiveUpdateAll);
}

/**
 * Starts Home's "Update all" unless one is already running; resolves to the
 * tally, or null when it refused. Two batches would write the same skills, and
 * the second one's id could not stop the first.
 */
export async function startHomeUpdateAll(
  updates: InstalledSkill[],
  forkNames?: ReadonlySet<string>,
) {
  if (activeBatch) return null;
  const control = newUpdateAllControl();
  setActiveBatch({ control, progress: { done: 0, total: 0, current: null }, isStopping: false });
  try {
    return await runHomeUpdateAll(
      updates,
      (done, total, current) =>
        activeBatch?.control === control &&
        setActiveBatch({ ...activeBatch, progress: { done, total, current } }),
      forkNames,
      control,
    );
  } finally {
    setActiveBatch(null);
  }
}

/** Stops the running batch, if any; the same call works from a component mounted after it started. */
export async function stopHomeUpdateAll(): Promise<void> {
  const batch = activeBatch;
  if (!batch) return;
  setActiveBatch({ ...batch, isStopping: true });
  try {
    await requestUpdateAllStop(batch.control);
  } catch (error) {
    // Cancel did not reach the backend, so the whole batch keeps running and
    // offers Cancel again.
    if (activeBatch?.control === batch.control)
      setActiveBatch({ ...activeBatch, isStopping: false });
    throw error;
  }
}

/** The list's bulk Update; `forkNames` as for `runHomeUpdateAll`. */
export function runListUpdate(
  skills: InstalledSkill[],
  forkNames: ReadonlySet<string>,
  onProgress: (done: number, total: number) => void,
) {
  return runBulkUpdate(
    skills,
    forkNames,
    {
      fork: forkSkill,
      pullFork: pullForkUpstream,
      updateAll: (targets, onUpdateProgress) =>
        updateAllSkillsWithProgress(targets, ({ done, total }) => onUpdateProgress(done, total)),
      updatePluginInstall: pluginBatchUpdater(),
    },
    onProgress,
  );
}

// ============================================================================
// skillBatchUpdates - the batch update commands, wired to the IPC layer. Both
// callers run `skillsWithLocalEdits` and the edits dialog first.
// ============================================================================

import type { PluginUpdateResult } from "../lib/skill-api";
import {
  forkSkill,
  pullForkUpstream,
  updateAllSkillsWithProgress,
  updatePlugin,
} from "../lib/skill-api";
import type { PluginUpdateTarget } from "../lib/skill-lifecycle-target";
import { updateAllOutdatedSkills } from "../components/Home/home-inbox-data";
import { runBulkUpdate } from "../components/SkillList/skill-bulk-actions";
import type { InstalledSkill } from "@skill-studio/lib";

/** Runs `claude plugin update` for one install and resolves to the CLI's outcome and message. */
export function updatePluginInstall(target: PluginUpdateTarget): Promise<PluginUpdateResult> {
  return updatePlugin(target.plugin_id, "Claude Code", target.scope, target.project_path);
}

/** Home's "Update all"; `forkNames` are the edited skills to fork and merge instead of overwrite. `onProgress`'s third argument names the skill that starts next. */
export function runHomeUpdateAll(
  updates: InstalledSkill[],
  onProgress: (done: number, total: number, current: string | null) => void,
  forkNames?: ReadonlySet<string>,
) {
  return updateAllOutdatedSkills(
    updates,
    pullForkUpstream,
    (targets, onOwnerDone) => updateAllSkillsWithProgress(targets, ({ done }) => onOwnerDone(done)),
    onProgress,
    forkNames && { names: forkNames, fork: forkSkill },
    updatePluginInstall,
  );
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
      updatePluginInstall,
    },
    onProgress,
  );
}

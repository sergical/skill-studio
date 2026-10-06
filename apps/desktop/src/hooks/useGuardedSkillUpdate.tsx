// ============================================================================
// useGuardedSkillUpdate - the one entry point for updating a single skill.
// Every single-skill Update button goes through `requestUpdate`, so an update
// that would replace local edits to a skills.sh skill always asks first:
// fork and merge, overwrite, or cancel.
// ============================================================================

import { useRef, useState } from "react";
import { forkSkill, pullForkUpstream, skillLocalEdits, updateSkill } from "../lib/skill-api";
import { updatePluginInstall } from "./skillBatchUpdates";
import {
  forkEditedAndUpdate,
  forkableDeployment,
  lifecycleTargetForPark,
  pluginFailureToast,
  pullForkAndUpdatePlugins,
  pullUpstreamToast,
  skillUpdateToast,
  skillsWithLocalEdits,
  updateSkillOwners,
} from "../lib/skill-lifecycle-target";
import type { InstalledSkill, LifecycleTarget } from "@skill-studio/lib";
import { useAppStore } from "../store/appStore";
import { UpdateOverwritesEditsDialog } from "../components/SkillDetail/UpdateOverwritesEditsDialog";

function canForkPending({ skill, scopeTarget }: PendingUpdate): boolean {
  const deployment = forkableDeployment(skill);
  if (!deployment) return false;
  return !scopeTarget || scopeTarget.owner_id === deployment.owner_id;
}

interface PendingUpdate {
  skill: InstalledSkill;
  overwrite: () => Promise<void>;
  /** Set when the update covers one owner only; the fork then replaces that owner alone. */
  scopeTarget?: LifecycleTarget;
  /** The request's options, so a fork-and-update reports the same way an overwrite does. */
  options: UpdateRequestOptions;
}

/** How a single-owner update ended, for callers that report it their own way. */
export interface UpdateFinish {
  success: boolean;
  error?: string;
}

interface UpdateRequestOptions {
  /** Update this one owner instead of every owner of the skill. */
  scopeTarget?: LifecycleTarget;
  /**
   * Called with the outcome of the update, after the dialog (when one opened) was confirmed;
   * never called when the user cancels. For a `scopeTarget` update the hook then leaves the
   * reporting to the caller; otherwise it toasts first.
   */
  onFinished?: (finish: UpdateFinish) => void | Promise<void>;
  /** Leave the skill's plugin installs alone; the caller updates them after `onFinished`. */
  skipPlugins?: boolean;
}

/**
 * `requestUpdate(skill, options)` updates the skill right away when it has no
 * local edits, and otherwise opens the dialog. The hook owns the update
 * commands, so no caller can reach them without the check. Render `dialog`
 * once next to the buttons.
 */
export function useGuardedSkillUpdate() {
  const addToast = useAppStore((state) => state.addToast);
  const [pending, setPending] = useState<PendingUpdate | null>(null);
  const [resolvingSkills, setResolvingSkills] = useState<ReadonlySet<string>>(new Set());
  const updating = useRef(new Set<string>());

  const overwriteFor =
    (skill: InstalledSkill, { scopeTarget, onFinished, skipPlugins }: UpdateRequestOptions) =>
    async () => {
      if (scopeTarget) {
        let finish: UpdateFinish;
        try {
          const result = await updateSkill(scopeTarget);
          finish = {
            success: result.success,
            error: result.error ?? "Update command failed without an error message.",
          };
        } catch (error) {
          finish = {
            success: false,
            error:
              error instanceof Error ? error.message : "Update failed without an error message.",
          };
        }
        await onFinished?.(finish);
        return;
      }
      const summary = await updateSkillOwners(
        skill,
        updateSkill,
        skipPlugins ? undefined : updatePluginInstall,
      );
      addToast(skillUpdateToast(skill.name, summary));
      await onFinished?.({
        success: summary.failures.length === 0,
        error: summary.failures.map((failure) => failure.message).join("; "),
      });
    };

  const requestUpdate = async (skill: InstalledSkill, options: UpdateRequestOptions = {}) => {
    const { scopeTarget } = options;
    const edited = await skillsWithLocalEdits(
      [skill],
      skillLocalEdits,
      scopeTarget ? () => [scopeTarget] : undefined,
    );
    const overwrite = overwriteFor(skill, options);
    if (edited.length > 0) {
      setPending({ skill, overwrite, scopeTarget, options });
      return;
    }
    await overwrite();
  };

  const resolve = async (title: string, run: (update: PendingUpdate) => Promise<void>) => {
    const update = pending;
    setPending(null);
    if (!update) return;
    setResolvingSkills((names) => new Set(names).add(update.skill.name));
    updating.current.add(update.skill.name);
    try {
      await run(update);
    } catch (error) {
      addToast({
        type: "error",
        title,
        message: error instanceof Error ? error.message : "Unknown error",
      });
    }
    updating.current.delete(update.skill.name);
    setResolvingSkills((names) => {
      const next = new Set(names);
      next.delete(update.skill.name);
      return next;
    });
  };

  const overwrite = () => resolve("Update failed", (update) => update.overwrite());

  const forkAndUpdate = () =>
    resolve("Fork and update failed", async ({ skill, scopeTarget, options }) => {
      const { pull, others, plugins } = await forkEditedAndUpdate(
        skill,
        {
          fork: forkSkill,
          pullFork: pullForkUpstream,
          updateOwner: updateSkill,
          updatePluginInstall: options.skipPlugins ? undefined : updatePluginInstall,
        },
        { updateOthers: scopeTarget === undefined },
      );
      addToast(pullUpstreamToast(pull));
      if (others.attempted > 0) addToast(skillUpdateToast(skill.name, others));
      for (const failure of plugins?.failures ?? []) {
        addToast(pluginFailureToast(failure));
      }
      const failures = [...others.failures, ...(plugins?.failures ?? [])];
      await options.onFinished?.({
        success: failures.length === 0,
        error: failures.map((failure) => failure.message).join("; "),
      });
    });

  const dialog = (
    <UpdateOverwritesEditsDialog
      skillNames={pending ? [pending.skill.name] : []}
      isBulk={false}
      canFork={pending !== null && canForkPending(pending)}
      onFork={() => void forkAndUpdate()}
      onOverwrite={() => void overwrite()}
      onCancel={() => setPending(null)}
    />
  );

  /** "Pull latest" for one row: a fork merges upstream, any other skill takes the guarded update.
   * Reports the outcome as a toast and never throws, so a row menu can fire and forget. A second
   * pull of the same skill while one runs, here or from the dialog, is dropped: the menu closes at
   * once, so a user who sees nothing happen picks it again, and two `npx skills update` runs on one
   * folder race. */
  const pullLatest = async (skill: InstalledSkill) => {
    if (updating.current.has(skill.name)) return;
    updating.current.add(skill.name);
    try {
      if (skill.source_kind === "fork") {
        // The fork's pull and its plugin installs are separate updates: a failed pull must not
        // hold the plugins back, and a fork whose only update is a plugin has nothing to pull.
        await pullForkAndUpdatePlugins(
          skill,
          () => pullForkUpstream(lifecycleTargetForPark(skill)),
          addToast,
          updatePluginInstall,
        );
      } else {
        await requestUpdate(skill);
      }
    } catch (error) {
      // An `if`, not a conditional expression: the React Compiler can't compile a value block
      // directly inside a try/catch statement.
      let message = "Unknown error";
      if (error instanceof Error) message = error.message;
      addToast({ type: "error", title: "Update failed", message });
    }
    updating.current.delete(skill.name);
  };

  return {
    requestUpdate,
    pullLatest,
    isResolving: resolvingSkills.size > 0,
    /** The skills whose confirmed update is running, which `pullLatest` has already returned from. */
    resolvingSkills,
    dialog,
  };
}

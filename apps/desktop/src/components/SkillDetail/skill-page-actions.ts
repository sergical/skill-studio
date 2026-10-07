// ============================================================================
// useSkillPageActions - Every action the skill page's header offers (primary
// button, overflow menu, and the park/fork/remove flows that used to live in
// InstalledSkillHeader's icon row and SkillDetailActions): reveal, open in
// editor, copy path, park/unpark, fork/un-fork/pull upstream, update, and
// remove. One hook so the header stays a thin render of this state.
// ============================================================================

import { useState } from "react";
import type { ReactNode } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  forkSkill,
  getSkillSnapshot,
  onSkillSnapshot,
  openSkillPath,
  parkCheck,
  parkSkills,
  pullForkUpstream,
  removeSkill,
  requestSkillRescan,
  unforkSkill,
  unparkSkills,
  updatePlugin,
} from "../../lib/skill-api";
import {
  isLiveCopy,
  lifecycleTargetForDeployment,
  lifecycleTargetForSkill,
  parkEveryAgentPlan,
  skillRemovalBlockedReason,
  skillRemovalChoices,
  pullForkAndUpdatePlugins,
  skillHasManagedUpdate,
  skillRemovalEmptiesSkill,
  skillUpdateOwnerTargets,
  updateSkillPluginsWithToasts,
} from "../../lib/skill-lifecycle-target";
import type { PluginInstallUpdater, SkillRemovalChoice } from "../../lib/skill-lifecycle-target";
import { deploymentLabelFromAgentId, homeRelativePath } from "@skill-studio/lib";
import type {
  Deployment,
  InstalledSkill,
  LifecycleTarget,
  ParkCheck,
  SkillSnapshot,
  Toast,
} from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { useGuardedSkillUpdate } from "../../hooks/useGuardedSkillUpdate";
import { gitWarningText } from "./skill-location-helpers";
import type { UpdateFinish } from "../../hooks/useGuardedSkillUpdate";

/**
 * The one deployment `forkSkill` will accept: the shared-folder copy at
 * `~/.agents/skills/<name>`, or the Claude Code whole-dir symlink to it.
 * `undefined` when neither is deployed, in which case Fork has nothing
 * forkable to point at.
 */
function sharedFolderDeployment(skill: InstalledSkill) {
  return skill.deployments.find(
    (d) => d.path.includes("/.agents/skills/") || d.path.includes("/.claude/skills/"),
  );
}

type AddToast = ReturnType<typeof useAppStore.getState>["addToast"];

/**
 * The header's update button: a fork pulls upstream, any other kind with an update runs the update.
 * Any kind, not only dotagents/skills-sh: a skill with a plugin copy reports `plugin` as its kind
 * while its skills.sh copy still has an update, and the header is the page's only Update.
 */
/**
 * The header Update: managed copies first, through the overwrite guard, then the
 * skill's plugin installs. Plugins wait for the guard's `onFinished`, so a
 * cancelled overwrite dialog or a failed copy update leaves them alone.
 */
export async function runHeaderUpdate<
  S extends Pick<InstalledSkill, "deployments" | "update_owner_ids" | "update_owners">,
>(
  skill: S,
  guard: {
    requestUpdate: (
      skill: S,
      options: { skipPlugins: boolean; onFinished: (finish: UpdateFinish) => Promise<void> },
    ) => Promise<void>;
  },
  addToast: (toast: Omit<Toast, "id">) => void,
  updatePluginInstall: PluginInstallUpdater,
): Promise<void> {
  if (skillUpdateOwnerTargets(skill).length === 0) {
    await updateSkillPluginsWithToasts(skill, addToast, updatePluginInstall);
    return;
  }
  await guard.requestUpdate(skill, {
    skipPlugins: true,
    onFinished: async ({ success }) => {
      if (success) await updateSkillPluginsWithToasts(skill, addToast, updatePluginInstall);
    },
  });
}

/** "Pull latest" only when the fork itself is outdated; a fork whose only update is a plugin takes "Update". */
export function headerUpdateLabel(
  skill: Pick<InstalledSkill, "source_kind" | "update_owner_ids">,
): "Pull latest" | "Update" | null {
  if (skill.update_owner_ids.length === 0) return null;
  return skill.source_kind === "fork" && skillHasManagedUpdate(skill) ? "Pull latest" : "Update";
}

/**
 * The header Remove button's success toast: "Removed" plus the skill name,
 * not "Updated N deployments" (unit 3.9b) - `removeSkill` takes exactly one
 * deployment off disk, so a deployment count has nothing to count.
 */
export function removeSuccessToast(skillName: string): Omit<Toast, "id"> {
  return { type: "success", title: "Removed", message: skillName };
}

type GitCheck = (target: LifecycleTarget) => Promise<Pick<ParkCheck, "git_tracked">>;

interface ParkForEveryAgentApi {
  parkSkills: typeof parkSkills;
  unparkSkills: typeof unparkSkills;
  parkCheck: GitCheck;
  ask: (
    message: string,
    options: { title: string; kind: "warning"; okLabel: string },
  ) => Promise<boolean>;
  /** The first snapshot built after the call, or `null` when none lands in time. */
  rescan: () => Promise<RescanResult | null>;
}

type RescanResult = Pick<SkillSnapshot, "skills" | "scan_partial" | "unread_roots">;

const RESCAN_TIMEOUT_MS = 10_000;

async function rescanAfterWrite(): Promise<RescanResult | null> {
  const before = (await getSkillSnapshot())?.revision ?? 0;
  return new Promise((resolve) => {
    let settled = false;
    let unlisten: (() => void) | undefined;
    const finish = (snapshot: RescanResult | null) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      unlisten?.();
      resolve(snapshot);
    };
    const timer = setTimeout(() => finish(null), RESCAN_TIMEOUT_MS);
    onSkillSnapshot((snapshot) => {
      if (snapshot.revision > before) finish(snapshot);
    }).then(
      (stop) => {
        unlisten = stop;
        if (settled) stop();
        else void requestSkillRescan().catch(() => finish(null));
      },
      () => finish(null),
    );
  });
}

const PARK_FOR_EVERY_AGENT_API: ParkForEveryAgentApi = {
  parkSkills,
  unparkSkills,
  parkCheck,
  ask,
  rescan: rescanAfterWrite,
};

/** The confirm text for parking project folders, with the git warning for each tracked one. */
async function projectParkMessage(folders: Deployment[], check: GitCheck): Promise<string> {
  const lines = await Promise.all(
    folders.map(async (folder) => {
      const gitTracked = await check({ deployment_id: folder.id }).then(
        (answer) => answer.git_tracked,
        () => null,
      );
      const warning = gitWarningText(gitTracked, "parking");
      return warning
        ? `${homeRelativePath(folder.path)}\n${warning}`
        : homeRelativePath(folder.path);
    }),
  );
  return `This moves these project folders into ~/.agents/skills-parked:\n\n${lines.join("\n\n")}`;
}

function stillOnMessage(stillOn: Deployment[]): string | null {
  if (stillOn.length === 0) return null;
  const paths = stillOn.map((deployment) => homeRelativePath(deployment.path)).join(", ");
  const pluginHint = stillOn.some((deployment) => deployment.plugin)
    ? " Turn a plugin's copy off with /plugin in its agent."
    : "";
  return `Still on: ${paths}.${pluginHint}`;
}

const UNCONFIRMED = "Skill Studio couldn't rescan to confirm which agents still load it.";

/**
 * The agents that still skip a restored copy: its own agent when `disabled_by`
 * is set, and for a shared copy each reader whose own setting hides it.
 */
function agentsStillOff(deployment: Deployment): string[] {
  const own = deployment.disabled_by === null ? [] : [deployment.agent];
  const readers = (deployment.disabled_readers ?? []).map(deploymentLabelFromAgentId);
  return [...own, ...readers];
}

/** Lists the copies an agent still has off in its own settings after Turn on. */
function stillOffMessage(fresh: Deployment[]): string | null {
  const off = fresh.flatMap((deployment) => {
    const agents = deployment.scope === "parked" ? [] : agentsStillOff(deployment);
    return agents.length === 0
      ? []
      : [`${homeRelativePath(deployment.path)} (${agents.join(", ")})`];
  });
  if (off.length === 0) return null;
  return `Still off in agent settings: ${off.join(", ")}. The skill page shows where to turn it on.`;
}

function isUnder(path: string, root: string): boolean {
  return path === root || path.startsWith(root.endsWith("/") ? root : `${root}/`);
}

/**
 * The skill's copies from a rescan that read every place they live, or `null`
 * when the scan was cut short or could not read one of them: a partial scan
 * carries old rows forward and can leave the skill out.
 */
function completeRescanOf(
  skill: InstalledSkill,
  snapshot: RescanResult | null,
): Deployment[] | null {
  if (!snapshot || snapshot.scan_partial) return null;
  const fresh = snapshot.skills.find((candidate) => candidate.name === skill.name)?.deployments;
  if (!fresh) return null;
  const paths = [...skill.deployments, ...fresh].map((deployment) => deployment.path);
  const unread = snapshot.unread_roots.some((root) => paths.some((path) => isUnder(path, root)));
  return unread ? null : fresh;
}

/**
 * Reads the skill back from a rescan after the write and names anything that
 * did not change: copies still on after Park, copies still off after Turn on.
 * Without a complete rescan the result is unknown, so it says so instead of
 * reporting success. Only plugin copies are certain without one: Park never moves them.
 */
async function leftoverCheck(
  skill: InstalledSkill,
  rescan: ParkForEveryAgentApi["rescan"],
): Promise<{ message: string | null; confirmed: boolean }> {
  const fresh = completeRescanOf(skill, await rescan());
  if (!fresh) {
    const plugins = skill.parked
      ? null
      : stillOnMessage(
          skill.deployments.filter((deployment) => deployment.plugin && isLiveCopy(deployment)),
        );
    return { message: [plugins, UNCONFIRMED].filter(Boolean).join(" "), confirmed: false };
  }
  const message = skill.parked ? stillOffMessage(fresh) : stillOnMessage(fresh.filter(isLiveCopy));
  return { message, confirmed: true };
}

/**
 * Park every live folder of `skill`, or turn every parked one back on, in one
 * batch. Project folders wait for a confirm that carries the git warning.
 * Returns the toast to show, or `null` when the confirm was cancelled. The
 * toast is a warning when the core refused a folder or a copy stays on.
 */
export async function parkForEveryAgent(
  skill: InstalledSkill,
  api: ParkForEveryAgentApi = PARK_FOR_EVERY_AGENT_API,
): Promise<Omit<Toast, "id"> | null> {
  const plan = parkEveryAgentPlan(skill);
  if (!skill.parked && plan.projectFolders.length > 0) {
    const confirmed = await api.ask(await projectParkMessage(plan.projectFolders, api.parkCheck), {
      title: `Park ${skill.name} for every agent?`,
      kind: "warning",
      okLabel: "Park",
    });
    if (!confirmed) return null;
  }
  const results = skill.parked
    ? await api.unparkSkills(plan.targets)
    : await api.parkSkills(plan.targets);
  const errors = results.flatMap((result) => (result.error ? [result.error] : []));
  const done = skill.parked ? "Turned on" : "Parked";
  const leftover = await leftoverCheck(skill, api.rescan);
  if (errors.length > 0) {
    const total = plan.targets.length;
    return {
      type: "warning",
      title: `${done} ${total - errors.length} of ${total} copies of ${skill.name}`,
      message: [errors[0], leftover.message].filter(Boolean).join(" "),
    };
  }
  if (leftover.message) {
    const still = skill.parked ? "still off" : "still on";
    return {
      type: "warning",
      title: leftover.confirmed
        ? `${done} ${skill.name}, but a copy is ${still}`
        : `${done} ${skill.name}, not confirmed`,
      message: leftover.message,
    };
  }
  return { type: "success", title: `${done} ${skill.name}` };
}

/**
 * Runs `fn` with `setBusy` bracketing it, and reports a thrown error as an
 * error toast titled `errorTitle`. Every action below is this same shape.
 */
async function runAction(
  addToast: AddToast,
  setBusy: (busy: boolean) => void,
  errorTitle: string,
  fn: () => Promise<void>,
) {
  setBusy(true);
  try {
    await fn();
  } catch (err) {
    addToast({
      type: "error",
      title: errorTitle,
      message: err instanceof Error ? err.message : "Unknown error",
    });
  } finally {
    setBusy(false);
  }
}

export interface SkillPageAction {
  label: string;
  run: () => void;
  busy: boolean;
  /** Present for the primary button when it needs a non-default title. */
  title?: string;
}

export interface SkillPageActions {
  path: string | undefined;
  copied: boolean;
  reveal: () => void;
  openEditor: () => void;
  copyPath: () => void;
  /** The one primary action for the header - "Pull latest" or "Update" - `null` when there is none. */
  primaryAction: SkillPageAction | null;
  /** Park or turn on for every agent (the ⋯ menu; Locations switches act per agent) - `null` when the skill has no Global Universal folder to move. */
  parkAction: SkillPageAction | null;
  /** Fork (when forkable) or Un-fork (when already forked) - `null` when neither applies. */
  forkAction: SkillPageAction | null;
  /** One entry per scope with an exact mutable removal target, global first. */
  removeActions: (SkillPageAction & { key: string })[];
  /** Why there is no Remove, for a skill whose files the app must not delete. */
  removeBlockedReason: string | null;
  /** The "Update will replace your edits" dialog; render it once beside the header. */
  updateDialog: ReactNode;
}

/**
 * Consolidates every header/overflow-menu action for `skill` into one hook,
 * so `InstalledSkillHeader` only has to render menu items and one primary
 * button. No behaviour changes from the old `InstalledSkillHeader` icon row
 * and `SkillDetailActions`, aside from the update button always targeting
 * global scope (the page header has no scope picker) and Remove gaining the
 * same `ask()` confirm the other destructive actions here already use.
 *
 * `skill` is nullable so `SkillPage` can call this once, unconditionally,
 * even on the "no longer installed" render - every hook above still runs
 * every time, only the returned actions collapse to no-ops.
 */
export function useSkillPageActions(
  skill: InstalledSkill | null,
  onRemoveComplete: () => void,
): SkillPageActions {
  const addToast = useAppStore((state) => state.addToast);
  const [copied, setCopied] = useState(false);
  const [isParking, setIsParking] = useState(false);
  const [isForking, setIsForking] = useState(false);
  const [isPulling, setIsPulling] = useState(false);
  const [isUnforking, setIsUnforking] = useState(false);
  const [isUpdating, setIsUpdating] = useState(false);
  const [isRemoving, setIsRemoving] = useState(false);
  const guard = useGuardedSkillUpdate();

  if (!skill) {
    return {
      path: undefined,
      copied,
      reveal: () => undefined,
      openEditor: () => undefined,
      copyPath: () => undefined,
      primaryAction: null,
      parkAction: null,
      forkAction: null,
      removeActions: [],
      removeBlockedReason: null,
      updateDialog: null,
    };
  }

  const path = skill.deployments[0]?.path ?? skill.skill_path;

  const reveal = () => {
    if (!path) return;
    openSkillPath(path, "reveal").catch((err) => {
      addToast({
        type: "error",
        title: "Couldn't reveal in Finder",
        message: err instanceof Error ? err.message : "Unknown error",
      });
    });
  };

  const openEditor = () => {
    if (!path) return;
    openSkillPath(path, "editor").catch((err) => {
      addToast({
        type: "error",
        title: "Couldn't open in editor",
        message: err instanceof Error ? err.message : "Unknown error",
      });
    });
  };

  const copyPath = () => {
    if (!path) return;
    navigator.clipboard.writeText(path).then(() => {
      setCopied(true);
      addToast({ type: "success", title: "Copied path" });
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const togglePark = () =>
    runAction(
      addToast,
      setIsParking,
      skill.parked ? "Couldn't turn skill on" : "Couldn't park skill",
      async () => {
        const toast = await parkForEveryAgent(skill);
        if (toast) addToast(toast);
      },
    );

  const forkDeployment = sharedFolderDeployment(skill);

  const doFork = () => {
    if (!forkDeployment) return;
    return runAction(addToast, setIsForking, "Fork failed", async () => {
      await forkSkill(lifecycleTargetForDeployment(forkDeployment));
      addToast({ type: "success", title: "Forked", message: `${skill.name} is now yours to edit` });
    });
  };

  const doUnfork = async () => {
    const origin = skill.fork?.origin_source ?? "its origin";
    const confirmed = await ask(
      `Discard your changes and reinstall ${skill.name} from ${origin}?`,
      {
        title: "Un-fork skill",
        kind: "warning",
      },
    );
    if (!confirmed) return;
    await runAction(addToast, setIsUnforking, "Un-fork failed", async () => {
      await unforkSkill(lifecycleTargetForSkill(skill, "global"));
      addToast({
        type: "success",
        title: "Un-forked",
        message: `${skill.name} is reinstalled from ${origin}`,
      });
    });
  };

  const updatePluginInstall: PluginInstallUpdater = (target) =>
    updatePlugin(target.plugin_id, "Claude Code", target.scope, target.project_path);

  const doPullUpstream = () =>
    runAction(addToast, setIsPulling, "Pull upstream failed", () =>
      pullForkAndUpdatePlugins(
        skill,
        () => pullForkUpstream(lifecycleTargetForSkill(skill, "global")),
        addToast,
        updatePluginInstall,
      ),
    );

  const doUpdate = () =>
    runAction(addToast, setIsUpdating, "Update failed", () =>
      runHeaderUpdate(skill, guard, addToast, updatePluginInstall),
    );

  const doRemove = async (choice: SkillRemovalChoice) => {
    const confirmed = await ask(choice.confirmMessage, {
      title: choice.confirmTitle,
      kind: "warning",
      okLabel: choice.label,
    });
    if (!confirmed) return;
    await runAction(addToast, setIsRemoving, "Remove failed", async () => {
      await removeSkill(choice.preview.target);
      if (skillRemovalEmptiesSkill(skill, choice.selection)) onRemoveComplete();
      addToast(removeSuccessToast(skill.name));
    });
  };

  let primaryAction: SkillPageAction | null = null;
  const updateLabel = headerUpdateLabel(skill);
  if (updateLabel === "Pull latest") {
    primaryAction = { label: updateLabel, run: doPullUpstream, busy: isPulling };
  } else if (updateLabel === "Update") {
    primaryAction = { label: updateLabel, run: doUpdate, busy: isUpdating || guard.isResolving };
  }

  let forkAction: SkillPageAction | null = null;
  if (skill.source_kind === "fork") {
    forkAction = { label: "Un-fork", run: doUnfork, busy: isUnforking };
  } else if (forkDeployment) {
    forkAction = { label: "Fork", run: doFork, busy: isForking };
  }

  const removeActions = skillRemovalChoices(skill).map((choice) => ({
    key: choice.key,
    label: choice.label,
    run: () => void doRemove(choice),
    busy: isRemoving,
  }));

  return {
    path,
    copied,
    reveal,
    openEditor,
    copyPath,
    primaryAction,
    parkAction:
      parkEveryAgentPlan(skill).targets.length > 0
        ? {
            label: skill.parked ? "Turn on for every agent" : "Park for every agent",
            run: togglePark,
            busy: isParking,
          }
        : null,
    forkAction,
    removeActions,
    removeBlockedReason: skillRemovalBlockedReason(skill),
    updateDialog: guard.dialog,
  };
}

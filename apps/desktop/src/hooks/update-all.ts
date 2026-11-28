// ============================================================================
// update-all - "Update all": one batch update for many outdated skills, its
// progress and its toast, run by `runUpdateAll`.
// ============================================================================

import type {
  InstalledSkill,
  ForkRecord,
  LifecycleTarget,
  PullResult,
  UpdateAllOutcome,
} from "@skill-studio/lib";
import {
  excludeForkedOwner,
  forkableDeployment,
  forkTargetForSkill,
  ForkPullError,
  forkThenPull,
  lifecycleTargetForPark,
  pluginTargetKey,
  skillHasManagedUpdate,
  skillPluginUpdateTargets,
  skillUpdateOwnerTargets,
  uniquePluginTargets,
  updatePluginTargets,
} from "../lib/skill-lifecycle-target";
import type { PluginInstallUpdater, PluginUpdateTarget } from "../lib/skill-lifecycle-target";

interface UpdateAllTally {
  attempted: number;
  succeeded: number;
  failures: number;
  /** `attempted`/`succeeded` count update targets (one per copy); these count distinct skills, which is what the toast names. */
  skillsAttempted: number;
  skillsSucceeded: number;
  /** True when a stop request left skills unattempted; `skillsSucceeded` then counts only skills that finished. */
  stopped?: boolean;
  errors: string[];
  /** Skills whose pull left conflict markers; set only when there are any. */
  conflicted?: string[];
  /** The skills every copy of which was updated, so their rows can say so. */
  updated: string[];
}

const MAX_ERROR_LENGTH = 140;

/** The "Update all" toast title: "Updated N of M skills", or "Stopped. Updated N of M skills." after a stop. */
export function updateAllTitle(tally: UpdateAllTally): string {
  const { skillsAttempted, skillsSucceeded } = tally;
  const counts = `Updated ${skillsSucceeded} of ${skillsAttempted} skill${skillsAttempted === 1 ? "" : "s"}`;
  return tally.stopped ? `Stopped. ${counts}.` : counts;
}

/** "1 failed: <reasons>" for the toast, or `undefined` when nothing failed. Counts skills, matching the toast title; a skill with any failed copy counts once. */
export function updateAllFailureMessage(tally: UpdateAllTally): string | undefined {
  if (tally.failures === 0) return undefined;
  const failedSkills = tally.skillsAttempted - tally.skillsSucceeded;
  if (tally.errors.length === 0) return `${failedSkills} failed`;
  const reasons = tally.errors;
  return `${failedSkills} failed: ${reasons.map((reason) => (reason.length > MAX_ERROR_LENGTH ? `${reason.slice(0, MAX_ERROR_LENGTH - 1)}…` : reason)).join("; ")}`;
}

/**
 * "Update all": a fork pulls upstream one at a time (no batched CLI
 * form for that path), while every other outdated owner flattens into one
 * `updateAllOwners` call - one IPC round trip and one rescan for the whole
 * batch, instead of one update round trip and rescan per skill.
 * `onProgress(done, total, current)` counts forks and owner targets in one sequence;
 * `updateAllOwners` reports how many of its own targets finished. `current` names
 * the skill that starts next, or is `null` once everything finished. A skill named
 * in `forkEdited.names` is forked first and then pulled like a fork, so its
 * local edits survive instead of being overwritten by the batch. Plugin installs
 * run last, through `updatePluginInstall`, once per plugin id, scope, and project
 * even when several skills ship from one plugin. A fork is pulled only when the fork
 * itself is outdated, and a failed pull does not stop its plugin installs.
 * `shouldStop` is awaited before each fork and each plugin install and before the
 * owner batch; whatever it skips counts as not updated, and `tally.stopped` is set.
 * A step already running always finishes.
 */
export async function updateAllOutdatedSkills(
  skills: Pick<
    InstalledSkill,
    "name" | "deployments" | "source_kind" | "update_owner_ids" | "update_owners"
  >[],
  pullFork: (target: LifecycleTarget) => Promise<PullResult>,
  updateAllOwners: (
    targets: LifecycleTarget[],
    onOwnerDone: (done: number, skillName: string) => void,
  ) => Promise<UpdateAllOutcome>,
  onProgress?: (done: number, total: number, current: string | null) => void,
  forkEdited?: {
    names: ReadonlySet<string>;
    fork: (target: LifecycleTarget) => Promise<ForkRecord>;
  },
  updatePluginInstall?: PluginInstallUpdater,
  shouldStop?: () => boolean | Promise<boolean>,
): Promise<UpdateAllTally> {
  // An edited skill is forked only while the copy a fork replaces (its Global
  // Universal owner) is still in this batch; its other copies update normally.
  const forksEdit = (skill: (typeof skills)[number]) => {
    if (forkEdited?.names.has(skill.name) !== true) return false;
    const forkedOwnerId = forkableDeployment(skill)?.owner_id;
    return !!forkedOwnerId && skill.update_owner_ids.includes(forkedOwnerId);
  };
  const pullsUpstream = (skill: (typeof skills)[number]) =>
    skillHasManagedUpdate(skill) && (skill.source_kind === "fork" || forksEdit(skill));
  const forks = skills.filter(pullsUpstream);
  // A skill forked because it was edited keeps its other owners (project or
  // per-harness copies) on the normal update; only the forked owner is replaced.
  const ownerTargetsOf = (skill: (typeof skills)[number]) => {
    if (skill.source_kind === "fork") return [];
    const targets = skillUpdateOwnerTargets(skill);
    return pullsUpstream(skill) ? excludeForkedOwner(skill, targets) : targets;
  };
  const pluginTargetsOf = (skill: (typeof skills)[number]) =>
    updatePluginInstall ? skillPluginUpdateTargets(skill) : [];
  let total =
    forks.length +
    skills.flatMap(ownerTargetsOf).length +
    uniquePluginTargets(skills.flatMap(pluginTargetsOf)).length;
  const ownerSkillNames = new Set(
    skills.flatMap((skill) =>
      !pullsUpstream(skill) &&
      (skillUpdateOwnerTargets(skill).length > 0 || pluginTargetsOf(skill).length > 0)
        ? [skill.name]
        : [],
    ),
  );
  const failedSkillNames = new Set<string>();
  const skippedSkillNames = new Set<string>();
  // Skills whose own copies or fork creation failed, as opposed to a fork pull: only these hold
  // their plugins back.
  const copyFailedNames = new Set<string>();
  const tally: UpdateAllTally = {
    attempted: total,
    succeeded: 0,
    failures: 0,
    skillsAttempted: forks.length + ownerSkillNames.size,
    skillsSucceeded: 0,
    errors: [],
    updated: [],
  };
  let pluginsDone = 0;
  const fail = (count: number, message: string) => {
    tally.failures += count;
    if (!tally.errors.includes(message)) tally.errors.push(message);
  };
  // A skill whose copies failed keeps its plugins untouched, and so does every other skill
  // sharing one of those plugins. They leave the total too, so progress still reaches it.
  const blockedPluginKeys = () =>
    new Set(
      skills.flatMap((skill) =>
        copyFailedNames.has(skill.name) ? pluginTargetsOf(skill).map(pluginTargetKey) : [],
      ),
    );
  const pluginTargetsOfOk = () => {
    const blocked = blockedPluginKeys();
    return uniquePluginTargets(
      skills.flatMap((skill) =>
        copyFailedNames.has(skill.name)
          ? []
          : pluginTargetsOf(skill).filter((target) => !blocked.has(pluginTargetKey(target))),
      ),
    );
  };
  // The first skill that ships `target`'s plugin install: the name shown while it updates.
  const pluginSkillName = (target: PluginUpdateTarget) =>
    skills.find((skill) =>
      pluginTargetsOf(skill).some((own) => pluginTargetKey(own) === pluginTargetKey(target)),
    )?.name ?? null;
  // A skill whose fork failed keeps all its owners untouched, so its other copies are not updated either.
  const ownerEntries = () =>
    skills.flatMap((skill) =>
      failedSkillNames.has(skill.name)
        ? []
        : ownerTargetsOf(skill).map((target) => ({ name: skill.name, target })),
    );
  const firstPluginName = () => {
    const [first] = pluginTargetsOfOk();
    return first ? pluginSkillName(first) : null;
  };
  const afterForksName = () => ownerEntries()[0]?.name ?? firstPluginName();
  onProgress?.(0, total, forks[0]?.name ?? afterForksName());

  for (const [index, skill] of forks.entries()) {
    // react-doctor-disable-next-line react-doctor/async-await-in-loop -- the stop check sits between sequential pulls
    if (await shouldStop?.()) {
      for (const unstarted of forks.slice(index)) skippedSkillNames.add(unstarted.name);
      break;
    }
    const makesFork = skill.source_kind !== "fork" && forkEdited !== undefined;
    try {
      const pullOne = makesFork
        ? () => forkThenPull(forkTargetForSkill(skill), forkEdited.fork, pullFork)
        : () => pullFork(lifecycleTargetForPark(skill));
      // react-doctor-disable-next-line react-doctor/async-await-in-loop -- update-all runs sequentially on purpose; concurrent `npx skills update` calls race on ~/.agents/.skill-lock.json
      const pull = await pullOne();
      if (pull.conflicts.length > 0) (tally.conflicted ??= []).push(skill.name);
      tally.succeeded += 1;
    } catch (error) {
      failedSkillNames.add(skill.name);
      if (makesFork && !(error instanceof ForkPullError)) copyFailedNames.add(skill.name);
      fail(1, error instanceof Error ? error.message : String(error));
    }
    onProgress?.(index + 1, total, forks[index + 1]?.name ?? afterForksName());
  }

  const owners = ownerEntries();
  const ownerTargets = owners.map((entry) => entry.target);
  const retotal = (done: number, current: string | null) => {
    const plannedTotal = total;
    total = forks.length + ownerTargets.length + pluginTargetsOfOk().length;
    tally.attempted = total;
    if (total !== plannedTotal) onProgress?.(done, total, current);
  };
  retotal(forks.length, afterForksName());
  if (ownerTargets.length > 0 && (await shouldStop?.())) {
    for (const entry of owners) skippedSkillNames.add(entry.name);
  } else if (ownerTargets.length > 0) {
    try {
      // Refused targets finish first, so `done` alone cannot say which owner is next:
      // each event consumes one entry of the skill it names.
      const pending = [...owners];
      const outcome = await updateAllOwners(ownerTargets, (done, skillName) => {
        const finished = pending.findIndex((entry) => entry.name === skillName);
        if (finished >= 0) pending.splice(finished, 1);
        onProgress?.(forks.length + done, total, pending[0]?.name ?? firstPluginName());
      });
      // `errors` is keyed by skill name, so two failing owners of one
      // twice-installed skill collapse to one entry there; `items` carries
      // one entry per owner regardless, so count failures from `items`
      // instead (N1, review round 3).
      for (const name of outcome.not_run) skippedSkillNames.add(name);
      const failedItems = outcome.items.filter((item) => item.outcome === null);
      tally.succeeded += outcome.items.length - failedItems.length;
      for (const item of failedItems) {
        failedSkillNames.add(item.skill);
        copyFailedNames.add(item.skill);
      }
      for (const item of failedItems) fail(1, outcome.errors[item.skill] ?? `${item.skill} failed`);
    } catch (error) {
      // Every skill in the rejected batch failed, including a forked skill's other copies.
      for (const skill of skills) {
        if (!failedSkillNames.has(skill.name) && ownerTargetsOf(skill).length > 0) {
          failedSkillNames.add(skill.name);
          copyFailedNames.add(skill.name);
        }
      }
      fail(ownerTargets.length, error instanceof Error ? error.message : String(error));
    }
  }

  retotal(forks.length + ownerTargets.length, firstPluginName());
  const pluginTargets = pluginTargetsOfOk();
  if (updatePluginInstall) {
    // A skill sharing a plugin with a failed one is not updated, so it does not count as updated.
    const blocked = blockedPluginKeys();
    for (const skill of skills) {
      if (pluginTargetsOf(skill).some((target) => blocked.has(pluginTargetKey(target)))) {
        failedSkillNames.add(skill.name);
      }
    }
  }
  if (updatePluginInstall && pluginTargets.length > 0) {
    const plugins = await updatePluginTargets(
      pluginTargets,
      (target) =>
        updatePluginInstall(target).finally(() => {
          pluginsDone += 1;
          const next = pluginTargets[pluginsDone];
          onProgress?.(
            forks.length + ownerTargets.length + pluginsDone,
            total,
            next ? pluginSkillName(next) : null,
          );
        }),
      shouldStop,
    );
    const notRunKeys = new Set(plugins.notRun.map(pluginTargetKey));
    for (const skill of skills) {
      if (pluginTargetsOf(skill).some((target) => notRunKeys.has(pluginTargetKey(target)))) {
        skippedSkillNames.add(skill.name);
      }
    }
    tally.succeeded += plugins.succeeded;
    if (plugins.failures.length > 0) {
      const failedKeys = new Set(
        plugins.failures.map((failure) => pluginTargetKey(failure.target)),
      );
      for (const skill of skills) {
        if (pluginTargetsOf(skill).some((target) => failedKeys.has(pluginTargetKey(target)))) {
          failedSkillNames.add(skill.name);
        }
      }
      for (const failure of plugins.failures) fail(1, failure.message);
    }
  }

  // Core can report one requested skill under two names, so the difference can go below zero.
  const notUpdated = new Set([...failedSkillNames, ...skippedSkillNames]);
  tally.skillsSucceeded = Math.max(0, tally.skillsAttempted - notUpdated.size);
  const requested = [...forks.map((skill) => skill.name), ...ownerSkillNames];
  const requestedNames = new Set(requested);
  // A failure reported under a name nobody requested cannot be pinned to a row, so no row claims it.
  const unattributed = [...notUpdated].some((name) => !requestedNames.has(name));
  tally.updated = unattributed ? [] : requested.filter((name) => !notUpdated.has(name));
  if (skippedSkillNames.size > 0) tally.stopped = true;
  return tally;
}

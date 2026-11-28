// ============================================================================
// Skill Studio - skill-coverage
// Effective visibility of a skill to an agent: the shared `.agents/skills`
// root is read natively by Codex, OpenCode, pi, Cursor and Grok Build, so a
// skill deployed only there is still visible to them even without a
// deployment in their own directory. Claude Code does not read the shared
// root (see docs/agent-skill-conventions.md line 67); it needs its own
// deployment.
// ============================================================================

import { HARNESS_CATALOG } from "./skill-types.generated";
import { ownDeployments } from "./skill-plugin-partition";
import type { AgentId, Deployment, InstalledSkill } from "./skill-types";

/** First-class agents that read the shared `.agents/skills` root natively. */
export const AGENTS_READING_SHARED_ROOT: readonly AgentId[] = HARNESS_CATALOG.filter(
  (row) => row.readsSharedFolder,
).map((row) => row.id);

const ID_BY_LABEL = new Map<string, AgentId>(HARNESS_CATALOG.map((row) => [row.label, row.id]));
const LABEL_BY_ID = new Map<string, string>(HARNESS_CATALOG.map((row) => [row.id, row.label]));

/** The `Deployment.agent` value for a deployment placed in the shared root, not any one agent's own folder. */
const SHARED_AGENT_ID = "shared";

/**
 * Maps a `Deployment.agent` display label (the Rust side serializes
 * `AgentId::display_name()`, e.g. "Claude Code") to the first-class agent id.
 * `null` for the shared root and for agents outside the first-class set.
 */
export function agentIdFromDeploymentLabel(label: string): AgentId | "shared" | null {
  return label === SHARED_AGENT_ID ? "shared" : (ID_BY_LABEL.get(label) ?? null);
}

/** The inverse of `agentIdFromDeploymentLabel`: a wire harness id (e.g. from
 * `DiscoverySourceSetting.harness`) to the label a deployment/agent chip
 * shows. Falls back to the id itself for one this function doesn't know. */
export function deploymentLabelFromAgentId(id: string): string {
  return LABEL_BY_ID.get(id) ?? id;
}

/**
 * True when a deployment's symlink doesn't resolve: either the target is
 * confirmed missing (`symlink_is_broken`), or resolving it failed for some
 * other reason (`symlink_error`, e.g. a permissions error or a symlink loop).
 * Either way the deployment can't back visibility.
 */
export function isUnresolvedDeployment(deployment: Deployment): boolean {
  return deployment.symlink_is_broken || deployment.symlink_error != null;
}

/**
 * A deployment counts toward visibility only when its symlink (if any)
 * resolves and it isn't disabled for its harness (see `Deployment.disabled`)
 * or parked (see `InstalledSkill.parked`) - either way the harness doesn't
 * actually see it.
 */
function isOwnDirDeployment(skill: InstalledSkill, agent: AgentId): boolean {
  if (skill.parked) return false;
  return ownDeployments(skill).some(
    (d) =>
      agentIdFromDeploymentLabel(d.agent) === agent &&
      (d.scope === "global" || d.scope === "project") &&
      !d.disabled &&
      !isUnresolvedDeployment(d),
  );
}

/** True when the deployment is a symlink whose target lives in a `.agents/skills` root. */
function isLinkedToSharedRoot(target: string | null | undefined): boolean {
  return target != null && /\/\.agents\/skills\//.test(target + "/");
}

/**
 * Classifies a single deployment for the small link marker shown next to its
 * harness chip: `shared-root` when it *is* the shared `.agents/skills` copy,
 * `linked-to-shared` when it's a symlink pointing into that shared root,
 * `broken` when its symlink target doesn't resolve, else `own`.
 */
export function deploymentLinkKind(
  deployment: Deployment,
): "shared-root" | "linked-to-shared" | "own" | "broken" {
  if (isUnresolvedDeployment(deployment)) return "broken";
  if (deployment.agent === SHARED_AGENT_ID) return "shared-root";
  if (deployment.is_symlink && isLinkedToSharedRoot(deployment.symlink_target)) {
    return "linked-to-shared";
  }
  // A real directory (not a symlink) can still be the same folder as the
  // shared root, reached through a linked root (e.g. a whole `.claude/skills`
  // symlinked to `.agents/skills`) - see `Deployment.resolved_path`.
  if (!deployment.is_symlink && isLinkedToSharedRoot(deployment.resolved_path)) {
    return "linked-to-shared";
  }
  return "own";
}

/** Where a skill really lives, and who links to or copies it - see `SkillLocationCell`. */
export interface LocationSummary {
  /** The deployment under the shared root, when the skill has one; the global one when the skill
   * has a shared root in more than one scope. */
  truth: Deployment | null;
  /** Deployments that are symlinks into the shared root. */
  links: Deployment[];
  /** Own deployments other than the shared-root truth - real directories, or healthy symlinks to somewhere outside the shared root. Two copies can drift. */
  copies: Deployment[];
  /** Deployments whose symlink doesn't resolve. */
  broken: Deployment[];
}

/**
 * Groups `skill`'s own deployments (see `ownDeployments`) by their relation
 * to the shared `.agents/skills` root, for `SkillLocationCell`.
 */
export function locationSummary(skill: InstalledSkill): LocationSummary {
  const summary: LocationSummary = { truth: null, links: [], copies: [], broken: [] };
  for (const deployment of ownDeployments(skill)) {
    switch (deploymentLinkKind(deployment)) {
      case "shared-root":
        if (summary.truth === null) {
          summary.truth = deployment;
        } else if (summary.truth.scope !== "global" && deployment.scope === "global") {
          summary.copies.push(summary.truth);
          summary.truth = deployment;
        } else {
          summary.copies.push(deployment);
        }
        break;
      case "linked-to-shared":
        summary.links.push(deployment);
        break;
      case "broken":
        summary.broken.push(deployment);
        break;
      case "own":
        summary.copies.push(deployment);
        break;
    }
  }
  return summary;
}

/** The copy every other copy is measured against: the Universal folder, or with none the first
 * live copy that has a content hash. */
export function driftReference(summary: LocationSummary): Deployment | null {
  return summary.truth ?? comparableCopies(summary)[0] ?? null;
}

function comparableCopies(summary: LocationSummary): Deployment[] {
  return summary.copies.filter((copy) => copy.scope !== "parked" && copy.content_hash);
}

/**
 * Which live copies have drifted from `driftReference`: a different content hash. A parked copy
 * is off for every agent, so it never counts. Same-content copies aren't a drift risk worth a
 * warning border - see `SkillLocationCell`.
 */
export function driftingCopies(summary: LocationSummary): Deployment[] {
  const reference = driftReference(summary);
  if (!reference?.content_hash) return [];
  return comparableCopies(summary).filter(
    (copy) => copy !== reference && copy.content_hash !== reference.content_hash,
  );
}

/** `pickCompareDefaults`'s picks, either of which is unset when `candidates` is empty. */
export interface CompareDefaults {
  left: Deployment | undefined;
  right: Deployment | undefined;
}

/**
 * `SkillCompareDialog`'s default left/right picks out of its candidate
 * deployments (own deployments with a `content_hash`): left is the copy with
 * a strict majority content hash, if one exists, else the first candidate;
 * right is the first candidate whose content differs from the left's, so the
 * dialog opens already showing a real difference when one exists.
 */
export function pickCompareDefaults(candidates: Deployment[]): CompareDefaults {
  if (candidates.length === 0) return { left: undefined, right: undefined };

  const counts = new Map<string, number>();
  for (const d of candidates) counts.set(d.content_hash, (counts.get(d.content_hash) ?? 0) + 1);
  const majorityHash = [...counts.entries()].find(
    ([, count]) => count * 2 > candidates.length,
  )?.[0];

  const left =
    majorityHash !== undefined
      ? candidates.find((d) => d.content_hash === majorityHash)
      : candidates[0];
  const right = candidates.find((d) => d !== left && d.content_hash !== left?.content_hash);
  return { left, right };
}

/**
 * `SkillCompareDialog`'s selected path for one side, kept stable across a
 * background rescan: `selectedPath` stays selected as long as some candidate
 * still has it, since that's a choice the user made and it must never be
 * overridden while it remains valid. Once it stops existing (the deployment
 * was removed or renamed), falls back to `fallbackPath` - the side's default
 * pick recomputed from the current candidates.
 */
export function resolveCompareSelection(
  selectedPath: string | undefined,
  candidates: Deployment[],
  fallbackPath: string | undefined,
): string | undefined {
  if (selectedPath !== undefined && candidates.some((d) => d.path === selectedPath)) {
    return selectedPath;
  }
  return fallbackPath;
}

/**
 * A broken, disabled, or parked shared-root deployment doesn't make a skill
 * visible via the shared root. When `reader` is set, a shared deployment that
 * explicitly disables that reader does not provide visibility to it.
 */
function isSharedRootDeployment(skill: InstalledSkill, reader?: AgentId): boolean {
  if (skill.parked) return false;
  return ownDeployments(skill).some(
    (d) =>
      d.agent === SHARED_AGENT_ID &&
      !d.disabled &&
      !isUnresolvedDeployment(d) &&
      (reader === undefined || !(d.disabled_readers ?? []).includes(reader)),
  );
}

/**
 * Whether `agent` can actually see `skill`: "own" via a deployment in the
 * agent's own directory, "shared" via the shared root (only for agents that
 * read it), or "none". This is effective visibility only - it says nothing
 * about whether the agent's *own* deployment is a healthy link; a broken own
 * deployment with a healthy shared fallback still reports "shared" here.
 * `cellForAgent` layers that local-link health back on for the matrix.
 */
export function skillVisibleToAgent(
  skill: InstalledSkill,
  agent: AgentId,
): "own" | "shared" | "none" {
  if (isOwnDirDeployment(skill, agent)) return "own";
  if (AGENTS_READING_SHARED_ROOT.includes(agent) && isSharedRootDeployment(skill, agent)) {
    return "shared";
  }
  return "none";
}

/** A live plugin copy in `agent`'s own plugin cache. `skillVisibleToAgent` counts only the skill's
 * own global and project deployments. A plugin cache copy can come without its plugin's details. */
function readsThroughPlugin(skill: InstalledSkill, agent: AgentId): boolean {
  if (skill.parked) return false;
  return skill.deployments.some(
    (d) =>
      (d.plugin || d.scope === "plugin") &&
      agentIdFromDeploymentLabel(d.agent) === agent &&
      !d.disabled &&
      !isUnresolvedDeployment(d),
  );
}

/**
 * How `agent` reads `skill`: `skillVisibleToAgent`, or `plugin` through a live plugin copy. The one
 * reader rule for the Skills row's agent icons, the agent filter and the coverage grid, so a row
 * that shows an agent's icon is also listed under that agent and marked in its column. Pass the
 * skill with its plugin copies.
 */
export function skillReadByAgent(
  skill: InstalledSkill,
  agent: AgentId,
): "own" | "shared" | "plugin" | "none" {
  const visible = skillVisibleToAgent(skill, agent);
  if (visible !== "none") return visible;
  return readsThroughPlugin(skill, agent) ? "plugin" : "none";
}

// ============================================================================
// Skill x agent matrix (moved from skill-stats.ts): now reflects effective
// visibility rather than raw per-agent deployments.
// ============================================================================

/** Column order for the per-agent columns of the coverage matrix. */
export const AGENT_MATRIX_LABELS = HARNESS_CATALOG.map((row) => row.label);

export type AgentMatrixLabel = (typeof AGENT_MATRIX_LABELS)[number];

/**
 * One matrix cell: whether the skill is visible (`state`), and whether the
 * link backing that visibility is a (broken) symlink. `isBroken` also flags a
 * broken own-directory deployment even when `state` is "shared" because a
 * healthy shared copy still makes the skill visible - the marker exists so
 * the matrix keeps showing the local breakage worth fixing.
 */
export interface AgentMatrixCell {
  state: "own" | "shared" | "plugin" | "none";
  isSymlink: boolean;
  isBroken: boolean;
}

/** One row of `agentMatrix`: a skill, its shared-root cell, and its per-agent cells. */
export interface AgentMatrixRow {
  skill: InstalledSkill;
  shared: AgentMatrixCell;
  cells: Record<AgentMatrixLabel, AgentMatrixCell>;
}

const EMPTY_CELL: AgentMatrixCell = { state: "none", isSymlink: false, isBroken: false };

function cellForAgent(skill: InstalledSkill, agent: AgentId): AgentMatrixCell {
  const state = skillReadByAgent(skill, agent);
  const own = ownDeployments(skill);
  const ownDeployment = own.find(
    (d) =>
      agentIdFromDeploymentLabel(d.agent) === agent &&
      (d.scope === "global" || d.scope === "project"),
  );

  if (state === "none") {
    // The own-dir copy is broken and there's no healthy shared fallback: the
    // skill is effectively invisible to this agent, but keep the broken
    // marker in the matrix rather than reporting a plain empty cell.
    if (ownDeployment && isUnresolvedDeployment(ownDeployment)) {
      return { state: "none", isSymlink: ownDeployment.is_symlink, isBroken: true };
    }
    return EMPTY_CELL;
  }
  if (state === "plugin") return { state, isSymlink: false, isBroken: false };

  const deployment = state === "own" ? ownDeployment : own.find((d) => d.agent === SHARED_AGENT_ID);

  return {
    state,
    isSymlink: deployment?.is_symlink ?? false,
    // A healthy shared fallback (`state === "shared"`) doesn't hide a broken
    // own-directory deployment - the matrix still marks it broken so it's
    // visible as something worth fixing, even though the skill remains
    // effectively visible to the agent via the shared root.
    isBroken:
      (deployment !== undefined && isUnresolvedDeployment(deployment)) ||
      (ownDeployment !== undefined && isUnresolvedDeployment(ownDeployment)),
  };
}

function cellForSharedRoot(skill: InstalledSkill): AgentMatrixCell {
  const deployment = ownDeployments(skill).find((d) => d.agent === SHARED_AGENT_ID);
  if (!deployment) return EMPTY_CELL;
  return { state: "own", isSymlink: deployment.is_symlink, isBroken: deployment.symlink_is_broken };
}

/**
 * Builds the skill x agent visibility matrix: for every skill, whether each
 * first-class agent sees it (own directory or via the shared root), plus a
 * dedicated cell for the shared root itself. Rows follow `skills`' input
 * order; sort before calling if needed.
 */
export function agentMatrix(skills: InstalledSkill[]): AgentMatrixRow[] {
  return skills.map((skill) => ({
    skill,
    shared: cellForSharedRoot(skill),
    // SAFETY: mapping every AGENT_MATRIX_LABELS entry produces exactly the
    // keys of Record<AgentMatrixLabel, AgentMatrixCell>.
    cells: Object.fromEntries(
      HARNESS_CATALOG.map(({ label, id }) => [label, cellForAgent(skill, id)]),
    ) as Record<AgentMatrixLabel, AgentMatrixCell>,
  }));
}

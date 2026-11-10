import type { SkillEvent } from "@skill-studio/lib";

/** True when Activity can offer the backend restore action for this event. */
export function canRestoreSkillEvent(event: SkillEvent): boolean {
  return event.restorable;
}

/** The backend's drift-guard refusal names the drifted path and ends in this phrase - see event_store.rs. */
function isDriftRefusal(message: string): boolean {
  return message.includes("changed since") || message.includes("drifted");
}

/** True only when the backend says bypassing this event's drift check is safe. */
export function shouldOfferForceRestore(event: SkillEvent, message: string): boolean {
  return event.force_restorable && isDriftRefusal(message);
}

/**
 * The row's label. A split that names an agent is that agent's turn-off: the
 * plain split row has no agent, "Turn off for one agent" writes the agent on it.
 */
export function eventLabel(event: SkillEvent, agentLabel: string | null): string {
  if (event.kind === "split" && agentLabel) return `Turned off for ${agentLabel}`;
  return kindLabel(event.kind);
}

const EVENT_LABELS = new Map([
  ["install", "Installed"],
  ["remove", "Removed"],
  ["update", "Updated"],
  ["park", "Parked"],
  ["unpark", "Unparked"],
  ["harness_disable", "Turned off"],
  ["harness_enable", "Turned on"],
  ["move_aside_disable", "Moved aside"],
  ["move_aside_restore", "Moved back"],
  ["invocation_change", "Invocation changed"],
  ["fork", "Forked"],
  ["unlink_harness", "Agent link removed"],
  ["relink_harness", "Agent link restored"],
  ["explode_shared_dir", "Whole-folder link converted"],
  ["materialize_then_disable", "Agent copy turned off"],
  ["reconcile_remove_stale_link", "Stale link removed"],
  ["repair_remove_link", "Broken link removed"],
  ["repair_relink_link", "Link repaired"],
  ["make_independent_copy", "Independent copy created"],
  ["repair_skill_frontmatter", "Frontmatter repaired"],
  ["restore", "Restored"],
  ["quarantine_prune", "Quarantine pruned"],
  ["split", "Split into agent copies"],
  ["distribute_from_shared", "Distributed from Universal"],
]);

export function kindLabel(kind: string): string {
  return EVENT_LABELS.get(kind) ?? kind.replace(/_/g, " ");
}

export function restoreUnavailableReason(event: SkillEvent): string {
  if (event.reverted_by) return "Already restored.";
  switch (event.kind) {
    case "park":
      return "Unpark the skill to reverse this.";
    case "unpark":
      return "Park the skill to reverse this.";
    case "quarantine_prune":
      return "Pruned backups cannot be restored.";
    default:
      return "This event has no saved change to restore.";
  }
}

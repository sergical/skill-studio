// ============================================================================
// skill-location-helpers - pure functions shared by the Locations card's
// action wiring. Split out so `skill-location-status.ts` only exports the
// card's status model (react-doctor/only-export-components: mixing component
// and non-component exports in one file defeats Fast Refresh).
// ============================================================================

import type { Deployment, LeftBehindPair } from "@skill-studio/lib";

/** What each left-behind fix does to the pair - see `LeftBehindDialog`. */
interface LeftBehindFix {
  /** The copy the fix deletes. */
  discard: Deployment;
  /** The copy that stays. */
  keep: Deployment;
  /** True when the deleted copy sits in a repository, so the confirm checks it. */
  checksRepository: boolean;
}

/** "Keep live" deletes the parked copy; "Keep parked" deletes the live one. */
export function leftBehindFix(
  choice: "keep-live" | "keep-parked",
  pair: LeftBehindPair,
): LeftBehindFix {
  return choice === "keep-live"
    ? { discard: pair.parked, keep: pair.live, checksRepository: false }
    : { discard: pair.live, keep: pair.parked, checksRepository: pair.live.scope === "project" };
}

/**
 * The git warning a confirm shows for a copy that sits in a project, from
 * `parkCheck`'s `git_tracked`: `true` warns, `null` says the check could not
 * run, `false` shows nothing. `doing` is the gerund the confirm is about,
 * e.g. "parking". Never a reason to block the action.
 */
export function gitWarningText(gitTracked: boolean | null, doing: string): string | null {
  if (gitTracked === false) return null;
  const effect = `${doing} it shows as deleted files in your repository.`;
  return gitTracked
    ? `Git tracks this folder, so ${effect}`
    : `Couldn't check whether git tracks this folder. If it does, ${effect}`;
}

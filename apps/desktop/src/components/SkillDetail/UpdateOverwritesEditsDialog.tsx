// ============================================================================
// UpdateOverwritesEditsDialog - the confirm step before Update replaces a
// skills.sh skill the user edited. Offers a fork (keep the edits, merge the
// update in) ahead of the overwrite. Used for one skill and for "Update all".
// ============================================================================

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@skill-studio/ui";

/** The pending "Update will replace your edits" choice: the dialog is open while `skillNames` is not empty. */
export interface UpdatePrompt {
  skillNames: string[];
  /** False when none of the edited skills can be forked here. */
  canFork: boolean;
  unforkableSkillNames: string[];
  fork: (unforkable?: "skip" | "overwrite") => void;
  overwrite: () => void;
  cancel: () => void;
}

function forkCopy(skillNames: string[], isBulk: boolean): string {
  if (!isBulk) {
    return `You changed ${skillNames[0]} after you installed it. Fork it to keep your edits and merge the update in, or overwrite your edits with the new version.`;
  }
  const pronoun = skillNames.length === 1 ? "it" : "them";
  return `You changed ${skillNames.join(", ")} after you installed ${pronoun}. Fork ${pronoun} to keep your edits and merge the update in, or overwrite your edits with the new version.`;
}

function noForkCopy(skillNames: string[], isBulk: boolean): string {
  const names = isBulk ? skillNames.join(", ") : skillNames[0];
  return `You changed ${names} after you installed ${isBulk && skillNames.length > 1 ? "them" : "it"}. ${isBulk && skillNames.length > 1 ? "They" : "It"} cannot be forked here, so the update would overwrite your edits.`;
}

export function UpdateOverwritesEditsDialog({
  skillNames,
  isBulk,
  canFork,
  unforkableSkillNames = [],
  onFork,
  onOverwrite,
  onCancel,
}: {
  /** The edited skills the update would replace; the dialog is closed while this is empty. */
  skillNames: string[];
  /** True for "Update all": the buttons say "all" and the description lists every name. */
  isBulk: boolean;
  canFork: boolean;
  unforkableSkillNames?: string[];
  onFork: (unforkable: "skip" | "overwrite") => void;
  onOverwrite: () => void;
  onCancel: () => void;
}) {
  const unforkableNames = new Set(unforkableSkillNames);
  const forkableNames = skillNames.filter((name) => !unforkableNames.has(name));
  return (
    <AlertDialog open={skillNames.length > 0} onOpenChange={(open) => !open && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Update will replace your edits</AlertDialogTitle>
          <AlertDialogDescription>
            {canFork ? forkCopy(forkableNames, isBulk) : noForkCopy(skillNames, isBulk)}
            {canFork &&
              unforkableSkillNames.length > 0 &&
              ` ${unforkableSkillNames.join(", ")} cannot be forked here. Skip ${unforkableSkillNames.length === 1 ? "it" : "them"}, or overwrite ${unforkableSkillNames.length === 1 ? "its" : "their"} edits.`}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={onOverwrite}>
            {isBulk ? "Overwrite all" : "Overwrite edits"}
          </AlertDialogAction>
          {canFork && (
            <>
              <AlertDialogAction onClick={() => onFork("overwrite")}>
                {unforkableSkillNames.length > 0
                  ? "Fork eligible and overwrite others"
                  : isBulk
                    ? "Fork edited and update all"
                    : "Fork and update"}
              </AlertDialogAction>
              {unforkableSkillNames.length > 0 && (
                <AlertDialogAction variant="outline" onClick={() => onFork("skip")}>
                  Fork eligible and skip others
                </AlertDialogAction>
              )}
            </>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

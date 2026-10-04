// ============================================================================
// LeftBehindDialog - the confirm step for the two fixes to a parked copy left
// behind: "Keep live" deletes the parked copy, "Keep parked" deletes the live
// copy. Both go through `discard_skill_copy`, which Activity can undo.
// ============================================================================

import { useState } from "react";
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
import { homeRelativePath } from "@skill-studio/lib";
import type { LeftBehindPair } from "@skill-studio/lib";
import { useGitWarning } from "../../hooks/useGitWarning";
import { discardSkillCopy } from "../../lib/skill-api";
import { useAppStore } from "../../store/appStore";
import { leftBehindFix } from "./skill-location-helpers";

export type LeftBehindChoice = "keep-live" | "keep-parked";

export function LeftBehindDialog({
  skillName,
  choice,
  pair,
  onClose,
}: {
  skillName: string;
  choice: LeftBehindChoice;
  pair: LeftBehindPair;
  onClose: () => void;
}) {
  const [isWorking, setIsWorking] = useState(false);
  const addToast = useAppStore((state) => state.addToast);
  const keepLive = choice === "keep-live";
  const { discard: doomed, keep: kept, checksRepository } = leftBehindFix(choice, pair);
  const gitWarning = useGitWarning({ deployment_id: doomed.id }, "deleting", checksRepository);

  const handleConfirm = () => {
    setIsWorking(true);
    discardSkillCopy({ deployment_id: doomed.id })
      .then(onClose)
      .catch((err) => {
        addToast({
          type: "error",
          title: keepLive ? "Couldn't delete the parked copy" : "Couldn't delete the live copy",
          message: err instanceof Error ? err.message : "Unknown error",
        });
      })
      .finally(() => setIsWorking(false));
  };

  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {keepLive
              ? `Keep the live copy of ${skillName}?`
              : `Keep the parked copy of ${skillName}?`}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {keepLive
              ? `This deletes the parked copy at ${homeRelativePath(doomed.path)}. The live copy at ${homeRelativePath(kept.path)} stays on.`
              : `This deletes the live copy at ${homeRelativePath(doomed.path)}. The parked copy at ${homeRelativePath(kept.path)} stays parked, so the skill stays off.`}{" "}
            You can undo it from Activity.
          </AlertDialogDescription>
          {gitWarning && <p className="m-0 text-small text-text-secondary">{gitWarning}</p>}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onClose} disabled={isWorking}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={handleConfirm} disabled={isWorking}>
            {isWorking ? "Deleting…" : keepLive ? "Keep live" : "Keep parked"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

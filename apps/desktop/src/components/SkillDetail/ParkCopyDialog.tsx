// ============================================================================
// ParkCopyDialog - the confirm step for parking a project copy. The folder
// moves out of the repository into ~/.agents/skills-parked, so the dialog
// warns when git tracks it (the move shows as deleted files).
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
import type { Deployment } from "@skill-studio/lib";
import { useGitWarning } from "../../hooks/useGitWarning";
import { parkSkill } from "../../lib/skill-api";
import { useAppStore } from "../../store/appStore";

export function ParkCopyDialog({
  skillName,
  deployment,
  scopeLabel,
  onClose,
}: {
  skillName: string;
  deployment: Deployment;
  /** The project's block name on the card, e.g. "Project · web". */
  scopeLabel: string;
  onClose: () => void;
}) {
  const [isParking, setIsParking] = useState(false);
  const addToast = useAppStore((state) => state.addToast);
  const { warning: gitWarning, isChecking } = useGitWarning(
    { deployment_id: deployment.id },
    "parking",
    true,
  );

  const handlePark = () => {
    setIsParking(true);
    parkSkill({ deployment_id: deployment.id })
      .then(onClose)
      .catch((err) => {
        addToast({
          type: "error",
          title: "Couldn't park skill",
          message: err instanceof Error ? err.message : "Unknown error",
        });
      })
      .finally(() => setIsParking(false));
  };

  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            Park {skillName} in {scopeLabel}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            This moves {homeRelativePath(deployment.path)} out of the project into
            ~/.agents/skills-parked, so every agent stops loading it. Turn it on again to move it
            back.
          </AlertDialogDescription>
          {gitWarning && <p className="m-0 text-small text-text-secondary">{gitWarning}</p>}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onClose} disabled={isParking}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction onClick={handlePark} disabled={isParking || isChecking}>
            {isParking ? "Parking…" : isChecking ? "Checking git…" : "Park"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

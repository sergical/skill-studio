// ============================================================================
// RemoveDeploymentsDialog - the confirm step for the Locations card's trash
// action. `remove_skill` works per scope, not per harness, so this names the
// scope it is about to clear rather than pretending a single harness can be
// deleted on its own.
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
import { useGitWarning } from "../../hooks/useGitWarning";
import { removeSkill } from "../../lib/skill-api";
import {
  skillDeploymentRemovalAvailability,
  skillRemovalAvailability,
  skillRemovalDescription,
} from "../../lib/skill-lifecycle-target";
import { useAppStore } from "../../store/appStore";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";

export function RemoveDeploymentsDialog({
  skill,
  scopeLabel,
  projectPath,
  deployment,
  onClose,
}: {
  skill: InstalledSkill;
  /** "Global" or the project folder's name - what the section this came from is called. */
  scopeLabel: string;
  /** `null` for the global scope, otherwise the project directory to remove from. */
  projectPath: string | null;
  /** Exact independent Copy selected from a Locations row. */
  deployment?: Deployment;
  onClose: () => void;
}) {
  const [isRemoving, setIsRemoving] = useState(false);
  const addToast = useAppStore((state) => state.addToast);
  const skillName = skill.name;
  const removalAvailability = deployment
    ? skillDeploymentRemovalAvailability(skill, deployment)
    : skillRemovalAvailability(skill, {
        skillName,
        scope: projectPath ? "project" : "global",
        projectPath,
      });

  // `park_check` takes one folder: the selected copy, else the first real folder the removal deletes.
  const checked =
    deployment ??
    (removalAvailability.available ? removalAvailability.preview.managedDeployments[0] : undefined);
  const { warning: gitWarning, isChecking } = useGitWarning(
    checked ? { deployment_id: checked.id } : null,
    "removing",
    projectPath !== null,
  );

  const handleRemove = () => {
    if (!removalAvailability.available) return;
    setIsRemoving(true);
    removeSkill(removalAvailability.preview.target)
      .then(() => {
        onClose();
      })
      .catch((err) => {
        addToast({
          type: "error",
          title: "Couldn't remove",
          message: err instanceof Error ? err.message : "Unknown error",
        });
      })
      .finally(() => setIsRemoving(false));
  };

  return (
    <AlertDialog open onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            Remove {skillName} from {scopeLabel}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            {removalAvailability.available
              ? skillRemovalDescription(removalAvailability.preview)
              : removalAvailability.reason}
          </AlertDialogDescription>
          {gitWarning && <p className="m-0 text-small text-text-secondary">{gitWarning}</p>}
          {removalAvailability.available && (
            <ul className="max-h-40 overflow-auto font-mono text-xs break-all">
              {[
                ...removalAvailability.preview.linkedDeployments,
                ...removalAvailability.preview.otherLinks,
                ...removalAvailability.preview.managedDeployments,
              ].map((item) => (
                <li key={item.id || item.path}>{item.path}</li>
              ))}
            </ul>
          )}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onClose} disabled={isRemoving}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={handleRemove}
            disabled={isRemoving || isChecking || !removalAvailability.available}
          >
            {isRemoving ? "Removing…" : isChecking ? "Checking git…" : `Remove from ${scopeLabel}`}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

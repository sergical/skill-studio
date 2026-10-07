// ============================================================================
// TurnOffForAgentDialog - confirms "Turn off for <Agent>" from a row under the
// shared folder. It asks the backend first: a skill dotagents manages, or an
// agent linked to the whole folder, gets the refusal reason and an "Off
// everywhere" way out instead of the confirm. One Activity event holds the undo.
// ============================================================================

import { useState } from "react";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@skill-studio/ui";
import { useSkillTurnOff } from "../../hooks/useSkillTurnOff";
import { useAppStore } from "../../store/appStore";
import { gitWarningText } from "./skill-location-helpers";
import { turnOffConfirmText, turnOffSuccessMessage, turnOffView } from "./skill-agent-off-model";
import type { TurnOffAction } from "./skill-agent-off-model";

interface TurnOffForAgentDialogProps {
  skillName: string;
  request: TurnOffAction;
  onClose: () => void;
  /** Parks the shared copy for every agent: the existing Park action, with its own check and confirm. */
  onOffEverywhere: (request: TurnOffAction) => void;
}

export function TurnOffForAgentDialog({
  skillName,
  request,
  onClose,
  onOffEverywhere,
}: TurnOffForAgentDialogProps) {
  const addToast = useAppStore((state) => state.addToast);
  const [isRunning, setIsRunning] = useState(false);
  const { agent, agentLabel, target } = request;
  const { check, turnOff } = useSkillTurnOff(target, agent);

  const handleTurnOff = () => {
    setIsRunning(true);
    turnOff()
      .then(() => {
        addToast({
          type: "success",
          title: `${skillName} is off for ${agentLabel}`,
          message: turnOffSuccessMessage(agentLabel),
        });
        onClose();
      })
      .catch((err) => {
        addToast({
          type: "error",
          title: `Couldn't turn off for ${agentLabel}`,
          message: err instanceof Error ? err.message : String(err),
        });
      })
      .finally(() => setIsRunning(false));
  };

  const view = check ? turnOffView(check) : null;
  // Git only matters for a project copy: the move shows as deleted files in its repository.
  const gitWarning = check?.project ? gitWarningText(check.git_tracked, "parking") : null;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            Turn off {skillName} for {agentLabel}?
          </DialogTitle>
          <DialogDescription>
            {view === null
              ? "Checking…"
              : view.kind === "refused"
                ? view.reason
                : turnOffConfirmText(skillName, agentLabel)}
          </DialogDescription>
          {view?.kind === "confirm" && gitWarning && (
            <p className="m-0 text-small text-text-secondary">{gitWarning}</p>
          )}
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={isRunning}>
            Cancel
          </Button>
          {view?.kind === "refused" ? (
            view.offEverywhere && (
              <Button onClick={() => onOffEverywhere(request)}>Off everywhere</Button>
            )
          ) : (
            <Button onClick={handleTurnOff} disabled={isRunning || view === null}>
              {isRunning ? "Turning off…" : `Turn off for ${agentLabel}`}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ============================================================================
// Skill Studio - Assistant view policy
// Controls history navigation within the skill improvement view.
// ============================================================================

import { useState } from "react";

type SkillAssistantPanelMode = "assistant" | "runs";
type SkillAssistantPanelAction =
  | "open-assistant"
  | "close-assistant"
  | "open-runs"
  | "close-runs"
  | "change-skill";

/** Selects the Assistant panel mode after one workspace or Runs navigation action. */
export function nextSkillAssistantPanelMode(
  current: SkillAssistantPanelMode,
  action: SkillAssistantPanelAction,
): SkillAssistantPanelMode {
  switch (action) {
    case "open-runs":
      return "runs";
    case "open-assistant":
    case "close-assistant":
    case "close-runs":
    case "change-skill":
      return "assistant";
    default:
      return current;
  }
}

/** Owns Assistant workspace and improvement history navigation, resetting the nested view when its skill changes. */
export function useSkillAssistantNavigation(
  skillName: string | undefined,
  setIsAssistantOpen: (isOpen: boolean) => void,
) {
  const [panelMode, setPanelMode] = useState<SkillAssistantPanelMode>("assistant");
  const [previousSkillName, setPreviousSkillName] = useState(skillName);

  if (previousSkillName !== skillName) {
    // react-doctor-disable-next-line react-doctor/no-adjust-state-on-prop-change -- adjust-during-render, per React docs "storing information from previous renders"
    setPreviousSkillName(skillName);
    if (panelMode === "runs") {
      setPanelMode((current) => nextSkillAssistantPanelMode(current, "change-skill"));
    }
  }

  const transitionPanel = (action: SkillAssistantPanelAction) => {
    setPanelMode((current) => nextSkillAssistantPanelMode(current, action));
  };

  return {
    isRunsOpen: panelMode === "runs",
    openAssistant: () => {
      transitionPanel("open-assistant");
      setIsAssistantOpen(true);
    },
    closeAssistant: () => {
      transitionPanel("close-assistant");
      setIsAssistantOpen(false);
    },
    openRuns: () => transitionPanel("open-runs"),
    closeRuns: () => transitionPanel("close-runs"),
  };
}

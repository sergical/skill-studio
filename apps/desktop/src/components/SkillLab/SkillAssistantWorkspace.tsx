import { useEffect, useRef, type ReactNode } from "react";
import { isFeatureEnabled } from "../../lib/feature-flags";
import { PageShell } from "../Shell/PageShell";
import { SkillLab } from "./SkillLab";

interface Props {
  open: boolean;
  skillName: string;
  skillPath: string;
  onBack: () => void;
  improveSkill?: ReactNode;
}

export function SkillAssistantWorkspace(props: Props) {
  return isFeatureEnabled("skill-assistant") ? <EnabledWorkspace {...props} /> : null;
}

function EnabledWorkspace({ open, skillName, skillPath, onBack, improveSkill }: Props) {
  const workspaceRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open) workspaceRef.current?.focus();
  }, [open]);
  return (
    <div
      ref={workspaceRef}
      id="skill-assistant-workspace"
      role="region"
      hidden={!open}
      aria-label={`Assistant for ${skillName}`}
      tabIndex={-1}
      className={open ? "flex min-h-0 flex-1 flex-col outline-none" : "hidden"}
    >
      <PageShell title="Assistant" parent={{ label: skillName, onClick: onBack }}>
        <SkillLab skillName={skillName} skillPath={skillPath} improveSkill={improveSkill} />
      </PageShell>
    </div>
  );
}

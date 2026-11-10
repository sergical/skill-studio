import { SkillAssistantWorkspace } from "../../components/SkillLab/SkillAssistantWorkspace";
import { recordedRun } from "./skill-lab-recording";

export function SkillLabPreview() {
  return (
    <main className="flex h-dvh flex-col text-text-primary">
      <SkillAssistantWorkspace
        open
        skillName="report-writer"
        skillPath={recordedRun.suite.skill_path}
        onBack={() => window.history.back()}
      />
    </main>
  );
}

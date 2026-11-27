// ============================================================================
// SettingsView - the app's own preferences: which agents Skill Studio shows
// and connects, the project folders discovery searches, which application
// "Open in editor" hands a skill folder to, telemetry, and the version.
// ============================================================================

import type { SkillSnapshot } from "@skill-studio/lib";
import { PageShell } from "../Shell/PageShell";
import { AgentsSection } from "./AgentsSection";
import { AppVersionCard } from "./AppVersionCard";
import { EditorCard } from "./EditorCard";
import { ProjectFoldersCard, useDiscoverySources } from "./ProjectFoldersCard";
import { TelemetryCard } from "./TelemetryCard";

interface SettingsViewProps {
  snapshot: SkillSnapshot | undefined;
}

export function SettingsView({ snapshot }: SettingsViewProps) {
  const discovery = useDiscoverySources();
  return (
    <PageShell title="Settings" width="narrow">
      <div className="flex flex-col gap-8">
        <AgentsSection sources={discovery.sources} onToggleSource={discovery.toggle} />
        <ProjectFoldersCard snapshot={snapshot} discovery={discovery} />
        <EditorCard />
        <TelemetryCard />
        <AppVersionCard />
      </div>
    </PageShell>
  );
}

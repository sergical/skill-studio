import { Bot } from "lucide-react";
import { Checkbox } from "@skill-studio/ui";
import type { AgentId } from "@skill-studio/lib";
import { changeHarnessEnabled, useDetectedHarnesses } from "../../hooks/useDetectedHarnesses";
import { HarnessIcon } from "../ui/HarnessIcon";
import { SettingsCard, SettingsError, SettingsRow } from "./SettingsCard";

export function DetectedAgentsCard() {
  const { report, choice, error, saving } = useDetectedHarnesses();
  const kept = new Set(choice?.kept ?? []);
  const rows = report?.harnesses.filter((row) => row.state !== "not_found") ?? [];
  return (
    <SettingsCard
      icon={Bot}
      title="Agents"
      description="Skill Studio shows skills for the agents you turn on. It checks for agents at launch; a new one stays off until you turn it on."
    >
      {error && <SettingsError message={error} />}
      {!report && !error && (
        <p role="status" className="m-0 text-small text-text-tertiary">
          Checking installed agents…
        </p>
      )}
      {rows.length > 0 && (
        <ul className="m-0 flex list-none flex-col p-0">
          {rows.map((row) => (
            <SettingsRow
              key={row.id}
              // SAFETY: detected rows are the first-class harnesses, whose ids are `AgentId` wire values.
              icon={<HarnessIcon harness={row.id as AgentId} size={14} />}
              label={row.display_name}
              asLabel
              status={
                row.executable_on_path === false && (
                  <span className="text-small text-text-tertiary">Not on PATH</span>
                )
              }
              control={
                <Checkbox
                  checked={kept.has(row.id)}
                  disabled={saving || choice === null}
                  onCheckedChange={(checked) => {
                    void changeHarnessEnabled(row.id, checked === true);
                  }}
                />
              }
            />
          ))}
        </ul>
      )}
    </SettingsCard>
  );
}

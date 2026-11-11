import { Bot } from "lucide-react";
import { Checkbox } from "@skill-studio/ui";
import { changeHarnessEnabled, useDetectedHarnesses } from "../../hooks/useDetectedHarnesses";
import { SettingsCard } from "./SettingsCard";

export function DetectedAgentsCard() {
  const { report, choice, error, saving } = useDetectedHarnesses();
  const kept = new Set(choice?.kept ?? []);
  return (
    <SettingsCard
      icon={<Bot size={16} />}
      title="Agents"
      description="Checked at launch. New agents stay off until you turn them on."
    >
      {error && <p role="alert">{error}</p>}
      {!report && !error && <p role="status">Checking installed agents…</p>}
      {report?.harnesses.map((row) =>
        row.state === "not_found" ? null : (
          <label key={row.id} className="flex items-center gap-2 text-body">
            <Checkbox
              checked={kept.has(row.id)}
              disabled={saving || choice === null}
              onCheckedChange={(checked) => {
                void changeHarnessEnabled(row.id, checked === true);
              }}
            />
            <span>{row.display_name}</span>
            {row.executable_on_path === false && (
              <span className="text-small text-text-tertiary">Not on PATH</span>
            )}
          </label>
        ),
      )}
    </SettingsCard>
  );
}

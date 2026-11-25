// ============================================================================
// AgentTabs - The two halves of the Agents place: what the agent sees of your
// skills (its skill list) and what it used (its recorded sessions). The tab
// keeps the chosen agent, so switching never resets the agent menu.
// ============================================================================

import { ToggleGroup, ToggleGroupItem } from "@skill-studio/ui";
import { singleSelectToggleValue } from "../../lib/single-select-toggle-group";
import { useAppStore } from "../../store/appStore";

type AgentTab = "listing" | "sessions";

export function AgentTabs({ tab, harness }: { tab: AgentTab; harness: string }) {
  const navigate = useAppStore((s) => s.setActiveView);
  return (
    <ToggleGroup
      variant="segmented"
      aria-label="Agent report"
      value={[tab]}
      onValueChange={(next) =>
        singleSelectToggleValue<AgentTab>(next, (kind) =>
          navigate({ kind, harness: harness || undefined }),
        )
      }
    >
      <ToggleGroupItem value="listing">What it sees</ToggleGroupItem>
      <ToggleGroupItem value="sessions">What it used</ToggleGroupItem>
    </ToggleGroup>
  );
}

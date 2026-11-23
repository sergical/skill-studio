import { Button } from "@skill-studio/ui";
import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { useClientIds } from "../../hooks/useClientRows";
import { HarnessIcon, harnessIdFromLabel } from "../ui/HarnessIcon";

export function HomeVisibility() {
  const navigate = useAppStore((s) => s.setActiveView);
  const clients = useClientIds();
  return (
    <section
      aria-label="Your clients"
      className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-border-subtle pb-3"
    >
      <h2 className="mr-1 text-small font-medium text-text-secondary">Your clients</h2>
      {clients?.map((client) => {
        const label = deploymentLabelFromAgentId(client);
        const id = harnessIdFromLabel(label);
        return (
          <Button
            key={client}
            variant="ghost"
            size="sm"
            className="gap-1.5"
            onClick={() => navigate({ kind: "listing", harness: client })}
          >
            {id && <HarnessIcon harness={id} size={14} />}
            {label}
          </Button>
        );
      })}
      {!clients?.length && (
        <Button size="sm" variant="ghost" onClick={() => navigate({ kind: "settings" })}>
          Choose clients in Settings
        </Button>
      )}
    </section>
  );
}

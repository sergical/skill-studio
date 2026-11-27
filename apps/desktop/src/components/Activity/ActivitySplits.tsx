// ============================================================================
// ActivitySplits - Who the 30 days of uses came from: one row per agent and,
// when the uses came in more than one way, one row per trigger. A row's bar
// is its share of the total, and pressing a row filters the page to it. An
// agent turned on in Settings with no uses in the data is named on one line
// as having no readable history, since the data can't tell "read, unused"
// from "nothing to read".
// ============================================================================

import type { ComponentProps, ReactNode } from "react";
import { deploymentLabelFromAgentId, formatCount, ranked, TRIGGERS } from "@skill-studio/lib";
import type {
  AgentId,
  Counts,
  HarnessFilter,
  SkillTrigger,
  TriggerFilter,
} from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { joinNames } from "../Home/home-todos";
import { HarnessIcon } from "../ui/HarnessIcon";
import { TooltipControl } from "../ui/TooltipControl";

export function PanelLabel({ children }: { children: ReactNode }) {
  return <h3 className="text-caption font-medium text-text-tertiary">{children}</h3>;
}

function ShareRow({
  icon,
  label,
  value,
  total,
  pressed,
  dimmed,
  onPress,
  ...rest
}: Omit<ComponentProps<typeof Button>, "value"> & {
  icon?: ReactNode;
  label: string;
  value: number;
  total: number;
  pressed: boolean;
  dimmed: boolean;
  onPress: () => void;
}) {
  const share = total > 0 ? value / total : 0;
  return (
    <Button
      {...rest}
      variant="ghost"
      size="sm"
      aria-pressed={pressed}
      className={`-mx-2 grid h-7 grid-cols-[minmax(0,1fr)_minmax(0,40%)_3.5rem] justify-stretch gap-3 px-2 font-normal aria-pressed:bg-bg-hover ${dimmed ? "opacity-50" : ""}`}
      onClick={onPress}
    >
      <span className="flex min-w-0 items-center gap-2 text-text-secondary">
        {icon}
        <span className="truncate">{label}</span>
      </span>
      <span className="h-1.5 overflow-hidden rounded-full bg-bg-tertiary" aria-hidden="true">
        <span
          className="block h-full rounded-full bg-data"
          style={{ width: `${Math.max(2, share * 100)}%` }}
        />
      </span>
      <span className="text-right text-small text-text-tertiary tabular-nums">
        {formatCount(value)}
      </span>
    </Button>
  );
}

/**
 * Every agent turned on in Settings. With uses from two or more agents, each gets a row with its
 * share; with one, the row would be a full bar that says nothing, so it is left out.
 */
export function AgentSplit({
  agents,
  counts,
  readable,
  harness,
  setHarness,
}: {
  /** Agents with a usage reader that are turned on in Settings, in catalog order. */
  agents: AgentId[];
  /** Uses per agent under the trigger filter. */
  counts: Counts<string>;
  /** Uses per agent under no trigger filter: an agent missing here has no readable history. */
  readable: Counts<string>;
  harness: HarnessFilter;
  setHarness: (next: HarnessFilter) => void;
}) {
  // SAFETY: harness ids come off the wire as plain strings; every one this page can show is a
  // known AgentId.
  const used = ranked(counts) as [AgentId, number][];
  const total = used.reduce((sum, [, n]) => sum + n, 0);
  const silent = agents.filter((id) => !readable[id]);
  return (
    <div className="flex flex-col gap-1.5">
      <PanelLabel>Agents</PanelLabel>
      {used.length > 1 ? (
        <div className="flex flex-col">
          {used.map(([id, n]) => (
            <ShareRow
              key={id}
              icon={<HarnessIcon harness={id} size={14} />}
              label={deploymentLabelFromAgentId(id)}
              value={n}
              total={total}
              pressed={harness === id}
              dimmed={harness !== "all" && harness !== id}
              onPress={() => setHarness(harness === id ? "all" : id)}
            />
          ))}
        </div>
      ) : (
        used.map(([id]) => (
          <span key={id} className="flex h-7 items-center gap-2 text-body text-text-secondary">
            <HarnessIcon harness={id} size={14} />
            {deploymentLabelFromAgentId(id)}
          </span>
        ))
      )}
      {silent.length > 0 && (
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-small text-text-tertiary">
          <span className="flex items-center gap-1" aria-hidden="true">
            {silent.map((id) => (
              <HarnessIcon key={id} harness={id} size={12} muted />
            ))}
          </span>
          No readable history from {joinNames(silent.map(deploymentLabelFromAgentId))}
        </p>
      )}
    </div>
  );
}

/** The trigger split, only when uses came in more than one way; one trigger is no split. */
export function TriggerSplit({
  counts,
  trigger,
  setTrigger,
}: {
  counts: Counts<SkillTrigger>;
  trigger: TriggerFilter;
  setTrigger: (next: TriggerFilter) => void;
}) {
  const rows = TRIGGERS.flatMap((t) => (counts[t.id] ? [{ ...t, n: counts[t.id] ?? 0 }] : []));
  if (rows.length < 2) return null;
  const total = rows.reduce((sum, row) => sum + row.n, 0);
  return (
    <div className="flex flex-col gap-1.5">
      <PanelLabel>Trigger</PanelLabel>
      <div className="flex flex-col">
        {rows.map((row) => (
          <TooltipControl key={row.id} content={row.hint}>
            <ShareRow
              label={row.label}
              value={row.n}
              total={total}
              pressed={trigger === row.id}
              dimmed={trigger !== "all" && trigger !== row.id}
              onPress={() => setTrigger(trigger === row.id ? "all" : row.id)}
            />
          </TooltipControl>
        ))}
      </div>
    </div>
  );
}

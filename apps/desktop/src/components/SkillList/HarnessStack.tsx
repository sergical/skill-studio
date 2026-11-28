// ============================================================================
// HarnessStack - the avatar-group stack: the agents that read the skill, from
// their own folder or the Universal folder, overlap into one shape, and past
// `MAX_DISCS` the rest fold into a "+N" disc.
// ============================================================================

import type { ReactNode } from "react";
import {
  agentIdFromDeploymentLabel,
  isUnresolvedDeployment,
  skillVisibleToAgent,
} from "@skill-studio/lib";
import type { InstalledSkill } from "@skill-studio/lib";
import { useClientIds } from "../../hooks/useClientRows";
import { RichTooltip } from "../ui/RichTooltip";
import { HarnessGlyph, HarnessMark } from "./HarnessMark";
import { whereFacts } from "./skill-row-state";
import type { HarnessListEntry, HarnessReach } from "./skill-row-state";

/** Past this many discs, the rest fold into a "+N" disc. */
const MAX_DISCS = 5;

/** The "+N" disc's tooltip: one line per hidden harness, glyph and label only - no how word,
 * since the row already shows how. */
function OverflowTooltip({ overflow }: { overflow: HarnessReach[] }) {
  return (
    <div className="flex flex-col gap-1 text-small">
      {overflow.map((reach) => (
        <span key={reach.harness} className="flex items-center gap-1.5 text-text-tertiary">
          <HarnessGlyph harness={reach.harness} size={14} />
          {reach.label}
        </span>
      ))}
    </div>
  );
}

/** One disc in the stack: the avatar-group convention - `size-5 rounded-full
 * bg-bg-secondary`, ringed in the page's own background colour so the ring is
 * what visually separates neighbours rather than a border against the fill,
 * `-ml-1` (4px) overlap after the first so the group reads as one shape until
 * a disc is singled out on hover, and `group-hover:ring-bg-secondary` so the
 * row hover doesn't leave the ring looking detached from its fill. A disabled
 * disc only mutes its glyph - `HarnessMark` already does that, so the disc
 * itself needs no separate disabled treatment. */
function Disc({ first, children }: { first: boolean; children: ReactNode }) {
  return (
    <span
      className={`relative inline-flex size-5 items-center justify-center rounded-full bg-bg-secondary ring-2 ring-bg-primary group-hover:ring-bg-secondary hover:z-10 ${
        first ? "" : "-ml-1"
      }`}
    >
      {children}
    </span>
  );
}

/** The agents that read `skill`, among the ones the app shows: an agent with a copy, link or live
 * plugin copy in its own folder keeps that deployment's how, and one that reads it only through
 * the Universal folder shows as reading the Universal folder's source. */
function readingHarnesses(
  skill: InstalledSkill,
  harnessList: HarnessListEntry[],
  shown: string[] | undefined,
): HarnessReach[] {
  const { harnesses } = whereFacts(skill, harnessList);
  const shownSet = shown && new Set(shown);
  return harnesses.flatMap((reach): HarnessReach[] => {
    if (shownSet && !shownSet.has(reach.harness)) return [];
    switch (skillVisibleToAgent(skill, reach.harness)) {
      case "own":
        return [reach];
      case "shared":
        return [{ ...reach, reached: true, how: "universal", disabled: false }];
      case "none":
        return readsThroughPlugin(skill, reach.harness) ? [reach] : [];
    }
  });
}

/** A plugin copy the agent loads: `skillVisibleToAgent` counts only the skill's own global and
 * project deployments. A plugin cache copy can come without its plugin's details. */
function readsThroughPlugin(skill: InstalledSkill, harness: HarnessReach["harness"]): boolean {
  if (skill.parked) return false;
  return skill.deployments.some(
    (d) =>
      (d.plugin || d.scope === "plugin") &&
      agentIdFromDeploymentLabel(d.agent) === harness &&
      !d.disabled &&
      !isUnresolvedDeployment(d),
  );
}

/** One disc per agent that reads the skill, up to `MAX_DISCS`, then a "+N" overflow disc. */
export function HarnessStack({
  skill,
  harnessList,
}: {
  skill: InstalledSkill;
  harnessList: HarnessListEntry[];
}) {
  const readers = readingHarnesses(skill, harnessList, useClientIds());
  if (readers.length === 0) {
    return <span className="text-small text-text-tertiary">—</span>;
  }
  const shown = readers.slice(0, MAX_DISCS);
  const overflow = readers.slice(MAX_DISCS);
  const discs: ReactNode[] = shown.map((reach, index) => (
    <Disc key={reach.harness} first={index === 0}>
      <HarnessMark reach={reach} size={11} />
    </Disc>
  ));
  if (overflow.length > 0) {
    discs.push(
      <Disc key="overflow" first={false}>
        <RichTooltip content={<OverflowTooltip overflow={overflow} />}>
          <span className="text-caption tabular-nums text-text-tertiary">
            +{overflow.length}
            <span className="sr-only">: {overflow.map((reach) => reach.label).join(", ")}</span>
          </span>
        </RichTooltip>
      </Disc>,
    );
  }
  return <span className="inline-flex items-center">{discs}</span>;
}

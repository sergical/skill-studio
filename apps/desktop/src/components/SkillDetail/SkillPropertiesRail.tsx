// ============================================================================
// SkillPropertiesRail - The skill page's right-hand facts that no other part
// of the page shows: Lifecycle, token counts, Installed, Modified. The source sits
// under the title, the locations and agents have their own sections.
// ============================================================================

import type { ReactNode } from "react";
import { formatTokens } from "@skill-studio/lib";
import type { InstalledSkill, PluginInfo } from "@skill-studio/lib";
import { buildInstalledSkillSourceLedgerModel } from "./installed-skill-source-ledger-model";

interface SkillPropertiesRailProps {
  skill: InstalledSkill;
  selectedPlugin?: PluginInfo | null;
}

/** One property row: a 96px label column and a value column, min height 28px. */
function PropertyRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid min-h-7 grid-cols-[96px_minmax(0,1fr)] items-baseline gap-1">
      <dt className="text-small text-text-tertiary">{label}</dt>
      <dd className="m-0 min-w-0 select-text text-small text-text-primary">{children}</dd>
    </div>
  );
}

export function SkillPropertiesRail({ skill, selectedPlugin }: SkillPropertiesRailProps) {
  const ledger = buildInstalledSkillSourceLedgerModel(skill, selectedPlugin);
  return (
    <aside aria-label="Properties" className="sticky top-5 flex min-w-0 flex-col self-start">
      <dl className="m-0 flex flex-col gap-1">
        <PropertyRow label="Lifecycle">
          {ledger.lifecycleOwner} · {ledger.lifecycleManagement}
        </PropertyRow>
        <PropertyRow label="Skill list">
          <span className="tabular-nums">{formatTokens(skill.description_tokens)} tokens</span>
        </PropertyRow>
        <PropertyRow label="SKILL.md">
          <span className="tabular-nums">{formatTokens(skill.skill_md_tokens)} tokens</span>
        </PropertyRow>
        <PropertyRow label="Installed">{ledger.installed}</PropertyRow>
        {ledger.lastModified && <PropertyRow label="Modified">{ledger.lastModified}</PropertyRow>}
      </dl>
    </aside>
  );
}

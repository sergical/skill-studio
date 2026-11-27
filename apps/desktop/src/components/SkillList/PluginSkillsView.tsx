import { FirstScanState, type FirstScanStateProps } from "../SkillList/FirstScanState";
// ============================================================================
// PluginSkillsView - Skills shipped inside agent plugin caches, grouped by
// the plugin that ships them. A separate place from Skills: plugin skills
// update with their plugin and follow the plugin spec, so they get no
// install, edit, or bulk affordances here.
// ============================================================================

import { formatTokens, groupPluginSkills } from "@skill-studio/lib";
import type { SkillSnapshot } from "@skill-studio/lib";
import { Button } from "@skill-studio/ui";
import { PageShell } from "../Shell/PageShell";
import { HarnessIcon, harnessIdFromLabel } from "../ui/HarnessIcon";

interface PluginSkillsViewProps extends FirstScanStateProps {
  snapshot: SkillSnapshot | undefined;
  onSelectSkill: (name: string, deploymentPath?: string) => void;
}

export function PluginSkillsView({
  snapshot,
  onSelectSkill,
  isLoading,
  error,
  onRetry,
}: PluginSkillsViewProps) {
  const groups = groupPluginSkills(snapshot?.skills ?? []);
  const skillCount = new Set(groups.flatMap((g) => g.skills.map(({ skill }) => skill.name))).size;
  const statsBySkill = new Map((snapshot?.invocations ?? []).map((stats) => [stats.skill, stats]));

  if (!snapshot) {
    return (
      <PageShell title="Plugins">
        <FirstScanState isLoading={isLoading} error={error} onRetry={onRetry} />
      </PageShell>
    );
  }

  return (
    <PageShell
      title="Plugins"
      subtitle="Skills inside agent plugins. Each plugin installs and updates its own."
    >
      {groups.length === 0 ? (
        <p className="m-0 text-small text-text-tertiary">
          No agent plugin ships skills on this machine.
        </p>
      ) : (
        <div className="flex flex-col gap-7">
          <span className="sr-only">{skillCount} plugin skills</span>
          {groups.map((group) => {
            const harnessId = harnessIdFromLabel(group.harness);
            const skills = [...group.skills].sort((a, b) =>
              a.skill.name.localeCompare(b.skill.name),
            );
            return (
              <section
                key={`${group.harness}-${group.pluginName}`}
                aria-label={`${group.pluginName} for ${group.harness}`}
                className="flex flex-col gap-2"
              >
                <header className="flex items-baseline gap-2 px-3">
                  {harnessId && (
                    <span className="self-center">
                      <HarnessIcon harness={harnessId} size={14} />
                    </span>
                  )}
                  <h2 className="m-0 text-body font-semibold text-text-primary">
                    {group.pluginName}
                  </h2>
                  <span className="text-caption text-text-secondary">{group.harness}</span>
                  {group.version && (
                    <span className="text-caption text-text-tertiary">v{group.version}</span>
                  )}
                  {skills.length > 1 && (
                    <span className="ml-auto text-caption tabular-nums text-text-tertiary">
                      {skills.length} skills
                    </span>
                  )}
                </header>
                <div className="flex flex-col">
                  {skills.map(({ skill, deploymentPath }) => {
                    // Only the plugin's own agent runs this copy; another agent's uses of a
                    // same-named skill are not this plugin's.
                    const uses =
                      (harnessId && statsBySkill.get(skill.name)?.by_harness_30_days[harnessId]) ||
                      0;
                    return (
                      <Button
                        key={skill.name}
                        variant="ghost"
                        className="grid h-8 min-w-0 justify-start gap-3 overflow-hidden rounded-none border-0 border-b border-border-subtle px-3 text-left font-normal last:border-b-0 [grid-template-columns:minmax(0,1fr)_minmax(0,2fr)_64px_80px]"
                        onClick={() => onSelectSkill(skill.name, deploymentPath)}
                      >
                        <span className="truncate text-body text-text-primary">{skill.name}</span>
                        <span className="select-text truncate text-small text-text-tertiary">
                          {skill.description}
                        </span>
                        {/* No recorded use shows nothing, not "0": the agent may keep no history
                            Skill Studio can read. */}
                        <span className="whitespace-nowrap text-right text-small tabular-nums text-text-secondary">
                          {uses > 0 && (
                            <>
                              {uses.toLocaleString()}
                              <span className="text-text-tertiary">
                                {" "}
                                {uses === 1 ? "use" : "uses"}
                              </span>
                            </>
                          )}
                        </span>
                        <span className="whitespace-nowrap text-right text-small tabular-nums text-text-secondary">
                          {formatTokens(skill.skill_md_tokens)}
                          <span className="text-text-tertiary"> tokens</span>
                        </span>
                      </Button>
                    );
                  })}
                </div>
              </section>
            );
          })}
        </div>
      )}
    </PageShell>
  );
}

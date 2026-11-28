import { FirstScanState, type FirstScanStateProps } from "../SkillList/FirstScanState";
// ============================================================================
// SkillsView - The unified, filterable skill list. Replaces SkillsScopeView,
// PluginSkillsView, SkillCoverageView, and SkillIssuesView: the sidebar holds
// places, this view holds filters (scope, harness, source, attention, query).
// ============================================================================

import { useState } from "react";
import { PageShell } from "../Shell/PageShell";
import { SkillCoverageMatrix } from "../Coverage/SkillCoverageMatrix";
import { ScanPartialBanner } from "./ScanPartialBanner";
import { SkillListTable } from "./SkillListTable";
import { deploymentForScope } from "./skill-list-deployment";
import type { SortMode } from "../../lib/skill-list-sort";
import { SkillListActiveFilters, SkillListFilterBar } from "./SkillListFilterBar";
import { useProjectFolderActions } from "../../hooks/useProjectFolderActions";
import { useClientIds } from "../../hooks/useClientRows";
import { attentionBySkill, attentionCounts, collectDashboardIssues } from "@skill-studio/lib";
import { applySkillListFilter, isProjectScope, skillForCoverageScope } from "@skill-studio/lib";
import { ownSkillsView, ownSkillsWithPluginUpdates, usesIn30DaysBySkill } from "@skill-studio/lib";
import type { InstalledSkill, SkillSnapshot } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";

interface SkillsViewProps extends FirstScanStateProps {
  snapshot: SkillSnapshot | undefined;
  onSelectSkill: (name: string, deploymentPath?: string) => void;
  /** Whether Skills is the view on screen right now - `false` while it's kept mounted but hidden
   * behind an open skill's page, so its window-level keyboard shortcuts stay off. */
  active: boolean;
}

/**
 * The unified, filterable skill list. Holds no filter state of its own -
 * scope, harness, source, attention, query, and the coverage toggle all live in
 * the store's `skillListFilter`/`showCoverage`, so opening a skill and
 * coming back never loses them.
 */
export function SkillsView({
  snapshot,
  onSelectSkill,
  active,
  isLoading,
  error,
  onRetry,
}: SkillsViewProps) {
  const filter = useAppStore((state) => state.skillListFilter);
  const setSkillListFilter = useAppStore((state) => state.setSkillListFilter);
  const resetSkillListFilter = useAppStore((state) => state.resetSkillListFilter);
  const showCoverage = useAppStore((state) => state.showCoverage);
  const setShowCoverage = useAppStore((state) => state.setShowCoverage);
  const [sort, setSort] = useState<SortMode>("name");
  const selectedSkillName = useAppStore((state) =>
    state.activeView.kind === "skill" ? state.activeView.name : null,
  );
  const lastClosedSkillName = useAppStore((state) => state.lastClosedSkillName);
  const userAddedProjects = useAppStore((state) => state.userAddedProjects);
  const excludedProjects = useAppStore((state) => state.excludedProjects);
  const openAddSkillSheet = useAppStore((state) => state.openAddSkillSheet);
  const { addProject, stopTracking } = useProjectFolderActions();
  const kept = useClientIds();

  const excludedProjectSet = new Set(excludedProjects);
  const projects = Array.from(
    new Set([...userAddedProjects, ...(snapshot?.projects ?? [])]),
  ).filter((path) => !excludedProjectSet.has(path));

  const allSkills = snapshot?.skills ?? [];
  // Plugin-shipped skills live in their own place (PluginSkillsView); this
  // list is always the user's own skills.
  const baseSkills = ownSkillsView(allSkills);
  const issues = collectDashboardIssues(baseSkills);
  // Home counts plugin-only skills with an update, so every Updates count here lists them too.
  const withPluginUpdates = ownSkillsWithPluginUpdates(allSkills);
  const listedSkills = withPluginUpdates;
  const attention = attentionBySkill(withPluginUpdates, issues);
  const matches = applySkillListFilter(listedSkills, filter, issues, snapshot?.invocations, kept);
  // Each category's count under every other filter, so a pick says how many it will list.
  const attentionTotals = attentionCounts(
    attention,
    applySkillListFilter(
      withPluginUpdates,
      { ...filter, attention: undefined },
      issues,
      snapshot?.invocations,
      kept,
    ),
  );
  const listedByName = new Map(listedSkills.map((skill) => [skill.name, skill]));
  const allByName = new Map(allSkills.map((skill) => [skill.name, skill]));
  const deploymentPathForSkill = ({ name }: InstalledSkill) => {
    const skill = listedByName.get(name);
    return skill && deploymentForScope(skill, filter.scope, filter.harness);
  };

  /** Adds a project via the shared hook, then switches the scope to it. */
  const handleAddProject = async () => {
    const added = await addProject();
    if (added) setSkillListFilter({ scope: { project: added } });
  };

  const title = filter.scope === "parked" ? "Parked" : "Skills";

  if (!snapshot) {
    return (
      <PageShell title={title}>
        <FirstScanState isLoading={isLoading} error={error} onRetry={onRetry} />
      </PageShell>
    );
  }

  return (
    <PageShell
      title={title}
      toolbar={
        <SkillListFilterBar
          filter={filter}
          onChange={setSkillListFilter}
          projects={projects}
          onAddProject={handleAddProject}
          onRemoveProject={stopTracking}
          showCoverage={showCoverage}
          onToggleCoverage={setShowCoverage}
          resultCount={matches.length}
          attentionCounts={attentionTotals}
          sort={sort}
          onSortChange={setSort}
          snapshot={snapshot}
        />
      }
    >
      <SkillListActiveFilters
        filter={filter}
        onChange={setSkillListFilter}
        onReset={resetSkillListFilter}
      />
      {snapshot?.scan_partial && (
        <ScanPartialBanner
          observations={snapshot.scan_observations}
          unreadLocationCount={snapshot.unread_roots.length}
        />
      )}
      {showCoverage ? (
        <SkillCoverageMatrix
          skills={matches.map((skill) => skillForCoverageScope(skill, filter.scope))}
          onSelectSkill={onSelectSkill}
          deploymentPathForSkill={deploymentPathForSkill}
        />
      ) : (
        <SkillListTable
          scope={filter.scope}
          skills={matches}
          usesBySkill={usesIn30DaysBySkill(snapshot.invocations, kept)}
          attention={attention}
          attentionFilter={filter.attention}
          sort={sort}
          onSelectSkill={onSelectSkill}
          selectedSkillName={selectedSkillName}
          initialCursorSkillName={lastClosedSkillName}
          active={active}
          deploymentPathForSkill={deploymentPathForSkill}
          packProjectPath={isProjectScope(filter.scope) ? filter.scope.project : null}
          hasAnySkills={listedSkills.length > 0}
          onClearFilters={resetSkillListFilter}
          onAddSkill={() => openAddSkillSheet()}
          resetKey={JSON.stringify(filter)}
          latestSkill={(name) => listedByName.get(name)}
          readerSkill={(name) => allByName.get(name)}
        />
      )}
    </PageShell>
  );
}

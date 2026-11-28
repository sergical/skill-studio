// ============================================================================
// SkillBrowser - Grid/list of skills from search results
// ============================================================================

import { Check, ArrowUp, Link2, FileCheck2, AlertTriangle } from "lucide-react";
import { Button } from "@skill-studio/ui";
import type { SkillWithStatus } from "@skill-studio/lib";
import { SOURCE_KIND_LABELS, specViolationSeverity } from "@skill-studio/lib";
import { formatInstalls } from "../../lib/skill-installs-format";

/**
 * The Browse tab pages through skills.sh and shows an install indicator per
 * card; the Installed tab always shows everything it has (no pagination) and
 * hides that indicator, since every card there is already installed. One
 * discriminated prop keeps those two states from combining into invalid
 * on/off pairs (e.g. `isLoadingMore` true while `hideInstalledIndicator` is
 * also true) that no call site ever actually produces.
 */
type SkillBrowserMode =
  | {
      kind: "browse";
      isLoading: boolean;
      isLoadingMore: boolean;
      hasMore: boolean;
      onLoadMore: () => void;
    }
  | { kind: "installed" };

interface SkillBrowserProps {
  skills: SkillWithStatus[];
  selectedSkill: SkillWithStatus | null;
  onSelectSkill: (skill: SkillWithStatus) => void;
  emptyMessage: string;
  mode: SkillBrowserMode;
}

export function SkillBrowser({
  skills,
  selectedSkill,
  onSelectSkill,
  emptyMessage,
  mode,
}: SkillBrowserProps) {
  const isLoading = mode.kind === "browse" && mode.isLoading;
  const hideInstalledIndicator = mode.kind === "installed";

  if (isLoading && skills.length === 0) {
    return (
      <div className="flex h-[200px] flex-col items-center justify-center text-pretty text-body text-text-tertiary">
        <span className="mb-3 size-6 animate-spin rounded-full border-2 border-border border-t-accent" />
        <p>Searching skills.sh…</p>
      </div>
    );
  }

  if (skills.length === 0) {
    return (
      <div className="flex h-[200px] flex-col items-center justify-center text-pretty text-body text-text-tertiary">
        <p>{emptyMessage}</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-scroll">
      <ul className="m-0 flex list-none flex-col p-0">
        {skills.map((skill) => (
          <li key={mode.kind === "installed" ? skill.name : skill.id}>
            <SkillCard
              skill={skill}
              isSelected={selectedSkill?.id === skill.id}
              onClick={() => onSelectSkill(skill)}
              hideInstalledIndicator={hideInstalledIndicator}
            />
          </li>
        ))}
      </ul>
      {mode.kind === "browse" && mode.hasMore && (
        <div className="flex justify-center py-4">
          <Button
            variant="outline"
            className="h-auto rounded-md border-border bg-bg-secondary px-6 py-2.5 text-body font-medium text-text-secondary hover:border-border-focus hover:bg-bg-tertiary hover:text-text-primary"
            onClick={mode.onLoadMore}
            disabled={mode.isLoadingMore}
          >
            {mode.isLoadingMore ? (
              <>
                <span className="size-3.5 animate-spin rounded-full border-2 border-border border-t-accent" />
                Loading…
              </>
            ) : (
              "Load more"
            )}
          </Button>
        </div>
      )}
    </div>
  );
}

interface SkillCardProps {
  skill: SkillWithStatus;
  isSelected: boolean;
  onClick: () => void;
  hideInstalledIndicator: boolean | undefined;
}

/** The installed indicator badge - an update arrow when an owning source has
 * an update pending, else a plain checkmark. `null` when the card doesn't
 * mark installs at all (the Browse tab's own indicator). */
function InstalledBadge({ skill }: { skill: SkillWithStatus }) {
  if (!skill.is_installed) return null;
  const hasUpdate = (skill.installed_info?.update_owner_ids.length ?? 0) > 0;
  return hasUpdate ? (
    <span
      className="flex size-[18px] items-center justify-center rounded-full bg-warning-soft text-warning"
      title="Update available"
    >
      <ArrowUp size={12} />
    </span>
  ) : (
    <span
      className="flex size-[18px] items-center justify-center rounded-full bg-success-soft text-success"
      title="Installed"
    >
      <Check size={12} />
    </span>
  );
}

/** The Installed tab's row of provenance/spec/deployment tags - hidden on
 * the Browse tab, where a card's own `installed_info` (if any) isn't the
 * point of the card. */
function InstalledSkillTags({
  installedInfo,
}: {
  installedInfo: NonNullable<SkillWithStatus["installed_info"]>;
}) {
  const specProblems = installedInfo.spec_violations.filter(
    (v) => specViolationSeverity(v) !== "note",
  );
  return (
    <div className="flex flex-wrap items-center gap-1">
      <span
        className={`inline-flex w-fit items-center rounded-sm px-1.5 py-0.5 text-caption tracking-[0.04em] uppercase ${sourceKindClass(installedInfo.source_kind)}`}
      >
        {SOURCE_KIND_LABELS[installedInfo.source_kind]}
      </span>
      {installedInfo.has_spec && (
        <span
          className="inline-flex items-center gap-[3px] rounded-sm bg-success-soft px-1.5 py-0.5 text-caption text-success"
          title="Ships behavior specs/evals"
        >
          <FileCheck2 size={11} />
          spec
        </span>
      )}
      {specProblems.length > 0 && (
        <span
          className="inline-flex items-center gap-[3px] rounded-sm bg-warning-soft px-1.5 py-0.5 text-caption text-warning"
          title={specProblems.join("\n")}
        >
          <AlertTriangle size={11} />
          spec issues
        </span>
      )}
      {installedInfo.deployments.map((deployment) => (
        <span
          key={deployment.path}
          className="inline-flex items-center gap-[3px] rounded-sm bg-bg-tertiary px-1.5 py-0.5 text-caption text-text-secondary"
        >
          {deployment.is_symlink && <Link2 size={10} />}
          {deployment.plugin
            ? `${deployment.agent} · via ${deployment.plugin.name}`
            : `${deployment.agent} · ${deployment.scope}`}
        </span>
      ))}
    </div>
  );
}

function SkillCard({ skill, isSelected, onClick, hideInstalledIndicator }: SkillCardProps) {
  const isInstalledMarked = skill.is_installed && !hideInstalledIndicator;
  const description = skill.description || skill.installed_info?.description;

  return (
    <Button
      variant="ghost"
      className={`h-auto w-full flex-col items-stretch justify-start gap-1 rounded-none border-0 border-b border-border px-5 py-3 text-left whitespace-normal ${isSelected ? "bg-accent-softer" : "hover:bg-bg-hover"}`}
      onClick={onClick}
    >
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-body font-medium text-text-primary">{skill.name}</span>
        {isInstalledMarked && <InstalledBadge skill={skill} />}
        {hasInstallCount(skill) && (
          <span className="ml-auto shrink-0 text-caption font-normal tabular-nums text-text-tertiary">
            {formatInstalls(skill.installs)} installs
          </span>
        )}
      </span>

      {skill.top_source && (
        <span className="truncate font-mono text-caption font-normal text-text-tertiary">
          {skill.top_source}
        </span>
      )}

      {hideInstalledIndicator && skill.installed_info && (
        <InstalledSkillTags installedInfo={skill.installed_info} />
      )}

      {description && (
        <span
          className="line-clamp-2 text-pretty text-small font-normal text-text-secondary"
          title={description}
        >
          {description}
        </span>
      )}
    </Button>
  );
}

/** skills.sh counts installs only for skills it lists; a local skill has no count. */
function hasInstallCount(skill: SkillWithStatus): boolean {
  return Number.isFinite(skill.installs);
}

/** skills.sh: accent; dotagents: info; plugin: warning; in-repo/manual: tertiary; fork: success. */
function sourceKindClass(kind: string): string {
  switch (kind) {
    case "skills-sh":
      return "text-accent bg-accent-softer";
    case "dotagents":
      return "text-info bg-info-soft";
    case "plugin":
      return "text-warning bg-warning-soft";
    case "fork":
      return "text-success bg-success-soft";
    default:
      return "text-text-tertiary bg-bg-tertiary";
  }
}

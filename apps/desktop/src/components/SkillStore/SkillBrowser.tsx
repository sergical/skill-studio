// ============================================================================
// SkillBrowser - Grid/list of skills from search results
// ============================================================================

import { Check, ArrowUp } from "lucide-react";
import { Button } from "@skill-studio/ui";
import type { SkillWithStatus } from "@skill-studio/lib";
import { formatInstalls } from "../../lib/skill-installs-format";

interface SkillBrowserProps {
  skills: SkillWithStatus[];
  selectedSkill: SkillWithStatus | null;
  onSelectSkill: (skill: SkillWithStatus) => void;
  emptyMessage: string;
  isLoading: boolean;
  isLoadingMore: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
}

export function SkillBrowser({
  skills,
  selectedSkill,
  onSelectSkill,
  emptyMessage,
  isLoading,
  isLoadingMore,
  hasMore,
  onLoadMore,
}: SkillBrowserProps) {
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
          <li key={skill.id}>
            <SkillCard
              skill={skill}
              isSelected={selectedSkill?.id === skill.id}
              onClick={() => onSelectSkill(skill)}
            />
          </li>
        ))}
      </ul>
      {hasMore && (
        <div className="flex justify-center py-4">
          <Button
            variant="outline"
            className="h-auto rounded-md border-border bg-bg-secondary px-6 py-2.5 text-body font-medium text-text-secondary hover:border-border-focus hover:bg-bg-tertiary hover:text-text-primary"
            onClick={onLoadMore}
            disabled={isLoadingMore}
          >
            {isLoadingMore ? (
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
}

/** The installed indicator badge - an update arrow when an owning source has
 * an update pending, else a plain checkmark. `null` for a skill not installed here. */
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

function SkillCard({ skill, isSelected, onClick }: SkillCardProps) {
  const description = skill.description || skill.installed_info?.description;

  return (
    <Button
      variant="ghost"
      className={`h-auto w-full flex-col items-stretch justify-start gap-1 rounded-none border-0 border-b border-border px-5 py-3 text-left whitespace-normal ${isSelected ? "bg-accent-softer" : "hover:bg-bg-hover"}`}
      onClick={onClick}
    >
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-body font-medium text-text-primary">{skill.name}</span>
        <InstalledBadge skill={skill} />
        <span className="ml-auto shrink-0 text-caption font-normal tabular-nums text-text-tertiary">
          {formatInstalls(skill.installs)} installs
        </span>
      </span>

      {skill.top_source && (
        <span className="truncate font-mono text-caption font-normal text-text-tertiary">
          {skill.top_source}
        </span>
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

// ============================================================================
// SkillDetailPanel - Detail view for a selected skill
// ============================================================================

import type { ReactNode } from "react";
import { Button, DrawerContent } from "@skill-studio/ui";
import { SkillDetailHeader } from "./SkillDetailHeader";
import { SkillContent } from "./SkillContent";
import { InstallControls } from "./InstallControls";
import { useLoad } from "../../hooks/useLoad";
import { useAppStore } from "../../store/appStore";
import type { SkillWithStatus } from "@skill-studio/lib";

interface SkillDetailPanelProps {
  skill: SkillWithStatus;
  inventoryNotice?: ReactNode;
  onClose: () => void;
  onInstallStart: (skillName: string) => void;
  onInstallPaused: () => void;
  onInstallComplete: (result: {
    success: boolean;
    error?: string;
    skillName?: string;
    warning?: string;
  }) => void;
  onUpdateComplete: () => void;
  onRemoveComplete: () => void;
}

export function SkillDetailPanel(props: SkillDetailPanelProps) {
  // GitHub owner/repository casing can change when the catalog joins a local record.
  const key = props.skill.id.replace(/^[^/]+\/[^/]+(?=\/)/, (repo) => repo.toLowerCase());
  return <SkillDetailsContent key={key} {...props} />;
}

function SkillDetailsContent({
  skill,
  inventoryNotice,
  onClose,
  onInstallStart,
  onInstallPaused,
  onInstallComplete,
  onUpdateComplete,
  onRemoveComplete,
}: SkillDetailPanelProps) {
  const hasRemoteSource = skill.id.split("/").length >= 3;
  const getSkillDetails = useAppStore((state) => state.getSkillDetails);
  // One load per panel: the panel is keyed by the skill, so a new skill mounts a new one. An id
  // that only changes its letter case keeps the result it has; Retry reads with the current id.
  const detailsLoad = useLoad(hasRemoteSource ? "skill-details" : null, () =>
    getSkillDetails(skill.id),
  );
  // A retry hides the last failure until its own answer lands.
  const loadFailure = !hasRemoteSource
    ? "This skill has no skills.sh source."
    : detailsLoad.isLoading
      ? null
      : detailsLoad.error;
  const details = detailsLoad.data ?? null;
  const resolvedTopSource = details?.source ?? skill.top_source ?? null;
  const openAddSkillSheet = useAppStore((state) => state.openAddSkillSheet);

  return (
    <DrawerContent
      side="right"
      className="w-[min(640px,92vw)] overflow-y-scroll bg-bg-secondary"
      showCloseButton={false}
    >
      <SkillDetailHeader skill={skill} resolvedTopSource={resolvedTopSource} onClose={onClose} />
      {loadFailure !== null && (
        <div className="mx-5 mb-3 flex items-center justify-between gap-3">
          <p role="alert" className="m-0 text-small text-error">
            {loadFailure}
          </p>
          {hasRemoteSource && (
            <Button variant="outline" size="sm" onClick={detailsLoad.reload}>
              Retry
            </Button>
          )}
        </div>
      )}
      <SkillContent
        skill={skill}
        skillMd={details?.skill_md ?? null}
        isLoading={detailsLoad.isLoading}
      />
      <Button
        variant="link"
        className="mx-5 h-auto self-start p-0 text-small"
        onClick={() =>
          openAddSkillSheet(resolvedTopSource ? `${resolvedTopSource}/${skill.name}` : skill.name)
        }
      >
        Add with more options…
      </Button>
      <div className="my-2 h-px bg-border" />
      {inventoryNotice ?? (
        <InstallControls
          skill={skill}
          resolvedTopSource={resolvedTopSource}
          onInstallStart={onInstallStart}
          onInstallPaused={onInstallPaused}
          onInstallComplete={onInstallComplete}
          onUpdateComplete={onUpdateComplete}
          onRemoveComplete={onRemoveComplete}
        />
      )}
    </DrawerContent>
  );
}

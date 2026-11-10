// ============================================================================
// SkillDetailPanel - Detail view for a selected skill
// ============================================================================

import { useEffect, useState } from "react";
import { Button, DrawerContent } from "@skill-studio/ui";
import { SkillDetailHeader } from "./SkillDetailHeader";
import { SkillContent } from "./SkillContent";
import { InstallControls } from "./InstallControls";
import { getSkillDetails } from "../../lib/skill-api";
import { useAppStore } from "../../store/appStore";
import type { SkillDetails, SkillWithStatus } from "@skill-studio/lib";

interface SkillDetailPanelProps {
  skill: SkillWithStatus;
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

type DetailsLoad =
  | { kind: "loading" }
  | { kind: "loaded"; details: SkillDetails }
  | { kind: "failed"; message: string };

export function SkillDetailPanel(props: SkillDetailPanelProps) {
  return <SkillDetailsContent key={props.skill.id} {...props} />;
}

function SkillDetailsContent({
  skill,
  onClose,
  onInstallStart,
  onInstallPaused,
  onInstallComplete,
  onUpdateComplete,
  onRemoveComplete,
}: SkillDetailPanelProps) {
  const hasRemoteSource = skill.id.split("/").length >= 3;
  const [load, setLoad] = useState<DetailsLoad>(() =>
    hasRemoteSource
      ? { kind: "loading" }
      : { kind: "failed", message: "This skill has no skills.sh source." },
  );
  const details = load.kind === "loaded" ? load.details : null;
  const resolvedTopSource = details?.source ?? skill.top_source ?? null;
  const openAddSkillSheet = useAppStore((state) => state.openAddSkillSheet);
  useEffect(() => {
    if (load.kind !== "loading") return;
    let cancelled = false;
    getSkillDetails(skill.id).then(
      (details) => {
        if (!cancelled) setLoad({ kind: "loaded", details });
      },
      (error) => {
        if (!cancelled)
          setLoad({
            kind: "failed",
            message: error instanceof Error ? error.message : String(error),
          });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [load.kind, skill.id]);

  return (
    <DrawerContent
      side="right"
      className="w-[min(640px,92vw)] overflow-y-scroll bg-bg-secondary"
      showCloseButton={false}
    >
      <SkillDetailHeader skill={skill} resolvedTopSource={resolvedTopSource} onClose={onClose} />
      {load.kind === "failed" && (
        <div className="mx-5 mb-3 flex items-center justify-between gap-3">
          <p role="alert" className="m-0 text-small text-error">
            {load.message}
          </p>
          {hasRemoteSource && (
            <Button variant="outline" size="sm" onClick={() => setLoad({ kind: "loading" })}>
              Retry
            </Button>
          )}
        </div>
      )}
      <SkillContent
        skill={skill}
        skillMd={details?.skill_md ?? null}
        isLoading={load.kind === "loading"}
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
      <InstallControls
        skill={skill}
        resolvedTopSource={resolvedTopSource}
        onInstallStart={onInstallStart}
        onInstallPaused={onInstallPaused}
        onInstallComplete={onInstallComplete}
        onUpdateComplete={onUpdateComplete}
        onRemoveComplete={onRemoveComplete}
      />
    </DrawerContent>
  );
}

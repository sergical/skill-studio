// ============================================================================
// UpdateAllButton - "Update all" in the Skills list's Updates group header.
// While it runs, the group's rows stay put: the one being updated spins, the
// rest wait, and each done row says "Updated" until the list is laid out
// again. The same button then says Stop.
// ============================================================================

import { Button, cn } from "@skill-studio/ui";
import type { InstalledSkill } from "@skill-studio/lib";
import { useUpdateAll } from "../../hooks/useUpdateAll";
import { UpdateOverwritesEditsDialog } from "../SkillDetail/UpdateOverwritesEditsDialog";

interface UpdateAllButtonProps {
  /** The skills that still have an update: what Update all runs on. */
  updates: readonly InstalledSkill[];
}

export function UpdateAllButton({ updates }: UpdateAllButtonProps) {
  const { activeBatch, isCheckingEdits, start, stop, editsPrompt } = useUpdateAll(updates);
  const progress = activeBatch?.progress ?? null;
  const isStopping = activeBatch?.isStopping ?? false;

  return (
    <>
      <Button
        variant="outline"
        size="xs"
        busy={isCheckingEdits || isStopping ? true : undefined}
        disabled={!activeBatch && updates.length === 0}
        onClick={(e) => {
          e.stopPropagation();
          void (activeBatch ? stop() : start());
        }}
      >
        {/* Both labels share one cell, so the header keeps one width whichever shows. */}
        <span className="grid">
          <span
            className={cn("col-start-1 row-start-1", activeBatch && "invisible")}
            aria-hidden={activeBatch ? true : undefined}
          >
            Update all
          </span>
          <span
            className={cn("col-start-1 row-start-1", !activeBatch && "invisible")}
            aria-hidden={activeBatch ? undefined : true}
          >
            Stop
          </span>
        </span>
      </Button>
      <span role="status" className="sr-only">
        {progress && progress.total > 0 ? `Updating ${progress.done} of ${progress.total}` : ""}
      </span>
      <UpdateOverwritesEditsDialog
        skillNames={editsPrompt.skillNames}
        isBulk
        canFork={editsPrompt.canFork}
        unforkableSkillNames={editsPrompt.unforkableSkillNames}
        onFork={editsPrompt.fork}
        onOverwrite={editsPrompt.overwrite}
        onCancel={editsPrompt.cancel}
      />
    </>
  );
}

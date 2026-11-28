// ============================================================================
// PackExportDialog - The selection bar's Pack, in three steps: a dry run
// lists each skill and the files the new pack folder would hold, Export
// writes it, and the result gives the install line to share once the folder
// is pushed. The core plans and writes; this dialog only shows the reports.
// ============================================================================

import { useEffect, useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Input,
} from "@skill-studio/ui";
import type { PackExportReport } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { CopyTextButton } from "../ui/CopyTextButton";
import { advancePlanPhase } from "./pack-plan-phase";
import type { PackPlanEvent, PackPlanState } from "./pack-plan-phase";

type ExportState = PackPlanState<PackExportReport>;

const canRun = (plan: PackExportReport) => plan.files.length > 0;

function after(event: PackPlanEvent<PackExportReport>) {
  return (current: ExportState): ExportState => {
    const move = advancePlanPhase(current, event, canRun);
    return move.ok ? move.state : current;
  };
}

interface PackExportDialogProps {
  /** Names of the skills to put in the pack, in list order. */
  skills: string[];
  /** The new pack folder the user chose. */
  dir: string;
  /** Take each skill's copy from this project; null for the home. */
  projectPath: string | null;
  onClose: () => void;
}

export function PackExportDialog({ skills, dir, projectPath, onClose }: PackExportDialogProps) {
  const exportPack = useAppStore((s) => s.exportPack);
  const [state, setState] = useState<ExportState>({ phase: "planning" });
  const [source, setSource] = useState("");

  useEffect(() => {
    let live = true;
    exportPack({ dir, skills, source: null, project_path: projectPath, dry_run: true })
      .then((plan) => live && setState(after({ type: "planned", plan })))
      .catch(
        (err) =>
          live &&
          setState(
            after({ type: "error", message: err instanceof Error ? err.message : String(err) }),
          ),
      );
    return () => {
      live = false;
    };
  }, [exportPack, dir, skills, projectPath]);

  const confirm = () => {
    setState(after({ type: "confirm" }));
    const trimmed = source.trim();
    exportPack({
      dir,
      skills,
      source: trimmed === "" ? null : trimmed,
      project_path: projectPath,
      dry_run: false,
    })
      .then((result) => setState(after({ type: "ran", result })))
      .catch((err) =>
        setState(
          after({ type: "error", message: err instanceof Error ? err.message : String(err) }),
        ),
      );
  };

  const running = state.phase === "running";
  const report =
    state.phase === "planned" || state.phase === "running"
      ? state.plan
      : state.phase === "done"
        ? state.result
        : null;
  const finished = state.phase === "done" || state.phase === "failed";

  return (
    <AlertDialog open busy={running} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent className="sm:max-w-lg">
        <AlertDialogHeader>
          <AlertDialogTitle>
            {state.phase === "done"
              ? "Pack written"
              : `Export ${skills.length} skill${skills.length === 1 ? "" : "s"} as a pack?`}
          </AlertDialogTitle>
          <AlertDialogDescription role="status">
            {state.phase === "planning"
              ? "Checking what the pack would hold…"
              : state.phase === "failed"
                ? state.message
                : report?.summary}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {report && (
          <ul
            aria-label="Skills in the pack"
            className="m-0 max-h-56 list-none overflow-y-auto p-0"
          >
            {report.skills.map((skill) => (
              <li
                key={skill.skill}
                className="flex items-baseline gap-2 border-b border-border-subtle py-1.5 last:border-b-0"
              >
                <span className="min-w-0 truncate text-body text-text-primary">{skill.skill}</span>
                <span className="ml-auto shrink-0 text-caption text-text-tertiary tabular-nums">
                  {skill.files} file{skill.files === 1 ? "" : "s"}
                </span>
              </li>
            ))}
          </ul>
        )}
        {state.phase === "planned" && (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              confirm();
            }}
          >
            <label className="flex flex-col gap-1 text-small text-text-secondary">
              Repository for the install line
              <Input
                placeholder="owner/repo"
                value={source}
                onChange={(event) => setSource(event.target.value)}
              />
            </label>
          </form>
        )}
        {state.phase === "done" && (
          <div className="flex flex-col gap-1">
            <span className="text-small text-text-secondary">
              Push the folder to git, then anyone can install the pack with:
            </span>
            <div className="flex items-center gap-2">
              <code
                aria-label="Install command"
                className="min-w-0 flex-1 truncate rounded-sm bg-bg-tertiary px-2 py-1 text-small text-text-primary"
                title={state.result.install_command}
              >
                {state.result.install_command}
              </code>
              <CopyTextButton
                text={state.result.install_command}
                label="Copy"
                namesText
                className="rounded-sm px-2 text-text-secondary"
              />
            </div>
          </div>
        )}
        <AlertDialogFooter>
          {finished ? (
            <AlertDialogCancel onClick={onClose}>Close</AlertDialogCancel>
          ) : (
            <>
              <AlertDialogCancel onClick={onClose} disabled={running}>
                Cancel
              </AlertDialogCancel>
              <AlertDialogAction
                onClick={confirm}
                busy={running}
                disabled={state.phase !== "planned" || !canRun(state.plan)}
              >
                Export
              </AlertDialogAction>
            </>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

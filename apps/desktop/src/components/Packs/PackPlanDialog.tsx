// ============================================================================
// PackPlanDialog - One pack action, in three steps: a dry run shows each
// skill the action would change or skip (with the reason), Confirm runs it,
// and the result lists what each step did. The core plans and runs the
// action; this dialog only shows the reports.
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
} from "@skill-studio/ui";
import type { PackActionReport, PackStep } from "@skill-studio/lib";
import { invokeErrorMessage, packAdd, packRemove, packUpdate } from "../../lib/skill-api";
import { advancePackPlan, plannedCount } from "./pack-plan-phase";
import type { PackPlanEvent, PackPlanState } from "./pack-plan-phase";

export type PackIntent =
  | { kind: "update"; source: string | null }
  | { kind: "remove"; source: string }
  | { kind: "add"; source: string; skills: string[] };

function runIntent(intent: PackIntent, dryRun: boolean): Promise<PackActionReport> {
  switch (intent.kind) {
    case "update":
      return packUpdate({
        source: intent.source,
        all: intent.source === null,
        project_path: null,
        dry_run: dryRun,
      });
    case "remove":
      return packRemove({ source: intent.source, project_path: null, dry_run: dryRun });
    case "add":
      return packAdd({
        source: intent.source,
        skills: intent.skills,
        ref_pin: null,
        harnesses: [],
        project_path: null,
        dry_run: dryRun,
      });
  }
}

const VERB: Record<PackIntent["kind"], string> = {
  update: "Update",
  remove: "Remove",
  add: "Add",
};

function title(intent: PackIntent): string {
  if (intent.kind === "update" && intent.source === null) return "Update every pack?";
  if (intent.kind === "add") return `Add skills from ${intent.source}?`;
  return `${VERB[intent.kind]} ${intent.source}?`;
}

const PLANNED_LABEL: Record<PackStep["action"], string> = {
  install: "Will install",
  update: "Will update",
  remove: "Will remove",
};

const DONE_LABEL: Record<PackStep["action"], string> = {
  install: "Installed",
  update: "Updated",
  remove: "Removed",
};

function scopeLabel(step: PackStep): string {
  return step.scope.scope === "global"
    ? "Global"
    : (step.scope.project.split("/").pop() ?? step.scope.project);
}

function StepRow({ step }: { step: PackStep }) {
  const { state } = step;
  const label =
    state.state === "planned"
      ? PLANNED_LABEL[step.action]
      : state.state === "done"
        ? DONE_LABEL[step.action]
        : state.state === "skipped"
          ? "Skipped"
          : "Failed";
  const reason =
    state.state === "skipped" || state.state === "failed" ? state.reason : null;
  return (
    <li className="flex flex-col gap-0.5 border-b border-border-subtle py-1.5 last:border-b-0">
      <div className="flex items-baseline gap-2">
        <span className="min-w-0 truncate text-body text-text-primary">{step.skill}</span>
        <span className="text-caption text-text-secondary">{scopeLabel(step)}</span>
        <span
          className={`ml-auto shrink-0 text-caption ${state.state === "failed" ? "text-error" : "text-text-secondary"}`}
        >
          {label}
        </span>
      </div>
      {reason && <span className="text-small text-text-secondary">{reason}</span>}
    </li>
  );
}

interface PackPlanDialogProps {
  intent: PackIntent;
  onClose: () => void;
}

export function PackPlanDialog({ intent, onClose }: PackPlanDialogProps) {
  const [state, setState] = useState<PackPlanState>({ phase: "planning" });
  const send = (event: PackPlanEvent) =>
    setState((current) => {
      const move = advancePackPlan(current, event);
      return move.ok ? move.state : current;
    });

  useEffect(() => {
    let live = true;
    runIntent(intent, true)
      .then((plan) => live && send({ type: "planned", plan }))
      .catch((err) => live && send({ type: "error", message: invokeErrorMessage(err) }));
    return () => {
      live = false;
    };
  }, [intent]);

  const confirm = () => {
    send({ type: "confirm" });
    runIntent(intent, false)
      .then((result) => send({ type: "ran", result }))
      .catch((err) => send({ type: "error", message: invokeErrorMessage(err) }));
  };

  const running = state.phase === "running";
  const report =
    state.phase === "planned" || state.phase === "running"
      ? state.plan
      : state.phase === "done"
        ? state.result
        : null;
  const toRun = state.phase === "planned" || running ? plannedCount(state.plan) : 0;
  const finished = state.phase === "done" || state.phase === "failed";

  return (
    <AlertDialog open busy={running} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent className="sm:max-w-lg">
        <AlertDialogHeader>
          <AlertDialogTitle>{state.phase === "done" ? "Done" : title(intent)}</AlertDialogTitle>
          <AlertDialogDescription role="status">
            {state.phase === "planning"
              ? "Checking what would change…"
              : state.phase === "failed"
                ? state.message
                : report?.summary}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {report && report.steps.length > 0 && (
          <ul aria-label="Steps" className="m-0 max-h-72 list-none overflow-y-auto p-0">
            {report.steps.map((step) => (
              <StepRow
                key={`${step.skill}:${step.scope.scope === "global" ? "global" : step.scope.project}`}
                step={step}
              />
            ))}
          </ul>
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
                variant={intent.kind === "remove" ? "destructive" : "default"}
                onClick={confirm}
                busy={running}
                disabled={toRun === 0}
              >
                {VERB[intent.kind]}
                {toRun > 0 && ` ${toRun} skill${toRun === 1 ? "" : "s"}`}
              </AlertDialogAction>
            </>
          )}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

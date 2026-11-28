// ============================================================================
// PackPlanDialog - One pack action, in three steps: a dry run shows each
// skill the action would change or skip (with the reason), Confirm runs it,
// and the result lists what each step did. The core plans and runs the
// action; this dialog only shows the reports.
// ============================================================================

import { useState } from "react";
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
import { distinctProjectNames } from "../../lib/skill-lifecycle-target";
import { useLoad } from "../../hooks/useLoad";
import { useAppStore } from "../../store/appStore";
import type { PackIntent } from "../../store/appStore";
import { advancePackPlan, plannedCount } from "./pack-plan-phase";
import type { PackPlanEvent, PackPlanState } from "./pack-plan-phase";

function after(event: PackPlanEvent) {
  return (current: PackPlanState): PackPlanState => {
    const move = advancePackPlan(current, event);
    return move.ok ? move.state : current;
  };
}

const VERB = {
  update: "Update",
  remove: "Remove",
  add: "Add",
} satisfies Record<PackIntent["kind"], string>;

function title(intent: PackIntent): string {
  if (intent.kind === "update" && intent.source === null) return "Update every pack?";
  if (intent.kind === "add") return `Add skills from ${intent.source}?`;
  return `${VERB[intent.kind]} ${intent.source}?`;
}

const PLANNED_WORD = {
  install: "to install",
  update: "to update",
  remove: "to remove",
  switch_source: "to switch",
} satisfies Record<PackStep["action"], string>;

const DONE_WORD = {
  install: "installed",
  update: "updated",
  remove: "removed",
  switch_source: "switched",
} satisfies Record<PackStep["action"], string>;

/** Failed steps first, the ones that ran or will run next, skipped ones last. */
const STATE_ORDER = {
  failed: 0,
  planned: 1,
  done: 1,
  skipped: 2,
} satisfies Record<PackStep["state"]["state"], number>;

const scopeKey = (step: PackStep) =>
  step.scope.scope === "global" ? "global" : step.scope.project;

/**
 * Each scope in the plan by a short name: "Global", or the fewest last
 * folders of a project path that no other project in the plan shares, so
 * /work/client/app and /work/personal/app read "client/app" and "personal/app".
 */
function scopeLabels(steps: PackStep[]): Map<string, string> {
  const projects = [...new Set(steps.map(scopeKey))].filter((key) => key !== "global");
  return new Map([["global", "Global"], ...distinctProjectNames(projects)]);
}

const stepKey = (step: PackStep) => `${step.source}:${step.skill}:${scopeKey(step)}`;

/** The report as counts, naming only states that have steps: "3 to update · 2 skipped". */
function countLine(report: PackActionReport): string {
  const count = (state: PackStep["state"]["state"]) =>
    report.steps.filter((step) => step.state.state === state).length;
  const action = report.steps[0]?.action ?? "update";
  const parts = [
    count("failed") > 0 && `${count("failed")} failed`,
    count("planned") > 0 && `${count("planned")} ${PLANNED_WORD[action]}`,
    count("done") > 0 && `${count("done")} ${DONE_WORD[action]}`,
    count("skipped") > 0 && `${count("skipped")} skipped`,
  ].filter(Boolean);
  return parts.length === 0 ? "Nothing to change." : parts.join(" · ");
}

const reasonOf = (step: PackStep) =>
  step.state.state === "skipped" || step.state.state === "failed" ? step.state.reason : null;

/** Steps in list order, with a skipped or failed run that shares one reason kept together. */
function groupSteps(steps: PackStep[]): PackStep[][] {
  const sorted = [...steps].sort(
    (a, b) =>
      STATE_ORDER[a.state.state] - STATE_ORDER[b.state.state] ||
      (reasonOf(a) ?? "").localeCompare(reasonOf(b) ?? ""),
  );
  const groups: PackStep[][] = [];
  for (const step of sorted) {
    const group = groups[groups.length - 1];
    const reason = reasonOf(step);
    if (
      group &&
      reason &&
      group[0].state.state === step.state.state &&
      reasonOf(group[0]) === reason
    ) {
      group.push(step);
    } else {
      groups.push([step]);
    }
  }
  return groups;
}

interface StepGroupProps {
  /** One step, or skipped or failed steps that share a reason, which shows once. */
  steps: PackStep[];
  /** Each step's scope by its short name; null when the plan has only one scope. */
  scopeLabels: Map<string, string> | null;
  /** Show each step's pack: only when the plan spans more than one. */
  showSource: boolean;
}

function StepGroup({ steps, scopeLabels, showSource }: StepGroupProps) {
  const state = steps[0].state.state;
  const reason = reasonOf(steps[0]);
  return (
    <li className="flex flex-col gap-0.5 border-b border-border-subtle py-1.5 last:border-b-0">
      {steps.map((step, index) => {
        const where = [showSource && step.source, scopeLabels?.get(scopeKey(step))].filter(Boolean);
        return (
          <div key={stepKey(step)} className="flex min-w-0 items-baseline gap-2">
            <span
              className={`min-w-0 truncate text-body ${reason ? "text-text-secondary" : "text-text-primary"}`}
            >
              {step.skill}
            </span>
            {where.length > 0 && (
              <span
                className="min-w-0 truncate text-caption text-text-tertiary"
                title={step.scope.scope === "project" ? step.scope.project : undefined}
              >
                {where.join(" · ")}
              </span>
            )}
            {reason && index === 0 && (
              <span
                className={`ml-auto shrink-0 text-caption ${state === "failed" ? "text-error" : "text-text-secondary"}`}
              >
                {state === "failed" ? "Failed" : "Skipped"}
              </span>
            )}
          </div>
        );
      })}
      {reason && <span className="text-small text-text-tertiary">{reason}</span>}
    </li>
  );
}

interface PackPlanDialogProps {
  intent: PackIntent;
  onClose: () => void;
}

export function PackPlanDialog({ intent, onClose }: PackPlanDialogProps) {
  const [state, setState] = useState<PackPlanState>({ phase: "planning" });
  const runPackAction = useAppStore((s) => s.runPackAction);

  useLoad(JSON.stringify(intent), () => runPackAction(intent, true), {
    onLoaded: (plan) => setState(after({ type: "planned", plan })),
    onFailed: (message) => setState(after({ type: "error", message })),
  });

  const confirm = () => {
    setState(after({ type: "confirm" }));
    runPackAction(intent, false)
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
  const toRun = state.phase === "planned" || running ? plannedCount(state.plan) : 0;
  const finished = state.phase === "done" || state.phase === "failed";
  const steps = report ? report.steps : [];
  const groups = groupSteps(steps);
  const showScope = new Set(steps.map(scopeKey)).size > 1;
  const labels = showScope ? scopeLabels(steps) : null;
  const onlyScope = !showScope && steps[0] ? steps[0].scope : null;
  const showSource = new Set(steps.map((step) => step.source)).size > 1;

  return (
    <AlertDialog open busy={running} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent className="sm:max-w-lg">
        <AlertDialogHeader>
          <AlertDialogTitle>{state.phase === "done" ? "Done" : title(intent)}</AlertDialogTitle>
          <AlertDialogDescription role="status" className="tabular-nums">
            {state.phase === "planning"
              ? "Checking what would change…"
              : state.phase === "failed"
                ? state.message
                : report && countLine(report)}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {onlyScope && (
          <p
            className="m-0 min-w-0 truncate text-small text-text-secondary"
            title={onlyScope.scope === "global" ? undefined : onlyScope.project}
          >
            {onlyScope.scope === "global" ? "Global" : `Project ${onlyScope.project}`}
          </p>
        )}
        {steps.length > 0 && (
          <ul aria-label="Steps" className="m-0 max-h-72 list-none overflow-y-auto p-0">
            {groups.map((group) => (
              <StepGroup
                key={stepKey(group[0])}
                steps={group}
                scopeLabels={labels}
                showSource={showSource}
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

import { useState } from "react";
import { Button, Textarea } from "@skill-studio/ui";
import type { RunRecord, Trial } from "@skill-studio/lib";
import { CheckboxControl } from "../ui/CheckboxControl";
import { activeRun, comparison } from "./lab-model";
import { LabPagination, useLabPage } from "./LabPagination";

interface Props {
  run: RunRecord;
  onCancel: () => void;
  onEdit: () => void;
  onExport: (suiteOnly: boolean) => void;
  onReview: (trialId: string, accepted: boolean, reason: string) => Promise<boolean>;
  onDelete: () => void;
  reviewDrafts: Record<string, string>;
  onReviewDraftChange: (trialId: string, reason: string) => void;
}

const label = (value: string) => value.replace(/_/g, " ");

export function RunResults({
  run,
  onCancel,
  onEdit,
  onExport,
  onReview,
  onDelete,
  reviewDrafts,
  onReviewDraftChange,
}: Props) {
  const [selectedId, setSelectedId] = useState(run.trials[0]?.id);
  const [blind, setBlind] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const selected = run.trials.find((trial) => trial.id === selectedId) ?? run.trials[0];
  const running = activeRun(run);
  const trialPage = useLabPage(run.trials);
  const variantName = (id: string) =>
    blind
      ? `Variant ${String.fromCharCode(65 + run.suite.variants.findIndex((item) => item.id === id))}`
      : (run.suite.variants.find((item) => item.id === id)?.name ?? id);
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-title font-semibold">{run.suite.name}</h2>
          <p className="text-body tabular-nums text-text-secondary" role="status">
            {label(run.state)} · {run.trials.filter((trial) => trial.state === "finished").length}{" "}
            of {run.trials.length} finished · {new Date(run.created_at).toLocaleString()}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {running ? (
            <Button variant="outline" onClick={onCancel}>
              Stop run
            </Button>
          ) : (
            <Button variant="outline" onClick={onEdit}>
              Edit and run again
            </Button>
          )}
          <Button variant="outline" onClick={() => onExport(false)}>
            Export results
          </Button>
        </div>
      </div>
      {run.error && (
        <p role="alert" className="rounded-md border border-border p-3 text-body">
          {run.error}
        </p>
      )}
      {run.trials.length > 1 && (
        <details className="rounded-md border border-border p-3">
          <summary className="cursor-pointer text-body font-medium">
            Compare results ({run.trials.length})
          </summary>
          <div className="space-y-4 pt-3">
            <p className="text-body text-text-secondary">
              A test passes only when its required checks pass. The pass count excludes runs that
              could not finish, checks with errors, and results that need review.
            </p>
            <label className="flex items-center gap-2 text-body">
              <CheckboxControl checked={blind} onCheckedChange={setBlind} />
              Hide variant names while reviewing. Output can still identify a skill.
            </label>
            <div className="overflow-x-auto rounded-md border border-border">
              <table className="w-full text-left text-body">
                <caption className="sr-only">Results by variant</caption>
                <thead className="bg-bg-secondary text-caption text-text-secondary">
                  <tr>
                    <th className="p-3">Variant</th>
                    <th className="p-3">Passed / evaluated</th>
                    <th className="p-3">Client completed</th>
                    <th className="p-3">Needs review</th>
                    <th className="p-3">Reported run cost</th>
                  </tr>
                </thead>
                <tbody>
                  {comparison(run).map((row) => (
                    <tr key={row.variant.id} className="border-t border-border">
                      <th className="p-3 font-medium">{variantName(row.variant.id)}</th>
                      <td className="p-3 tabular-nums">
                        {row.passed} / {row.evaluated}
                      </td>
                      <td className="p-3 tabular-nums">
                        {row.completed} / {row.total}
                      </td>
                      <td className="p-3 tabular-nums">{row.needsReview}</td>
                      <td className="p-3 tabular-nums">
                        {row.costs.length
                          ? `$${row.costs.reduce((sum, value) => sum + value, 0).toFixed(4)} (${row.costs.length}/${row.total} reported)`
                          : "Unknown"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="text-caption text-text-secondary">
              Costs above cover task runs. Judge usage is shown with each result. Repetitions show
              variability; a small sample does not establish a reliable improvement.
            </p>
          </div>
        </details>
      )}
      <div
        className={
          run.trials.length > 1
            ? "grid gap-4 min-[1100px]:grid-cols-[200px_minmax(0,1fr)]"
            : "space-y-4"
        }
      >
        {run.trials.length > 1 && (
          <nav className="space-y-1" aria-label="Trial results">
            {trialPage.items.map((trial) => (
              <Button
                key={trial.id}
                variant={selected?.id === trial.id ? "secondary" : "ghost"}
                className="h-auto w-full justify-start whitespace-normal py-3 text-left"
                aria-pressed={selected?.id === trial.id}
                onClick={() => setSelectedId(trial.id)}
              >
                <span>
                  <span className="block">
                    {run.suite.cases.find((item) => item.id === trial.case_id)?.name} ·{" "}
                    {variantName(trial.variant_id)}
                  </span>
                  <span className="block text-caption text-text-secondary">
                    Run {trial.repetition} ·{" "}
                    {trial.state === "finished" ? label(trial.outcome) : label(trial.state)}
                  </span>
                </span>
              </Button>
            ))}
            <LabPagination {...trialPage} label="Trial result pages" />
          </nav>
        )}
        {selected && (
          <TrialEvidence
            key={selected.id}
            trial={selected}
            blind={blind}
            canReview={!running}
            onReview={onReview}
            reason={reviewDrafts[selected.id] ?? ""}
            onReasonChange={(reason) => onReviewDraftChange(selected.id, reason)}
          />
        )}
      </div>
      {!blind && (
        <details className="text-caption text-text-secondary">
          <summary className="cursor-pointer">Reproduction details</summary>
          <p className="mt-2 break-all">Input hash: {run.input_hash}</p>
          <p>Run: {run.id}</p>
          <Button variant="ghost" onClick={() => onExport(true)}>
            Export suite
          </Button>
          <p>
            Inputs and skill content were frozen before launch. Editing and running again takes a
            new skill snapshot.
          </p>
        </details>
      )}
      {!running && (
        <div className="border-t border-border pt-4">
          {confirmDelete ? (
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-body">Delete this run and its saved evidence?</p>
              <Button variant="destructive" onClick={onDelete}>
                Delete run
              </Button>
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                Keep it
              </Button>
            </div>
          ) : (
            <Button variant="ghost" onClick={() => setConfirmDelete(true)}>
              Delete saved run…
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

function TrialEvidence({
  trial,
  blind,
  canReview,
  onReview,
  reason,
  onReasonChange,
}: {
  trial: Trial;
  blind: boolean;
  canReview: boolean;
  onReview: Props["onReview"];
  reason: string;
  onReasonChange: (reason: string) => void;
}) {
  const [saving, setSaving] = useState(false);
  const files = [...trial.artifacts].sort(
    (a, b) => Number(a.change === "unchanged") - Number(b.change === "unchanged"),
  );
  const changedFiles = files.filter((file) => file.change !== "unchanged").length;
  const review = (accepted: boolean) => {
    setSaving(true);
    return onReview(trial.id, accepted, reason)
      .then((acceptedReview) => {
        if (acceptedReview) onReasonChange("");
      })
      .finally(() => {
        setSaving(false);
      });
  };
  return (
    <section
      aria-label="Selected trial evidence"
      className="min-w-0 space-y-4 rounded-md border border-border p-4"
    >
      <div className="grid grid-cols-1 gap-3 text-body min-[600px]:grid-cols-3">
        <div>
          <p className="text-caption text-text-secondary">Client execution</p>
          <p>{trial.result ? label(trial.result.state) : label(trial.state)}</p>
        </div>
        <div>
          <p className="text-caption text-text-secondary">Task checks</p>
          <p>{label(trial.outcome)}</p>
        </div>
        <div>
          <p className="text-caption text-text-secondary">Skill loaded</p>
          <p>{trial.result?.skill_loaded ?? "unknown"}</p>
        </div>
      </div>
      {(trial.error || trial.result?.error) && (
        <p role="alert" className="rounded-md bg-bg-secondary p-3 text-body">
          {trial.error ?? trial.result?.error}
        </p>
      )}
      <details open>
        <summary className="cursor-pointer text-body font-medium">
          Files ({trial.artifacts.length})
        </summary>
        <div className="mt-2 space-y-2">
          {files.map((file) => (
            <details
              key={file.path}
              open={changedFiles === 1 && file.change !== "unchanged"}
              className="rounded-md bg-bg-secondary p-3"
            >
              <summary className="cursor-pointer text-body">
                {file.path} · {file.change} · {file.bytes.toLocaleString()} bytes
              </summary>
              <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-words text-caption">
                {file.text ??
                  "Text preview unavailable. The file is binary, deleted, or larger than the preview limit."}
              </pre>
            </details>
          ))}
          {trial.artifacts.length === 0 && (
            <p className="text-caption text-text-secondary">No saved output files yet.</p>
          )}
        </div>
      </details>
      {trial.checks.map((check) => (
        <details
          key={check.check_id}
          className="rounded-md border border-border p-3"
          open={check.status !== "passed"}
        >
          <summary className="cursor-pointer text-body font-medium">
            {label(check.status)} · {check.name}
          </summary>
          <p className="mt-2 text-body">{check.reason}</p>
          {check.evidence && (
            <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words text-caption text-text-secondary">
              {check.evidence}
            </pre>
          )}
        </details>
      ))}
      <details open={trial.artifacts.length === 0 && !!trial.result}>
        <summary className="cursor-pointer text-body font-medium">
          Final response and tool trace
        </summary>
        <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-words text-caption">
          {trial.result?.output || "No final response yet."}
        </pre>
        <ol className="mt-3 space-y-2">
          {toolTrace(trial).map(({ event, key }) => (
            <li key={key} className="rounded-md bg-bg-secondary p-2 text-caption">
              <pre className="overflow-auto whitespace-pre-wrap break-words">
                {JSON.stringify(event, null, 2)}
              </pre>
            </li>
          ))}
        </ol>
      </details>
      <details>
        <summary className="cursor-pointer text-body font-medium">Run details</summary>
        {!blind && (
          <p className="text-caption text-text-secondary">
            {trial.client_version ?? "Client has not started"}. {trial.isolation}
          </p>
        )}

        <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap text-caption">
          {JSON.stringify(
            {
              duration_ms: trial.result?.duration_ms,
              task_usage: trial.result?.usage,
              judge: trial.judge,
            },
            null,
            2,
          )}
        </pre>
      </details>
      {trial.reviews.map((item) => (
        <blockquote key={item.at} className="border-l-2 border-border pl-3 text-body">
          <p>
            {item.accepted ? "Accepted by reviewer" : "Rejected by reviewer"} ·{" "}
            {new Date(item.at).toLocaleString()}
          </p>
          <p className="text-text-secondary">{item.reason}</p>
        </blockquote>
      ))}
      {canReview && (
        <div className="space-y-2 border-t border-border pt-3">
          <h3 className="text-body font-medium">Add your review</h3>
          <p className="text-caption text-text-secondary">
            Your review is saved alongside the checks. It does not replace their result.
          </p>
          <Textarea
            aria-label="Review reason"
            placeholder="Explain what the evidence shows."
            value={reason}
            onChange={(event) => onReasonChange(event.target.value)}
          />
          <div className="flex gap-2">
            <Button
              disabled={saving || !reason.trim()}
              onClick={() => {
                void review(true);
              }}
            >
              Accept
            </Button>
            <Button
              disabled={saving || !reason.trim()}
              variant="ghost"
              onClick={() => {
                void review(false);
              }}
            >
              Reject
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

function toolTrace(trial: Trial) {
  return (
    trial.result?.events.flatMap((event, sequence) =>
      event.kind === "tool_call" || event.kind === "error"
        ? [{ event, key: `${trial.id}-${sequence}` }]
        : [],
    ) ?? []
  );
}

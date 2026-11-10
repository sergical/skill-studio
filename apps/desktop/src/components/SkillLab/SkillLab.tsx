import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button, Textarea } from "@skill-studio/ui";
import type { TestSuite } from "@skill-studio/lib";
import { useSkillLab, type LabTab } from "../../hooks/useSkillLab";
import { SelectControl } from "../ui/SelectControl";
import { newSuite } from "./lab-model";
import { RunResults } from "./RunResults";
import { SuiteEditor } from "./SuiteEditor";
import { jsonDraftErrors } from "./lab-json-drafts";
import { LabPagination, useLabPage } from "./LabPagination";
import "./skill-lab.css";

const TABS: LabTab[] = ["setup", "result", "history", "improve"];

export function SkillLab({
  skillName,
  skillPath,
  improveSkill,
}: {
  skillName: string;
  skillPath: string;
  improveSkill?: ReactNode;
}) {
  const lab = useSkillLab(skillName, skillPath);
  const [hasOpenedImprove, setHasOpenedImprove] = useState(false);
  const [suiteRevision, setSuiteRevision] = useState(0);
  const [reviewDrafts, setReviewDrafts] = useState<Record<string, Record<string, string>>>({});
  const resultRef = useRef<HTMLDivElement>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const {
    suite,
    clients,
    suites,
    history,
    run,
    tab,
    error,
    notice,
    loading,
    busy,
    importText,
    refresh,
    save,
    start,
    openRun,
    exportSuite,
  } = lab;
  const setSuite = (suite: TestSuite) => lab.update({ suite });
  const replaceSuite = (suite: TestSuite) => {
    lab.update({ suite, jsonDrafts: {} });
    setSuiteRevision((revision) => revision + 1);
  };
  const setTab = (tab: LabTab) => lab.update({ tab, notice: null });
  const setImportText = (importText: string | null) => lab.update({ importText });
  const invalidJson = jsonDraftErrors(suite, lab.jsonDrafts).length > 0;
  const historyPage = useLabPage(history);
  useEffect(() => {
    if (tab === "result" && run?.id) {
      resultRef.current?.focus({ preventScroll: true });
      workspaceRef.current?.scrollIntoView({ block: "start" });
    }
  }, [tab, run?.id]);

  return (
    <div ref={workspaceRef} className="skill-lab mx-auto w-full max-w-[840px] min-w-0 space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex gap-1" aria-label="Assistant views">
          {TABS.filter(
            (value) => (value !== "result" || run) && (value !== "improve" || improveSkill),
          ).map((value) => (
            <Button
              key={value}
              variant={tab === value ? "secondary" : "ghost"}
              aria-pressed={tab === value}
              onClick={() => {
                setTab(value);
                if (value === "improve") setHasOpenedImprove(true);
                if (value === "history") void refresh();
              }}
            >
              {value === "setup"
                ? "Task"
                : value === "history"
                  ? "Saved runs"
                  : value === "improve"
                    ? "Improve skill"
                    : "Result"}
            </Button>
          ))}
        </div>
      </div>
      {error && (
        <div role="alert" className="space-y-2 rounded-md border border-border p-3">
          <p className="text-body">{error}</p>
          <Button
            variant="outline"
            onClick={() => {
              void refresh();
            }}
          >
            Refresh clients and history
          </Button>
          <p className="text-caption text-text-secondary">
            After refreshing, repeat the action to try again.
          </p>
        </div>
      )}
      {notice && (
        <p role="status" className="text-body text-text-secondary">
          {notice}
        </p>
      )}
      <div hidden={tab !== "setup"}>
        <>
          {loading && clients.length === 0 ? (
            <p role="status" className="py-12 text-center text-text-secondary">
              Checking installed clients and saved tests…
            </p>
          ) : (
            <SuiteEditor
              key={`${suite.id}-${suiteRevision}`}
              savedTests={
                <>
                  <Button
                    variant="ghost"
                    onClick={() => {
                      void refresh();
                    }}
                    disabled={loading}
                  >
                    {loading ? "Checking clients…" : "Refresh clients"}
                  </Button>
                  <div className="flex flex-wrap gap-2">
                    <SelectControl
                      size="lg"
                      ariaLabel="Saved suite"
                      value={suites.some((item) => item.id === suite.id) ? suite.id : "new"}
                      items={[
                        { value: "new", label: "New suite" },
                        ...suites.map((item) => ({ value: item.id, label: item.name })),
                      ]}
                      onValueChange={(value) =>
                        replaceSuite(
                          suites.find((item) => item.id === value) ??
                            newSuite(skillName, skillPath),
                        )
                      }
                    />
                    <Button
                      variant="ghost"
                      onClick={() => setImportText(importText === null ? "" : null)}
                    >
                      Import suite
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={busy || invalidJson}
                      onClick={() => {
                        void exportSuite();
                      }}
                    >
                      Export suite
                    </Button>
                  </div>
                  {importText !== null && (
                    <div className="space-y-2 rounded-md border border-border p-3">
                      <p className="text-body">
                        Paste a Skill Studio suite. Import checks the format. Clients and acceptance
                        commands start only when you choose Run.
                      </p>
                      <Textarea
                        aria-label="Suite JSON to import"
                        rows={8}
                        value={importText}
                        onChange={(event) => setImportText(event.target.value)}
                        className="font-mono text-caption"
                      />
                      <Button
                        disabled={busy || !importText.trim()}
                        onClick={() => {
                          void lab.importSuite(importText);
                        }}
                      >
                        Import
                      </Button>
                    </div>
                  )}
                </>
              }
              suite={suite}
              clients={clients}
              busy={busy}
              jsonDrafts={lab.jsonDrafts}
              onJsonChange={(key, text) => {
                const jsonDrafts = { ...lab.jsonDrafts };
                if (text === undefined) delete jsonDrafts[key];
                else jsonDrafts[key] = text;
                lab.update({ jsonDrafts });
              }}
              onChange={setSuite}
              onSave={() => {
                void save();
              }}
              onRun={() => {
                void start();
              }}
            />
          )}
        </>
      </div>
      {hasOpenedImprove && <div hidden={tab !== "improve"}>{improveSkill}</div>}
      {tab === "history" && (
        <div className="space-y-2">
          <h2 className="text-emphasis font-semibold">Saved runs</h2>
          {history.length === 0 ? (
            <p className="py-10 text-center text-body text-text-secondary">
              No saved runs for this skill yet. Describe a task in the Task view to create your
              first run.
            </p>
          ) : (
            historyPage.items.map((item) => (
              <Button
                key={item.id}
                variant="outline"
                className="h-auto w-full justify-between gap-4 whitespace-normal py-3 text-left"
                onClick={() => {
                  void openRun(item.id);
                }}
              >
                <span>
                  {item.name}
                  <span className="block text-caption text-text-secondary">
                    {new Date(item.created_at).toLocaleString()} · {item.state}
                  </span>
                </span>
                <span className="text-caption">
                  {item.passed} passed · {item.failed} failed · {item.needs_review} need review ·{" "}
                  {item.not_evaluated} not evaluated
                </span>
              </Button>
            ))
          )}
          <LabPagination {...historyPage} label="Saved run pages" />
        </div>
      )}
      {tab === "result" && run && (
        <div
          ref={resultRef}
          tabIndex={-1}
          role="region"
          aria-label="Task result"
          className="outline-none"
        >
          <RunResults
            key={run.id}
            run={run}
            reviewDrafts={reviewDrafts[run.id] ?? {}}
            onReviewDraftChange={(trialId, reason) =>
              setReviewDrafts((drafts) => ({
                ...drafts,
                [run.id]: { ...drafts[run.id], [trialId]: reason },
              }))
            }
            onCancel={() => {
              void lab.cancel(run.id);
            }}
            onEdit={() => {
              replaceSuite(run.suite);
              setTab("setup");
            }}
            onExport={(suiteOnly) => {
              void lab.exportRun(run.id, suiteOnly);
            }}
            onReview={(trialId, accepted, reason) => lab.review(run.id, trialId, accepted, reason)}
            onDelete={() => {
              void lab.deleteRun(run.id);
            }}
          />
        </div>
      )}
    </div>
  );
}

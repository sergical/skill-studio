import { useEffect, useReducer } from "react";
import type { ClientInfo, RunRecord, RunSummary, TestSuite } from "@skill-studio/lib";
import { labApi } from "../lib/skill-api";
import { activeRun, id, newSuite } from "../components/SkillLab/lab-model";
import type { JsonDrafts } from "../components/SkillLab/lab-json-drafts";

export type LabTab = "setup" | "history" | "result" | "improve";

interface LabState {
  suite: TestSuite;
  clients: ClientInfo[];
  suites: TestSuite[];
  history: RunSummary[];
  run: RunRecord | null;
  tab: LabTab;
  error: string | null;
  notice: string | null;
  status: "loading" | "working" | "ready";
  importText: string | null;
  jsonDrafts: JsonDrafts;
}

type Change = Partial<LabState>;
const reduce = (state: LabState, change: Change): LabState => ({ ...state, ...change });

async function load(skillPath: string, restoreActive: boolean): Promise<Change> {
  const [clients, suites, history] = await Promise.all([
    labApi.clients(),
    labApi.suites(skillPath),
    labApi.runs(skillPath),
  ]);
  const active = restoreActive
    ? history.find((item) => item.state === "running" || item.state === "queued")
    : undefined;
  const change: Change = { clients, suites, history, status: "ready", error: null };
  if (active) {
    change.run = await labApi.run(active.id);
    change.tab = "result";
  }
  return change;
}

export function useSkillLab(skillName: string, skillPath: string) {
  const [state, update] = useReducer(reduce, null, (): LabState => ({
    suite: newSuite(skillName, skillPath),
    clients: [],
    suites: [],
    history: [],
    run: null,
    tab: "setup",
    error: null,
    notice: null,
    status: "loading",
    importText: null,
    jsonDrafts: {},
  }));
  useEffect(() => {
    let cancelled = false;
    void load(skillPath, true).then(
      (change) => {
        if (!cancelled) update(change);
      },
      (cause) => {
        if (!cancelled) update({ status: "ready", error: String(cause) });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [skillPath]);

  const runId = state.run?.id;
  const running = activeRun(state.run);
  useEffect(() => {
    if (!runId || !running) return;
    let cancelled = false;
    let fetching = false;
    const poll = () => {
      if (fetching) return;
      fetching = true;
      void labApi
        .run(runId)
        .then(async (run) => {
          if (cancelled) return;
          if (activeRun(run)) {
            update({ run });
          } else {
            const history = await labApi.runs(skillPath);
            if (!cancelled) update({ run, history });
          }
        })
        .catch((cause) => {
          if (!cancelled)
            update({
              error: `Could not read the saved run: ${String(cause)}. Reopen it from history to retry.`,
            });
        })
        .finally(() => {
          fetching = false;
        });
    };
    const timer = setInterval(poll, 1000);
    poll();
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [runId, running, skillPath]);

  function perform(promise: Promise<Change>): Promise<boolean> {
    update({ status: "working", error: null, notice: null });
    return promise.then(
      (change) => {
        update({ ...change, status: "ready" });
        return true;
      },
      (cause) => {
        update({ status: "ready", error: String(cause) });
        return false;
      },
    );
  }

  return {
    ...state,
    loading: state.status === "loading",
    busy: state.status !== "ready",
    update,
    refresh: () => perform(load(skillPath, false)),
    save: () =>
      perform(
        labApi.save(state.suite).then(async () => ({
          suites: await labApi.suites(skillPath),
          notice: "Suite saved. Its cases and expected results are ready for the next run.",
        })),
      ),
    start: () =>
      perform(
        labApi.start(state.suite).then(async (runId): Promise<Change> => ({
          run: await labApi.run(runId),
          tab: "result",
        })),
      ),
    openRun: (runId: string) =>
      perform(labApi.run(runId).then((run): Change => ({ run, tab: "result" }))),
    importSuite: (text: string) =>
      perform(
        labApi.import(text).then((suite): Change => ({
          suite: { ...suite, id: id("suite"), skill_path: skillPath },
          jsonDrafts: {},
          importText: null,
          notice:
            "Suite imported. Review its clients, commands and expected results before running.",
        })),
      ),
    cancel: (runId: string) =>
      perform(
        labApi.cancel(runId).then(() => ({
          notice: "Stop requested. The current process and its children will stop.",
        })),
      ),
    exportRun: (runId: string, suiteOnly: boolean) =>
      perform(
        labApi.export(runId, suiteOnly).then(async (json) => {
          const saved = await labApi.saveExport(
            `${runId}-${suiteOnly ? "suite" : "results"}.json`,
            json,
          );
          return { notice: saved ? "JSON file saved." : "Export cancelled." };
        }),
      ),
    exportSuite: () =>
      perform(
        labApi
          .saveExport(`${state.suite.id}.json`, JSON.stringify(state.suite, null, 2))
          .then((saved) => ({ notice: saved ? "JSON file saved." : "Export cancelled." })),
      ),
    review: (runId: string, trialId: string, accepted: boolean, reason: string) =>
      perform(labApi.review(runId, trialId, accepted, reason).then((run) => ({ run }))),
    deleteRun: (runId: string) =>
      perform(
        labApi.delete(runId).then(async (): Promise<Change> => ({
          ...(await load(skillPath, false)),
          run: null,
          tab: "history",
        })),
      ),
  };
}

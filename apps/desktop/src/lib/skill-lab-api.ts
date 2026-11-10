import { invoke } from "@tauri-apps/api/core";
import type { ClientInfo, RunRecord, RunSummary, TestSuite } from "@skill-studio/lib";

export const labApi = {
  clients: () => invoke<ClientInfo[]>("lab_clients"),
  suites: (skillPath: string) => invoke<TestSuite[]>("lab_suites", { skillPath }),
  save: (suite: TestSuite) => invoke<void>("lab_save_suite", { suite }),
  import: (json: string) => invoke<TestSuite>("lab_import_suite", { json }),
  start: (suite: TestSuite) => invoke<string>("lab_start", { suite }),
  cancel: (id: string) => invoke<void>("lab_cancel", { id }),
  runs: (skillPath: string) => invoke<RunSummary[]>("lab_runs", { skillPath }),
  run: (id: string) => invoke<RunRecord>("lab_run", { id }),
  review: (id: string, trialId: string, accepted: boolean, reason: string) =>
    invoke<RunRecord>("lab_review", { id, trialId, accepted, reason }),
  export: (id: string, suiteOnly: boolean) => invoke<string>("lab_export", { id, suiteOnly }),
  saveExport: (name: string, json: string) => invoke<boolean>("lab_save_export", { name, json }),
  delete: (id: string) => invoke<void>("lab_delete_run", { id }),
};

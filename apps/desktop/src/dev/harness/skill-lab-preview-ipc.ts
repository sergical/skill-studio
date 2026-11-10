import { mockIPC } from "@tauri-apps/api/mocks";
import { z } from "zod";
import { recordedRun } from "./skill-lab-recording";

export function installSkillLabPreview() {
  mockIPC((command, args) => {
    switch (command) {
      case "lab_clients":
        return recordedRun.suite.variants.map((variant) => ({
          id: variant.client,
          binary: variant.client,
          version: recordedRun.trials.find((trial) => trial.variant_id === variant.id)
            ?.client_version,
          available: true,
          no_skill_control: variant.client === "pi",
          note: "Recorded client. This browser preview does not execute tests.",
        }));
      case "lab_suites":
        return [recordedRun.suite];
      case "lab_runs":
        return [
          {
            id: recordedRun.id,
            suite_id: recordedRun.suite.id,
            name: recordedRun.suite.name,
            skill_path: recordedRun.suite.skill_path,
            created_at: recordedRun.created_at,
            state: recordedRun.state,
            total: recordedRun.trials.length,
            passed: recordedRun.trials.filter((trial) => trial.outcome === "passed").length,
            failed: 0,
            needs_review: 0,
            not_evaluated: 0,
          },
        ];
      case "lab_run":
        return recordedRun;
      case "lab_export": {
        const request = z.object({ suiteOnly: z.boolean() }).parse(args);
        return JSON.stringify(request.suiteOnly ? recordedRun.suite : recordedRun, null, 2);
      }
      case "lab_save_export": {
        const request = z.object({ name: z.string(), json: z.string() }).parse(args);
        const url = URL.createObjectURL(new Blob([request.json], { type: "application/json" }));
        const link = document.createElement("a");
        link.href = url;
        link.download = request.name;
        link.click();
        URL.revokeObjectURL(url);
        return true;
      }
      default:
        throw new Error(
          "This is a recorded preview. Run new tests in the desktop app or with skill-studio test.",
        );
    }
  });
}

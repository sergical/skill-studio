// ============================================================================
// useUpdateCheck - the source update check behind Home's Updates tile: the
// newest summary (the snapshot's or a "Check now" result), its one-line
// caption, and the action that runs a new check.
// ============================================================================

import { startTransition, useActionState } from "react";
import { formatRelativeTime } from "@skill-studio/lib";
import type { SkillSnapshot } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";

type UpdateCheckSummary = SkillSnapshot["update_check"];

type CheckResult =
  | { kind: "initial" }
  | { kind: "checked"; summary: UpdateCheckSummary }
  | { kind: "failed"; message: string };

export interface UpdateCheck {
  caption: string;
  /** True when `caption` reports a failure, from the last check or from "Check now". */
  failed: boolean;
  /** The error of the last "Check now" click, announced once. */
  checkError: string | null;
  pending: boolean;
  check: () => void;
}

export function useUpdateCheck(summary: UpdateCheckSummary): UpdateCheck {
  const checkSkillUpdates = useAppStore((state) => state.checkSkillUpdates);
  const [result, run, pending] = useActionState<CheckResult>(
    async () => {
      try {
        return { kind: "checked", summary: await checkSkillUpdates() };
      } catch (error) {
        return { kind: "failed", message: error instanceof Error ? error.message : String(error) };
      }
    },
    { kind: "initial" },
  );
  const resultAt = result.kind === "checked" ? result.summary.checked_at : null;
  const latest =
    result.kind === "checked" &&
    (!summary.checked_at || (resultAt !== null && resultAt >= summary.checked_at))
      ? result.summary
      : summary;
  const checkError = result.kind === "failed" ? result.message : null;
  const failure =
    checkError ??
    (latest.gh_status !== "ok" ? (latest.message ?? "Could not check for updates") : null);
  return {
    caption:
      failure ??
      (latest.checked_at ? `Checked ${formatRelativeTime(latest.checked_at)}` : "Not checked yet"),
    failed: failure !== null,
    checkError,
    pending,
    check: () => startTransition(run),
  };
}

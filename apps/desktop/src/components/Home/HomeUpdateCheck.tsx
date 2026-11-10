import { startTransition, useActionState } from "react";
import { Button } from "@skill-studio/ui";
import { formatRelativeTime } from "@skill-studio/lib";
import type { SkillSnapshot } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";

type CheckResult =
  | { kind: "initial" }
  | { kind: "checked"; summary: SkillSnapshot["update_check"] }
  | { kind: "failed"; message: string };

export function HomeUpdateCheck({ summary }: { summary: SkillSnapshot["update_check"] }) {
  const checkSkillUpdates = useAppStore((state) => state.checkSkillUpdates);
  const [result, check, pending] = useActionState<CheckResult>(
    async () => {
      try {
        return { kind: "checked", summary: await checkSkillUpdates() };
      } catch (error) {
        return { kind: "failed", message: error instanceof Error ? error.message : String(error) };
      }
    },
    { kind: "initial" },
  );
  const latest =
    result.kind === "checked" &&
    (!summary.checked_at ||
      !result.summary.checked_at ||
      result.summary.checked_at >= summary.checked_at)
      ? result.summary
      : summary;
  const status =
    latest.gh_status !== "ok"
      ? (latest.message ?? "Could not check for updates")
      : !latest.checked_at
        ? "Updates have not been checked yet"
        : latest.updates_available === 0
          ? "No updates available"
          : `${latest.updates_available} update${latest.updates_available === 1 ? "" : "s"} available`;
  return (
    <section
      aria-label="Skill update check"
      className="flex items-center justify-between gap-3 rounded-md border border-border-subtle px-4 py-3"
    >
      <div className="flex min-w-0 flex-col gap-1">
        <span role="status" className="text-small text-text-secondary">
          {result.kind === "failed" ? "Could not check for updates" : status}
        </span>
        {result.kind === "failed" && (
          <p role="alert" className="m-0 text-small text-error">
            {result.message}
          </p>
        )}
        {latest.checked_at && (
          <span className="text-caption text-text-tertiary">
            Checked {formatRelativeTime(latest.checked_at)}
          </span>
        )}
      </div>
      <Button variant="outline" size="sm" disabled={pending} onClick={() => startTransition(check)}>
        {pending ? "Checking…" : "Check now"}
      </Button>
    </section>
  );
}

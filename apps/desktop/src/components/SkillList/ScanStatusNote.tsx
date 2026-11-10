import type { BackgroundStatus } from "@skill-studio/lib";

export function ScanStatusNote({ status }: { status: BackgroundStatus | undefined }) {
  const tasks = status?.tasks.filter((task) => task.worker !== "update_check") ?? [];
  if (tasks.some((task) => task.running)) {
    return (
      <p role="status" className="text-body text-text-tertiary">
        Still scanning. Your previous results stay available.
      </p>
    );
  }
  const failure = tasks.find((task) => task.last_error);
  if (failure) {
    return (
      <p role="alert" className="text-body text-text-tertiary">
        Refresh failed. {failure.last_error}
      </p>
    );
  }
  return null;
}

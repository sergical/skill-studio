import { LoaderCircle } from "lucide-react";

/** A fixed-width status slot that keeps its space between refreshes. */
export function RefreshIndicator({ active, label }: { active: boolean; label: string }) {
  return (
    <span
      className="inline-flex size-3.5 shrink-0"
      role="status"
      aria-label={active ? label : undefined}
    >
      {active && (
        <LoaderCircle
          aria-hidden="true"
          className="size-full animate-spin text-text-tertiary motion-reduce:animate-none"
        />
      )}
    </span>
  );
}

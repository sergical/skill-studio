// ============================================================================
// SkillHistorySection - Activity view's History list: one row per event
// store entry (docs/spec-event-store.md), with restore, drift-guard
// force-restore, and "Reveal in Finder" for backed-up events.
// ============================================================================

import { useEffect, useId, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  ArchiveRestore,
  FolderOpen,
  FolderSymlink,
  History as HistoryIcon,
  Link2,
  Link2Off,
  RotateCcw,
  Undo2,
  Download,
  Trash2,
  RefreshCw,
  GitFork,
  SlidersHorizontal,
  FilePenLine,
  GitBranch,
} from "lucide-react";
import { Button } from "@skill-studio/ui";
import { deploymentLabelFromAgentId, formatRelativeTime } from "@skill-studio/lib";
import type { SkillEvent } from "@skill-studio/lib";
import { listSkillEvents, openSkillPath, restoreSkillEvent } from "../../lib/skill-api";
import { useAppStore } from "../../store/appStore";
import {
  canRestoreSkillEvent,
  eventLabel,
  kindLabel,
  shouldOfferForceRestore,
  restoreUnavailableReason,
} from "./skill-history-restore-policy";
import { PageSection } from "../Shell/PageSection";
import { CappedList } from "./ActivityParts";

/** Icon per event kind - see the Materialize section of spec-event-store.md for what each kind does. */
function iconForKind(kind: string, className: string) {
  const props = { size: 14, className: `shrink-0 ${className}` };
  switch (kind) {
    case "install":
      return <Download {...props} />;
    case "remove":
    case "quarantine_prune":
    case "recovery_plan_dropped":
      return <Trash2 {...props} />;
    case "update":
      return <RefreshCw {...props} />;
    case "fork":
      return <GitFork {...props} />;
    case "listing_setting_change":
    case "invocation_change":
      return <SlidersHorizontal {...props} />;
    case "repair_skill_frontmatter":
      return <FilePenLine {...props} />;
    case "split":
      return <GitBranch {...props} />;
    case "restore":
      return <Undo2 {...props} />;
    case "unlink_harness":
    case "materialize_then_disable":
    case "reconcile_remove_stale_link":
    case "repair_remove_link":
    case "harness_disable":
      return <Link2Off {...props} />;
    case "relink_harness":
    case "repair_relink_link":
    case "harness_enable":
      return <Link2 {...props} />;
    case "explode_shared_dir":
    case "distribute_from_shared":
    case "make_independent_copy":
      return <FolderSymlink {...props} />;
    case "park":
    case "move_aside_disable":
      return <Archive {...props} />;
    case "unpark":
    case "move_aside_restore":
      return <ArchiveRestore {...props} />;
    default:
      return <HistoryIcon {...props} />;
  }
}

/** What a restore's confirm dialog names as "what will be put back" - the inverse of the event's own kind. */
function restoreDescription(event: SkillEvent): string {
  const skillPart = event.skill ? `${event.skill}` : (event.harness ?? "this item");
  switch (event.kind) {
    case "unlink_harness":
      return `Restore ${skillPart}'s link for ${event.harness ?? "its agent"}`;
    case "explode_shared_dir":
      return `Restore ${event.harness ?? "the agent"}'s whole-folder link`;
    case "distribute_from_shared":
      return `Move ${skillPart} back into the Universal folder and remove the per-agent copies`;
    case "make_independent_copy":
      return `Restore ${skillPart}'s exact Universal link`;
    case "move_aside_disable":
      return `Restore ${skillPart} to its original location`;
    default:
      return `Undo "${kindLabel(event.kind)}" for ${skillPart}`;
  }
}

/** Owns the confirm-restore / force-restore-on-conflict flow for one event row. */
function useRestoreEvent(event: SkillEvent, onRestored: () => void) {
  const addToast = useAppStore((state) => state.addToast);
  const [isRestoring, setIsRestoring] = useState(false);

  const runRestore = async (force: boolean) => {
    setIsRestoring(true);
    try {
      await restoreSkillEvent(event.id, force);
      addToast({ type: "success", title: "Restored", message: restoreDescription(event) });
      onRestored();
      setIsRestoring(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unknown error";
      if (!force && shouldOfferForceRestore(event, err)) {
        const proceed = await ask(
          `${message}\n\nRestoring anyway will back up the current content first, so it stays restorable.`,
          { title: "Content has changed", kind: "warning" },
        );
        if (proceed) {
          await runRestore(true);
          setIsRestoring(false);
          return;
        }
      } else {
        addToast({ type: "error", title: "Restore failed", message });
      }
      setIsRestoring(false);
    }
  };

  const handleRestoreClick = async () => {
    const confirmed = await ask(`${restoreDescription(event)}?`, {
      title: "Restore",
      kind: "info",
    });
    if (!confirmed) return;
    await runRestore(false);
  };

  return { isRestoring, handleRestoreClick };
}

/** Status text before the time, only when the event did not finish: "done" is every normal row. */
function EventStatus({ status }: { status: string }) {
  if (status === "done") return null;
  const isFailed = status === "failed";
  const isInterrupted = status === "interrupted";
  return (
    <span
      className={`text-small font-medium ${
        isFailed ? "text-error-text" : isInterrupted ? "text-warning" : "text-text-tertiary"
      }`}
    >
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

/**
 * The row's actions: Restore, or why it can't restore, and "Reveal in Finder" for backed-up events.
 * They cover the time while the row is hovered or holds focus, so a resting list shows only what
 * happened and when.
 */
function EventActions({
  event,
  isRestoring,
  onRestoreClick,
  reasonId,
}: {
  event: SkillEvent;
  isRestoring: boolean;
  onRestoreClick: () => void;
  reasonId: string;
}) {
  const addToast = useAppStore((state) => state.addToast);

  const handleReveal = () => {
    if (!event.backup_path) return;
    openSkillPath(event.backup_path, "reveal").catch((err) => {
      addToast({
        type: "error",
        title: "Couldn't reveal in Finder",
        message: err instanceof Error ? err.message : "Unknown error",
      });
    });
  };

  return (
    <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center gap-1 bg-bg-primary pl-3 whitespace-nowrap opacity-0 group-focus-within/row:pointer-events-auto group-focus-within/row:opacity-100 group-hover/row:pointer-events-auto group-hover/row:opacity-100 has-aria-busy:pointer-events-auto has-aria-busy:opacity-100">
      {canRestoreSkillEvent(event) ? (
        <Button variant="outline" size="xs" onClick={onRestoreClick} busy={isRestoring}>
          Restore
        </Button>
      ) : (
        <span id={reasonId} className="text-small text-text-tertiary">
          {event.restore_unavailable_reason ?? restoreUnavailableReason(event)}
        </span>
      )}
      {event.backup_path && (
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Reveal in Finder"
          title="Reveal in Finder"
          className="text-text-tertiary"
          onClick={handleReveal}
        >
          <FolderOpen />
        </Button>
      )}
    </div>
  );
}

function EventRow({ event, onRestored }: { event: SkillEvent; onRestored: () => void }) {
  const { isRestoring, handleRestoreClick } = useRestoreEvent(event, onRestored);
  const iconColor =
    event.status === "failed"
      ? "text-error"
      : event.status === "interrupted"
        ? "text-warning"
        : "text-text-tertiary";
  const harnessLabel = event.harness ? deploymentLabelFromAgentId(event.harness) : null;
  const subject = event.skill || (harnessLabel ?? kindLabel(event.kind));
  // An agent-only event names the agent as its subject; a skill event for one agent names it here.
  const agent = event.skill && harnessLabel && event.kind !== "split" ? ` · ${harnessLabel}` : "";
  const id = useId();
  // A row with no button takes focus itself, so Tab still shows why it cannot be restored. As a
  // named group, a screen reader says what happened on focus, then why it cannot be restored.
  const focusable = !canRestoreSkillEvent(event) && !event.backup_path;

  return (
    <div
      tabIndex={focusable ? 0 : undefined}
      role={focusable ? "group" : undefined}
      aria-labelledby={focusable ? `${id}-subject ${id}-what` : undefined}
      aria-describedby={focusable ? `${id}-reason` : undefined}
      className="group/row relative flex h-8 items-center gap-3 rounded-sm border-b border-border-subtle px-2 outline-none last:border-b-0 focus-visible:ring-1 focus-visible:ring-text-secondary"
    >
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {iconForKind(event.kind, iconColor)}
        <span
          id={`${id}-subject`}
          className="min-w-0 shrink truncate text-body text-text-primary"
          title={subject}
        >
          {subject}
        </span>
        <span
          id={`${id}-what`}
          className="min-w-0 shrink-[2] truncate text-small text-text-tertiary"
        >
          {eventLabel(event, harnessLabel)}
          {agent}
        </span>
      </div>
      <EventStatus status={event.status} />
      {canRestoreSkillEvent(event) && (
        // At rest, marks the rows that Restore can undo; the button itself shows on hover.
        <RotateCcw aria-hidden size={12} className="shrink-0 text-text-tertiary" />
      )}
      <span className="shrink-0 text-small text-text-tertiary tabular-nums">
        {formatRelativeTime(event.ts)}
      </span>
      <EventActions
        event={event}
        isRestoring={isRestoring}
        onRestoreClick={handleRestoreClick}
        reasonId={`${id}-reason`}
      />
    </div>
  );
}

/**
 * The Activity view's History section: every event store row, newest first.
 * Fetched on mount and refetched after a successful restore - the backend
 * already re-emits `skills://snapshot` on its own after a mutating command,
 * so nothing else needs to be triggered here.
 */
export function SkillHistorySection() {
  const [events, setEvents] = useState<SkillEvent[] | null>(null);
  const addToast = useAppStore((state) => state.addToast);

  useEffect(() => {
    let cancelled = false;
    listSkillEvents()
      .then((rows) => {
        if (!cancelled) setEvents(rows);
      })
      .catch((err) => {
        if (!cancelled) setEvents([]);
        addToast({
          type: "error",
          title: "Couldn't load history",
          message: err instanceof Error ? err.message : "Unknown error",
        });
      });
    return () => {
      cancelled = true;
    };
  }, [addToast]);

  const refresh = () => {
    listSkillEvents()
      .then(setEvents)
      .catch(() => {
        // A refetch failure after a successful restore isn't worth a second
        // toast - the row stays as it was until the next successful load.
      });
  };

  return (
    <PageSection title="History">
      {events === null ? (
        <p className="text-wrap-pretty text-body text-text-tertiary">Loading…</p>
      ) : events.length === 0 ? (
        <p className="text-wrap-pretty text-body text-text-tertiary">No events recorded yet.</p>
      ) : (
        <CappedList
          items={events}
          render={(event) => <EventRow key={event.id} event={event} onRestored={refresh} />}
        />
      )}
    </PageSection>
  );
}

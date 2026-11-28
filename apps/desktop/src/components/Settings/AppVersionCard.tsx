// ============================================================================
// AppVersionCard - Settings' "Version" section: the running app's version and
// build commit, and a "What's new" toggle for the current release's
// changelog notes, bundled into the app at build time so it needs no
// network.
// ============================================================================

import { useState } from "react";
import { Button } from "@skill-studio/ui";
import type { UpdateStatus } from "@skill-studio/lib";
import { invokeErrorMessage } from "../../lib/skill-command-error";
import { useBackendEvent } from "../../hooks/useBackendEvent";
import { useLoad } from "../../hooks/useLoad";
import { useAppStore } from "../../store/appStore";
import { PageSection } from "../Shell/PageSection";

/** Settings' own label for each `UpdateStatus` - kept here rather than in `skill-types.ts` since
 * it's UI copy, not part of the wire shape. */
function updateStatusLabel(status: UpdateStatus): string {
  switch (status.status) {
    case "up-to-date":
      return "Up to date";
    case "checking":
      return "Checking for updates…";
    case "downloading":
      return `Downloading v${status.version}…`;
    case "ready-to-install":
      return `v${status.version} ready to install`;
    case "check-failed":
      return `Couldn't check for updates: ${status.message}`;
    case "error":
      return status.message;
  }
}

/** The colour for each `UpdateStatus`: the manual click's hard `Error` and a
 * failed install stay red, a background check that could not reach the
 * endpoint is a warning (visible, but not the user's fault), and the rest
 * are neutral. */
function updateStatusTone(status: UpdateStatus): string {
  switch (status.status) {
    case "error":
      return "text-error";
    case "check-failed":
      return "text-warning";
    default:
      return "text-text-secondary";
  }
}

export function AppVersionCard() {
  const addToast = useAppStore((state) => state.addToast);
  const appVersion = useAppStore((state) => state.appVersion);
  const checkForUpdate = useAppStore((state) => state.checkForUpdate);
  const getUpdateStatus = useAppStore((state) => state.getUpdateStatus);
  const installUpdate = useAppStore((state) => state.installUpdate);
  const onUpdateStatus = useAppStore((state) => state.onUpdateStatus);
  const [showNotes, setShowNotes] = useState(false);
  const [installing, setInstalling] = useState(false);
  const version = useLoad("app-version", appVersion, {
    onFailed: (message) =>
      addToast({ type: "error", title: "Couldn't read the app version", message }),
  });
  const info = version.data;
  const update = useLoad("update-status", getUpdateStatus);
  // The status read and the event stream both write this one value; whichever lands last shows.
  useBackendEvent(onUpdateStatus, update.setData, (message) =>
    // A listener that fails to register is a toast, not an update status: writing it over the
    // status read would erase a "Restart to update" or leave a "checking" that never ends.
    addToast({ type: "error", title: "Couldn't follow update progress", message }),
  );
  // A failed read shows over an earlier event, as the last of the two to land.
  const updateStatus: UpdateStatus =
    update.error !== null
      ? { status: "error", message: update.error }
      : (update.data ?? { status: "up-to-date" });

  const handleCheckForUpdate = () => {
    update.setData({ status: "checking" });
    checkForUpdate()
      .then(update.setData)
      .catch((err) => {
        update.setData({ status: "error", message: invokeErrorMessage(err) });
        addToast({
          type: "error",
          title: "Couldn't check for updates",
          message: invokeErrorMessage(err),
        });
      });
  };

  const handleRestartToUpdate = () => {
    setInstalling(true);
    installUpdate().catch((err) => {
      setInstalling(false);
      addToast({
        type: "error",
        title: "Couldn't install the update",
        message: invokeErrorMessage(err),
      });
    });
  };

  if (!info) return null;

  const shortCommit = info.commit === "dev" ? "dev" : info.commit.slice(0, 7);
  const checking = updateStatus.status === "checking" || updateStatus.status === "downloading";

  return (
    <PageSection
      title="Version"
      meta={
        <>
          {info.notes && (
            <Button
              variant="ghost"
              size="sm"
              aria-expanded={showNotes}
              onClick={() => setShowNotes((open) => !open)}
            >
              {showNotes ? "Hide what's new" : "What's new"}
            </Button>
          )}
          {updateStatus.status === "ready-to-install" ? (
            <Button variant="default" size="sm" onClick={handleRestartToUpdate} busy={installing}>
              Restart to update
            </Button>
          ) : (
            <Button variant="outline" size="sm" onClick={handleCheckForUpdate} disabled={checking}>
              Check for updates
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <p className="m-0 flex flex-wrap items-center gap-x-2 text-body text-text-secondary">
          <span className="tabular-nums">
            v{info.version} ({shortCommit})
          </span>
          <span aria-hidden="true" className="text-text-tertiary">
            ·
          </span>
          <span role="status" className={updateStatusTone(updateStatus)}>
            {checking && (
              <span className="mr-2 inline-block size-3 animate-spin rounded-full border-2 border-current border-t-transparent align-middle motion-reduce:animate-none" />
            )}
            {updateStatusLabel(updateStatus)}
          </span>
        </p>
        {showNotes && info.notes && (
          <div className="whitespace-pre-wrap rounded-md bg-bg-secondary p-3 text-small text-text-secondary">
            {info.notes}
          </div>
        )}
      </div>
    </PageSection>
  );
}

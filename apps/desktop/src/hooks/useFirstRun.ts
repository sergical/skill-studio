// ============================================================================
// Skill Studio - useFirstRun
// Talks to skill-api.ts for the first-run harness screen (unit 3.2) so the
// component itself stays presentational - see the layering rule in
// .oxlintrc.json (components go through the store or a hook, not skill-api
// directly).
// ============================================================================

import { useEffect, useState } from "react";
import type { HarnessDetection, HarnessesChoice, HarnessReport } from "@skill-studio/lib";
import {
  detectHarnesses,
  getHarnessesChoice,
  invokeErrorMessage,
  saveHarnessesChoice,
} from "../lib/skill-api";

import { setHarnessDetection } from "./useDetectedHarnesses";

let launchDetection: Promise<HarnessReport> | undefined;
function detectAtLaunch(): Promise<HarnessReport> {
  launchDetection ??= detectHarnesses();
  return launchDetection;
}

interface FirstRunGateState {
  /** `null` while the saved-choice check is in flight. */
  showScreen: boolean | null;
}

type ChoiceRead = { ok: true; choice: HarnessesChoice | null } | { ok: false };

/** An unreadable registry (`ok: false`) skips the screen: the main app
 * already runs on a corrupt registry via `read_fork_registry_or_default`,
 * so a first-run screen whose Continue can never save would trap the user
 * instead of protecting them. */
export function showScreenForChoiceRead(read: ChoiceRead): boolean {
  return read.ok && read.choice === null;
}

/** Checks the registry for a saved choice once on mount. */
export function useFirstRunGate(): FirstRunGateState {
  const [showScreen, setShowScreen] = useState<boolean | null>(null);

  useEffect(() => {
    let cancelled = false;
    void loadLaunchChoice({
      readChoice: getHarnessesChoice,
      detect: detectAtLaunch,
      onChoice: (choice) => setHarnessDetection({ choice }),
      onGate: (show) => {
        if (!cancelled) setShowScreen(show);
      },
      onReport: (report) => setHarnessDetection({ report }),
      onError: (error) => setHarnessDetection({ error }),
    }).catch(() => {
      if (!cancelled) setShowScreen(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return { showScreen };
}

export async function loadLaunchChoice(effects: {
  readChoice: () => Promise<HarnessesChoice | null>;
  detect: () => Promise<HarnessReport>;
  onChoice: (choice: HarnessesChoice | null) => void;
  onGate: (show: boolean) => void;
  onReport: (report: HarnessReport) => void;
  onError: (message: string) => void;
}): Promise<void> {
  try {
    const choice = await effects.readChoice();
    effects.onChoice(choice);
    effects.onGate(showScreenForChoiceRead({ ok: true, choice }));
    if (choice === null) return;
  } catch (cause) {
    effects.onChoice(null);
    effects.onGate(showScreenForChoiceRead({ ok: false }));
    effects.onError(invokeErrorMessage(cause));
  }
  void effects
    .detect()
    .then(effects.onReport)
    .catch((cause: unknown) => effects.onError(invokeErrorMessage(cause)));
}

interface FirstRunScreenState {
  rows: HarnessDetection[] | null;
  kept: Set<string>;
  toggleRow: (id: string, checked: boolean) => void;
  searchProjectFolders: boolean;
  setSearchProjectFolders: (value: boolean) => void;
  telemetryEnabled: boolean;
  setTelemetryEnabled: (value: boolean) => void;
  /** Why harness detection failed; the screen still lets the user continue. */
  error: string | null;
  /** Why the last save failed, kept apart from `error` so a save cannot hide or replace the detection reason. */
  saveError: string | null;
  saving: boolean;
  continue: () => void;
}

/** The welcome screen's telemetry switch starts on; the registry itself
 * defaults to off (`skill_fork_registry.rs`) so a build that never shows
 * this screen never opts a user in. `save_harnesses_choice` writes
 * whatever the user leaves the switch at when they continue. */
export const FIRST_RUN_TELEMETRY_DEFAULT = true;

interface FirstRunSave {
  choice: HarnessesChoice;
  telemetryEnabled: boolean;
}

/** Builds `saveHarnessesChoice`'s arguments from the screen's state, kept
 * apart from `continueToApp` so a test can check the save without a
 * `tauri::AppHandle`-backed `saveHarnessesChoice` call. */
export function buildFirstRunSave(
  kept: Set<string>,
  searchProjectFolders: boolean,
  telemetryEnabled: boolean,
  savedAt: string,
): FirstRunSave {
  return {
    choice: {
      kept: Array.from(kept),
      search_project_folders: searchProjectFolders,
      saved_at: savedAt,
    },
    telemetryEnabled,
  };
}

/** Continue waits only for detection still in flight or a save in
 * progress. Neither a detection error nor a save error blocks it: the
 * user continues with an empty or partial choice rather than being
 * trapped on a screen a failed probe or a failed write can never let
 * them leave. */
export function continueIsBlocked(state: {
  rows: HarnessDetection[] | null;
  error: string | null;
  saving: boolean;
}): boolean {
  const detecting = state.rows == null && state.error == null;
  return detecting || state.saving;
}

/** Saves the welcome-screen choice and then opens the app. A save that fails keeps the
 *  welcome screen: `onSaved` is not called and `onSaveFailed` gets the reason text. */
export async function saveChoiceThenOpenApp(
  save: () => Promise<void>,
  effects: { onSaved: () => void; onSaveFailed: (reason: string) => void },
): Promise<void> {
  try {
    await save();
  } catch (cause) {
    effects.onSaveFailed(invokeErrorMessage(cause));
    return;
  }
  effects.onSaved();
}

/** Detects harnesses once on mount, tracks which rows the user keeps
 * (defaulting every non-"not_found" row to kept once detection resolves),
 * and exposes `continue` to persist the choice through
 * `saveHarnessesChoice`. */
export function useFirstRunScreen(onSaved: () => void): FirstRunScreenState {
  const [rows, setRows] = useState<HarnessDetection[] | null>(null);
  const [kept, setKept] = useState<Set<string>>(new Set());
  const [searchProjectFolders, setSearchProjectFolders] = useState(true);
  const [telemetryEnabled, setTelemetryEnabled] = useState(FIRST_RUN_TELEMETRY_DEFAULT);
  const [error, setError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    detectAtLaunch()
      .then((report) => {
        if (cancelled) return;
        setHarnessDetection({ report });
        setRows(report.harnesses);
        const found = new Set<string>();
        for (const row of report.harnesses) {
          if (row.state !== "not_found") found.add(row.id);
        }
        setKept(found);
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          const message = invokeErrorMessage(cause);
          setError(message);
          setHarnessDetection({ error: message });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function toggleRow(id: string, checked: boolean) {
    const next = new Set(kept);
    if (checked) next.add(id);
    else next.delete(id);
    setKept(next);
  }

  function continueToApp() {
    setSaveError(null);
    setSaving(true);
    const { choice, telemetryEnabled: savedTelemetryEnabled } = buildFirstRunSave(
      kept,
      searchProjectFolders,
      telemetryEnabled,
      new Date().toISOString(),
    );
    void saveChoiceThenOpenApp(() => saveHarnessesChoice(choice, savedTelemetryEnabled), {
      onSaved: () => {
        setHarnessDetection({ choice });
        onSaved();
      },
      onSaveFailed: (reason) => {
        setSaveError(`Couldn't save your choice. ${reason}`);
        setSaving(false);
      },
    });
  }

  return {
    rows,
    kept,
    toggleRow,
    searchProjectFolders,
    setSearchProjectFolders,
    telemetryEnabled,
    setTelemetryEnabled,
    error,
    saveError,
    saving,
    continue: continueToApp,
  };
}

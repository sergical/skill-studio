// ============================================================================
// useAddSkillMachine - What the Add by source form reads from this machine:
// the add method defaults, and a project folder the user picks.
// ============================================================================

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { AddMethodDefaults } from "@skill-studio/lib";
import { invokeErrorMessage } from "../../lib/skill-command-error";
import { useAppStore } from "../../store/appStore";

/**
 * What dotagents/skills.sh/the Universal folder look like on this machine -
 * fetched when the sheet opens and again when the install scope changes, so
 * the Method and Harnesses defaults reflect this machine instead of a generic
 * guess. Null until it answers, and when it fails.
 */
export function useAddMethodDefaults(
  isOpen: boolean,
  projectPath: string | null,
): AddMethodDefaults | null {
  const getAddMethodDefaults = useAppStore((state) => state.getAddMethodDefaults);
  const [defaults, setDefaults] = useState<AddMethodDefaults | null>(null);
  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    getAddMethodDefaults(projectPath)
      .then((next) => {
        if (!cancelled) setDefaults(next);
      })
      .catch(() => {
        if (!cancelled) setDefaults(null);
      });
    return () => {
      cancelled = true;
    };
  }, [isOpen, projectPath, getAddMethodDefaults]);
  return defaults;
}

/** Asks for a project folder, hands it to `onPicked`, then tracks it in the saved list. */
export function useBrowseProjectFolder(onPicked: (path: string) => void) {
  const addToast = useAppStore((state) => state.addToast);
  const setTrackedProjects = useAppStore((state) => state.setTrackedProjects);
  const registerSkillProjects = useAppStore((state) => state.registerSkillProjects);
  return async () => {
    const selected = await open({ directory: true, multiple: false, title: "Select Project" });
    if (!selected) return;
    // Installing into the folder does not depend on tracking it, so the pick
    // stands even when the saved list cannot be written.
    onPicked(selected);
    try {
      setTrackedProjects(await registerSkillProjects([selected]));
    } catch (err) {
      addToast({
        type: "error",
        title: "Couldn't save project folder",
        message: invokeErrorMessage(err),
      });
    }
  };
}

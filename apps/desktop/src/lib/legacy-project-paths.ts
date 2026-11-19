// ============================================================================
// Skill Studio - legacy-project-paths
// One-shot migration for a machine whose added/excluded project folders still
// sit in localStorage (`project-paths`/`excluded-project-paths`). App.tsx's
// startup effect calls `loadStartupTrackedProjects`, which imports them into
// ~/.agents/skill-studio.json and then clears both keys; the saved file is the
// only source afterwards.
// ============================================================================

import type { TrackedProjects } from "@skill-studio/lib";
import { getTrackedProjects, importTrackedProjects } from "./skill-api";

const PROJECT_PATHS_STORAGE_KEY = "project-paths";
const EXCLUDED_PROJECT_PATHS_STORAGE_KEY = "excluded-project-paths";

function loadRawPathList(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function splitPathList(raw: string | null): string[] {
  return (raw ?? "").split("\n").filter(Boolean);
}

/** The localStorage project lists, or `null` if this machine never had either key. */
function readLegacyProjectPaths(): { added: string[]; excluded: string[] } | null {
  const added = loadRawPathList(PROJECT_PATHS_STORAGE_KEY);
  const excluded = loadRawPathList(EXCLUDED_PROJECT_PATHS_STORAGE_KEY);
  if (added === null && excluded === null) return null;
  return { added: splitPathList(added), excluded: splitPathList(excluded) };
}

/** Drops the legacy keys once their contents are safely migrated into the backend. */
function clearLegacyProjectPaths(): void {
  try {
    localStorage.removeItem(PROJECT_PATHS_STORAGE_KEY);
    localStorage.removeItem(EXCLUDED_PROJECT_PATHS_STORAGE_KEY);
  } catch {
    // Storage can be unavailable (quota, private mode); nothing to clean up then.
  }
}

/**
 * The tracked project list at startup. A machine with leftover localStorage entries imports them
 * first (home-directory entries are the backend's job to drop, same as any other add) and clears
 * localStorage only once that import succeeds; every other machine reads the saved list.
 */
export async function loadStartupTrackedProjects(): Promise<TrackedProjects> {
  const legacy = readLegacyProjectPaths();
  if (!legacy) return getTrackedProjects();
  const projects = await importTrackedProjects(legacy.added, legacy.excluded);
  clearLegacyProjectPaths();
  return projects;
}

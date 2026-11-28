// ============================================================================
// useStoreReports - The listing and sessions reports the store caches. Each
// hook returns the cached report and asks the store to read it again when the
// component mounts and when what the report covers changes.
// ============================================================================

import { useEffect, useRef } from "react";
import type { ListingRequest } from "@skill-studio/lib";
import { listingReportKey, sessionsReportKey, useAppStore } from "../store/appStore";

/** The listing request for a scope as the agents are set up now, with no "what if" budget. */
export function currentListingRequest(projectPath: string | null): ListingRequest {
  return {
    project_path: projectPath,
    claude_budget_fraction: null,
    claude_context_window_tokens: null,
    codex_max_context_tokens: null,
    claude_transcript: null,
    codex_transcript: null,
  };
}

/**
 * The cached listing report for the home (`null`) or one project. `contentHash` is what the
 * caller knows the report was computed from: a new value reads the report again.
 */
export function useListingReport(projectPath: string | null, contentHash = "") {
  const report = useAppStore(
    (state) => state.listingReports[listingReportKey(currentListingRequest(projectPath))],
  );
  const lastRead = useRef<string | null>(null);

  useEffect(() => {
    const read = JSON.stringify([projectPath, contentHash]);
    if (lastRead.current === read) return;
    lastRead.current = read;
    useAppStore.getState().refreshListing(currentListingRequest(projectPath));
  }, [projectPath, contentHash]);

  return report;
}

/** The cached report on the last `sessions` sessions of the home (`null`) or one project. */
export function useSessionsReport(projectPath: string | null, sessions: number, contentHash = "") {
  const report = useAppStore(
    (state) => state.sessionsReports[sessionsReportKey({ project_path: projectPath, sessions })],
  );
  const lastRead = useRef<string | null>(null);

  useEffect(() => {
    const read = JSON.stringify([projectPath, sessions, contentHash]);
    if (lastRead.current === read) return;
    lastRead.current = read;
    useAppStore.getState().refreshSkillSessions({ project_path: projectPath, sessions });
  }, [projectPath, sessions, contentHash]);

  return report;
}

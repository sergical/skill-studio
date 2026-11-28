import { useEffect, useRef } from "react";
import type { ListingRequest } from "@skill-studio/lib";
import { listingReportKey, useAppStore } from "../../store/appStore";

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

export function useSkillVisibility(projectPath: string | null, contentHash: string) {
  const listingKey = listingReportKey(currentListingRequest(projectPath));
  const listing = useAppStore((s) => s.listingReports[listingKey]);
  const lastRead = useRef<{ projectPath: string | null; contentHash: string } | null>(null);
  useEffect(() => {
    if (
      lastRead.current?.projectPath === projectPath &&
      lastRead.current.contentHash === contentHash
    )
      return;
    lastRead.current = { projectPath, contentHash };
    useAppStore.getState().refreshListing(currentListingRequest(projectPath));
  }, [projectPath, contentHash]);
  const listings =
    listing?.data?.listings.filter((r) =>
      projectPath === null
        ? r.scope.scope === "global"
        : r.scope.scope === "project" && r.scope.project === projectPath,
    ) ?? [];
  return {
    listings,
    // An earlier report kept after a failed read is not a read of the current list.
    hasListing: Boolean(listing?.data) && !listing?.error,
    error: listing?.error,
  };
}

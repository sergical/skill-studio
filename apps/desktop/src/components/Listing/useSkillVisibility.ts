import { useListingReport } from "../../hooks/useStoreReports";

export function useSkillVisibility(projectPath: string | null, contentHash: string) {
  const listing = useListingReport(projectPath, contentHash);
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
    /** `listings` come from that earlier report, not from the latest read. */
    stale: Boolean(listing?.data) && Boolean(listing?.error),
    error: listing?.error,
  };
}

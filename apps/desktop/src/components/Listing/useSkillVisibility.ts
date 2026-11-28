import { useListingReport, useSessionsReport } from "../../hooks/useStoreReports";

export function useSkillVisibility(projectPath: string | null, contentHash: string) {
  const listing = useListingReport(projectPath, contentHash);
  const history = useSessionsReport(projectPath, 50, contentHash);
  const listings =
    listing?.data?.listings.filter((r) =>
      projectPath === null
        ? r.scope.scope === "global"
        : r.scope.scope === "project" && r.scope.project === projectPath,
    ) ?? [];
  return {
    listings,
    history: history?.data,
    error: listing?.error,
  };
}

import { useEffect, useRef } from "react";
import { currentListingRequest } from "../../components/Listing/useSkillVisibility";
import { listingReportKey, sessionsReportKey, useAppStore } from "../../store/appStore";

const listingRequest = currentListingRequest(null);
const listingKey = listingReportKey(listingRequest);
const sessionsRequest = { project_path: null, sessions: 10 };
const sessionsKey = sessionsReportKey(sessionsRequest);

const refreshListing = () => useAppStore.getState().refreshListing(listingRequest);
const refreshSessions = () => useAppStore.getState().refreshSkillSessions(sessionsRequest);

function useHomeReport<T extends { refreshing: boolean }>(
  revision: number,
  active: boolean,
  load: T | undefined,
  refresh: () => void,
) {
  const requested = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!active || load?.refreshing || (load && requested.current === revision)) return;
    requested.current = revision;
    refresh();
  }, [active, revision, load, refresh]);
  return { load, retry: refresh };
}

export function useHomeListing(revision: number, active: boolean) {
  const load = useAppStore((s) => s.listingReports[listingKey]);
  return useHomeReport(revision, active, load, refreshListing);
}

export function useRecentSessions(revision: number, active: boolean) {
  const load = useAppStore((s) => s.sessionsReports[sessionsKey]);
  return useHomeReport(revision, active, load, refreshSessions);
}

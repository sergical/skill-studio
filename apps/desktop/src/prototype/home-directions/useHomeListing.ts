import { useEffect, useRef } from "react";
import { currentListingRequest } from "../../components/Listing/useSkillVisibility";
import { listingReportKey, useAppStore } from "../../store/appStore";

const request = currentListingRequest(null);
const key = listingReportKey(request);

export function useHomeListing(revision: number, active: boolean) {
  const load = useAppStore((s) => s.listingReports[key]);
  const refresh = useAppStore((s) => s.refreshListing);
  const requested = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!active || load?.refreshing || (load && requested.current === revision)) return;
    requested.current = revision;
    refresh(request);
  }, [active, revision, load, refresh]);
  return { load, retry: () => refresh(request) };
}

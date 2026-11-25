import { useEffect, useRef } from "react";
import { currentListingRequest } from "../Listing/useSkillVisibility";
import { listingReportKey, useAppStore } from "../../store/appStore";

const listingRequest = currentListingRequest(null);
const listingKey = listingReportKey(listingRequest);
const refreshListing = () => useAppStore.getState().refreshListing(listingRequest);

/** The global skill list report for Home, read again after each new scan while Home is shown. */
export function useHomeListing(revision: number, active: boolean) {
  const load = useAppStore((s) => s.listingReports[listingKey]);
  const requested = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (!active || load?.refreshing || (load && requested.current === revision)) return;
    requested.current = revision;
    refreshListing();
  }, [active, revision, load]);
  return { load, retry: refreshListing };
}

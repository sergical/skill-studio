import { currentListingRequest, useListingReport } from "../../hooks/useStoreReports";
import { useAppStore } from "../../store/appStore";

const refreshListing = () => useAppStore.getState().refreshListing(currentListingRequest(null));

/** The global skill list report for Home, read again after each new scan while Home is shown. */
export function useHomeListing(revision: number, active: boolean) {
  const load = useListingReport(null, String(revision), active);
  return { load, retry: refreshListing };
}

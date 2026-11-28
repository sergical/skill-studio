// @vitest-environment happy-dom

import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { ListingReport } from "@skill-studio/lib";
import { mockListingReport } from "../dev/harness/mock-listing";
import { buildHarnessSnapshot } from "../dev/harness/skill-fixture";
import { useAppStore } from "../store/appStore";
import { useListingReport } from "./useStoreReports";

afterEach(cleanup);

it("a new snapshot revision during a slow listing read reads again, so Home shows the new list", async () => {
  const reads: ((report: ListingReport) => void)[] = [];
  useAppStore.setState({
    listingReports: {},
    readListing: () => new Promise<ListingReport>((resolve) => reads.push(resolve)),
  });
  // Revision 1 lists Claude Code; revision 2 lists Codex.
  const [claude, codex] = mockListingReport(buildHarnessSnapshot(), {}).listings;
  const fromRevision1: ListingReport = { listings: [claude] };
  const fromRevision2: ListingReport = { listings: [codex] };

  const { result, rerender } = renderHook(({ hash }) => useListingReport(null, hash), {
    initialProps: { hash: "1" },
  });
  await waitFor(() => expect(reads).toHaveLength(1));
  rerender({ hash: "2" });
  reads[0](fromRevision1);

  await waitFor(() => expect(reads).toHaveLength(2));
  reads[1](fromRevision2);
  await waitFor(() =>
    expect(result.current?.data?.listings.map((listing) => listing.harness)).toEqual(["codex"]),
  );
});

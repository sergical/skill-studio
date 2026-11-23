// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { mockListingReport } from "../../dev/harness/mock-listing";
import { buildHarnessSnapshot } from "../../dev/harness/skill-fixture";
import { ClientConfiguration } from "./ClientConfiguration";

afterEach(cleanup);

it("shows a budget-free preview and a saved listing independently", () => {
  const row = mockListingReport(buildHarnessSnapshot(), {}).listings[0];
  const session = {
    transcript: "/record",
    at: null,
    model: "recorded-model",
    listed: 3,
    described: 2,
  };
  const report = { ...row, harness: "pi", budget: null, setting: null };
  const { rerender } = render(<ClientConfiguration row={report} onSaved={() => {}} />);
  expect(screen.getByLabelText("Listing sources")).toBeTruthy();
  expect(screen.queryByText(/Last recorded listing/)).toBeNull();
  rerender(
    <ClientConfiguration
      row={{ ...report, evidence: "discovery", observed_session: session }}
      onSaved={() => {}}
    />,
  );
  expect(screen.queryByLabelText("Listing sources")).toBeNull();
  expect(screen.getByText(/Last recorded listing.*3 entries.*recorded-model/)).toBeTruthy();
});

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
  expect(screen.getByRole("region", { name: /Where the \d+ skills come from/ })).toBeTruthy();
  expect(screen.queryByText(/Last session listed/)).toBeNull();
  rerender(
    <ClientConfiguration
      row={{ ...report, evidence: "discovery", observed_session: session }}
      onSaved={() => {}}
    />,
  );
  expect(screen.queryByRole("region", { name: /Where the \d+ skills come from/ })).toBeNull();
  expect(screen.getByText(/Last session listed 3 skills.*recorded-model/)).toBeTruthy();
});

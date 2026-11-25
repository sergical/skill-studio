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

it("counts skills the budget left out in the headline, so a full budget never reads as every description fits", () => {
  const row = mockListingReport(buildHarnessSnapshot(), {}).listings[0];
  const [first] = row.entries;
  const entries = [
    { ...first, name: "a", path: "/a", state: "described" as const },
    { ...first, name: "b", path: "/b", state: "described" as const },
    { ...first, name: "c", path: "/c", state: "omitted" as const },
  ];
  render(<ClientConfiguration row={{ ...row, entries }} onSaved={() => {}} />);
  expect(screen.getByText("of 3 skills keep a description")).toBeTruthy();
  expect(screen.queryByText(/Every description fits/)).toBeNull();
});

// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Sidebar } from "./Sidebar";
import { useAppStore } from "../../store/appStore";
afterEach(cleanup);
it.each(["dark", "light"] as const)(
  "offers to switch to the other theme in %s mode",
  (resolvedTheme) => {
    useAppStore.setState({ resolvedTheme });
    render(
      <Sidebar snapshot={undefined} emittedSnapshotRevision={0} requestRescan={async () => {}} />,
    );
    const target = resolvedTheme === "dark" ? "light" : "dark";
    expect(screen.getByRole("button", { name: `Switch to ${target} theme` })).toBeTruthy();
    expect(screen.queryByRole("button", { name: `Switch to ${resolvedTheme} theme` })).toBeNull();
  },
);

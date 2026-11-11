// @vitest-environment happy-dom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Sidebar } from "./Sidebar";
import { useAppStore } from "../../store/appStore";
afterEach(cleanup);
it.each(["dark", "light"] as const)("shows the target theme icon in %s mode", (resolvedTheme) => {
  useAppStore.setState({ resolvedTheme });
  render(
    <Sidebar snapshot={undefined} emittedSnapshotRevision={0} requestRescan={async () => {}} />,
  );
  const target = resolvedTheme === "dark" ? "light" : "dark";
  const button = screen.getByRole("button", { name: `Switch to ${target} theme` });
  expect(button.querySelector(target === "light" ? ".lucide-sun" : ".lucide-moon")).toBeTruthy();
});

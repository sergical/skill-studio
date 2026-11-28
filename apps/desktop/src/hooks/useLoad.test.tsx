// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";
import { useLoad } from "./useLoad";

Reflect.set(globalThis, "IS_REACT_ACT_ENVIRONMENT", true);

function Shown({ path, read }: { path: string; read: (path: string) => Promise<string> }) {
  const file = useLoad(path, () => read(path));
  return <output>{file.isLoading ? "loading" : file.data}</output>;
}

it("a slow read of the file shown before never replaces the file shown now", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const pending = new Map<string, (content: string) => void>();
  const read = (path: string) =>
    new Promise<string>((resolve) => {
      pending.set(path, resolve);
    });
  try {
    await act(async () => root.render(<Shown path="a/SKILL.md" read={read} />));
    await act(async () => root.render(<Shown path="b/SKILL.md" read={read} />));
    expect(host.textContent).toBe("loading");
    await act(async () => pending.get("b/SKILL.md")?.("content of b"));
    expect(host.textContent).toBe("content of b");
    await act(async () => pending.get("a/SKILL.md")?.("content of a"));
    expect(host.textContent).toBe("content of b");
  } finally {
    await act(async () => root.unmount());
    host.remove();
  }
});

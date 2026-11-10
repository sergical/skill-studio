// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Toaster, toast } from "sonner";
import { afterEach, expect, it } from "vitest";
import { HomeView } from "./HomeView";
import { buildHarnessSnapshot, deployment, skill } from "../../dev/harness/skill-fixture";
import { getActiveUpdateAll } from "../../hooks/skillBatchUpdates";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function snapshot(plugin: boolean) {
  return {
    ...buildHarnessSnapshot(),
    invocations: [],
    skills: ["alpha", "beta", "gamma"].map((name) => {
      const owner = plugin ? `plugin:${name}@market` : `owner:v1/global/${name}`;
      return skill({
        name,
        source_kind: plugin ? "plugin" : "skills-sh",
        has_update: true,
        deployments: [
          deployment({
            agent: "shared",
            scope: "global",
            path: `/fixture/.agents/skills/${name}`,
            owner_kind: plugin ? "plugin" : "skills-sh",
            backing: { kind: "canonical" },
          }),
        ],
        update_owner_ids: [owner],
        update_owners: [
          {
            owner_id: owner,
            latest_commit: "new",
            latest_commit_at: null,
            plugin_scope: "user",
            plugin_project_path: null,
          },
        ],
      });
    }),
  };
}
afterEach(() => {
  cleanup();
  clearMocks();
  toast.dismiss();
});

it("Update all shows Updating during the edits check without flashing progress or permits a second batch", async () => {
  const check = deferred<unknown>();
  let checks = 0;
  mockIPC(
    (command) => {
      if (command === "skill_local_edits") {
        checks += 1;
        return check.promise;
      }
      if (command === "update_all_skills") return { items: [], errors: {}, not_run: [] };
      throw new Error(`Unexpected ${command}`);
    },
    { shouldMockEvents: true },
  );
  const user = userEvent.setup();
  render(<HomeView snapshot={snapshot(false)} isLoading={false} active onSelectSkill={() => {}} />);
  const update = screen.getByRole("button", { name: "Update all" });
  for (let i = 0; i < 24 && document.activeElement !== update; i += 1) await user.tab();
  expect(document.activeElement).toBe(update);
  await user.keyboard("{Enter}");
  const checking = screen.getByRole("button", { name: "Updating…" });
  expect(checking.hasAttribute("disabled")).toBe(true);
  expect(screen.queryByRole("progressbar")).toBeNull();
  expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
  await user.click(checking);
  expect(checks).toBe(1);
  check.resolve([]);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Update all" }).hasAttribute("disabled")).toBe(false),
  );
});

it.each([
  { key: "{Enter}", label: "Enter" },
  { key: " ", label: "Space" },
])(
  "Cancel with $label retries a failed stop and waits for its acknowledgement or starts the next skill",
  async ({ key }) => {
    const current = deferred<{ outcome: "updated"; message: null }>();
    const stop = deferred<void>();
    const updated: unknown[] = [];
    let stops = 0;
    mockIPC(
      (command, args) => {
        if (command === "update_plugin") {
          updated.push(args);
          return current.promise;
        }
        if (command === "cancel_update_all") {
          stops += 1;
          if (stops === 1) throw new Error("Stop request failed");
          return stop.promise;
        }
        throw new Error(`Unexpected ${command}`);
      },
      { shouldMockEvents: true },
    );
    const user = userEvent.setup();
    render(
      <>
        <HomeView snapshot={snapshot(true)} isLoading={false} active onSelectSkill={() => {}} />
        <Toaster />
      </>,
    );
    await user.click(screen.getByRole("button", { name: "Update all" }));
    let cancel = await screen.findByRole("button", { name: "Cancel" });
    for (let i = 0; i < 24 && document.activeElement !== cancel; i += 1) await user.tab();
    expect(document.activeElement).toBe(cancel);
    await user.keyboard(key);
    expect(await screen.findByText("Could not stop Update all")).toBeTruthy();
    cancel = screen.getByRole("button", { name: "Cancel" });
    expect(cancel.hasAttribute("disabled")).toBe(false);
    await user.click(cancel);
    expect(screen.getByRole("button", { name: "Stopping…" }).hasAttribute("disabled")).toBe(true);
    current.resolve({ outcome: "updated", message: null });
    await waitFor(() => expect(screen.getByText(/Updating 1 of 3/)).toBeTruthy());
    expect(updated).toHaveLength(1);
    expect(getActiveUpdateAll()).not.toBeNull();
    stop.resolve();
    expect(await screen.findByText("Stopped. Updated 1 of 3 skills.")).toBeTruthy();
    expect(updated).toHaveLength(1);
    expect(stops).toBe(2);
    expect(getActiveUpdateAll()).toBeNull();
    expect(screen.getByRole("button", { name: "Update all" }).hasAttribute("disabled")).toBe(false);
  },
);

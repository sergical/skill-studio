// @vitest-environment happy-dom

import { useRef, useState } from "react";
import { Button } from "@skill-studio/ui";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClientInfo, RunRecord, TestSuite } from "@skill-studio/lib";
import { SkillLab } from "./SkillLab";
import { SkillAssistantWorkspace } from "./SkillAssistantWorkspace";
import { newSuite, newCase, comparison } from "./lab-model";
import { installSkillLabPreview } from "../../dev/harness/skill-lab-preview-ipc";
import { recordedRun } from "../../dev/harness/skill-lab-recording";
import { isModalSurfaceOpen } from "../../lib/app-shortcuts";

function TestWorkspace() {
  const [open, setOpen] = useState(false);
  const [opened, setOpened] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <>
      <div hidden={open}>
        <Button
          ref={trigger}
          onClick={() => {
            setOpened(true);
            setOpen(true);
          }}
        >
          Assistant
        </Button>
      </div>
      {opened && (
        <SkillAssistantWorkspace
          open={open}
          skillName="report"
          skillPath="/skills/report/SKILL.md"
          onBack={() => {
            setOpen(false);
            requestAnimationFrame(() => trigger.current?.focus());
          }}
        />
      )}
    </>
  );
}

async function openAdvanced(user: ReturnType<typeof userEvent.setup>) {
  const summary = screen.getByText("Advanced options");
  if (!summary.parentElement?.hasAttribute("open")) await user.click(summary);
}

const clients: ClientInfo[] = [
  {
    id: "codex",
    available: true,
    binary: "/bin/codex",
    version: "Test client",
    no_skill_control: false,
    note: "Sign-in is checked on start",
  },
];

function savedRun(): RunRecord {
  const suite = newSuite("report", "/skills/report/SKILL.md");
  suite.cases[0].prompt = "Write a report";
  return {
    schema_version: 1,
    id: "saved-run",
    suite,
    input_hash: "frozen-inputs",
    created_at: "2026-11-09T12:00:00Z",
    finished_at: null,
    state: "running",
    error: null,
    trials: [
      {
        id: "trial-1",
        case_id: suite.cases[0].id,
        variant_id: "candidate",
        repetition: 1,
        state: "running",
        started_at: "2026-11-09T12:00:00Z",
        finished_at: null,
        client_binary: "/bin/codex",
        client_version: "Test client",
        isolation: "Other user skills may be available",
        skill_hash: "snapshot",
        result: null,
        judge: null,
        checks: [],
        outcome: "not_evaluated",
        artifacts: [],
        reviews: [],
        error: null,
      },
    ],
  };
}

function installMock(suites: TestSuite[] = []) {
  const called = vi.fn();
  const run = savedRun();
  mockIPC((command, args) => {
    called(command, args);
    if (command === "lab_clients") return clients;
    if (command === "lab_suites") return suites;
    if (command === "lab_runs") return [];
    if (command === "lab_start") return run.id;
    if (command === "lab_run") return run;
    if (command === "lab_save_suite" || command === "lab_cancel") return null;
    if (command === "lab_save_export") return true;
    if (command === "lab_import_suite") throw new Error("Suite version is not supported");
    return null;
  });
  return { called, run };
}

beforeEach(() => {
  localStorage.setItem("feature:skill-assistant", "on");
});

afterEach(() => {
  localStorage.clear();
  cleanup();
  clearMocks();
});

describe("Skill testing workspace", () => {
  it("keeps input filenames unique after removing a middle file and adding another", async () => {
    const { called } = installMock();
    const user = userEvent.setup();
    render(<TestWorkspace />);
    await user.click(screen.getByRole("button", { name: "Assistant" }));
    await user.type(await screen.findByLabelText("Task"), "Read the supplied files");
    const addFile = screen.getByRole("button", { name: "Add file" });
    await user.click(addFile);
    await user.click(addFile);
    await user.click(addFile);
    await user.type(screen.getByLabelText("File 3 contents"), "Keep this content");
    await user.click(screen.getByRole("button", { name: "Remove file 2" }));
    await user.click(addFile);
    await user.click(screen.getByRole("button", { name: "Run task" }));
    await waitFor(() =>
      expect(called.mock.calls.some(([command]) => command === "lab_start")).toBe(true),
    );
    const started = called.mock.calls.find(([command]) => command === "lab_start")?.[1].suite;
    expect(started.cases[0].files).toEqual([
      { path: "input.md", content: "" },
      { path: "input-3.txt", content: "Keep this content" },
      { path: "input-2.txt", content: "" },
    ]);
  });

  it("hides the entire test entry and makes no lab requests when the feature is off", () => {
    localStorage.setItem("feature:skill-assistant", "off");
    const { called } = installMock();
    render(
      <SkillAssistantWorkspace
        open
        skillName="report"
        skillPath="/skills/report/SKILL.md"
        onBack={() => {}}
      />,
    );
    expect(screen.queryByRole("button", { name: "Assistant" })).toBeNull();
    expect(called).not.toHaveBeenCalled();
  });

  it("opens a task-first page and keeps the draft when returning to the skill", async () => {
    installMock();
    const user = userEvent.setup();
    render(<TestWorkspace />);
    const trigger = screen.getByRole("button", { name: "Assistant" });
    await user.click(trigger);
    await user.type(await screen.findByLabelText("Task"), "Keep this unsaved task");
    expect(isModalSurfaceOpen()).toBe(false);
    expect(screen.getByText("Advanced options").parentElement?.hasAttribute("open")).toBe(false);
    expect(screen.getByRole("combobox", { name: "Saved suite" }).closest("details")?.open).toBe(
      false,
    );
    await user.click(screen.getByRole("button", { name: "report" }));
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(isModalSurfaceOpen()).toBe(false);
    await user.click(trigger);
    expect((await screen.findByLabelText<HTMLTextAreaElement>("Task")).value).toBe(
      "Keep this unsaved task",
    );
    await user.click(screen.getByRole("button", { name: "report" }));
    await user.click(trigger);
    expect((await screen.findByLabelText<HTMLTextAreaElement>("Task")).value).toBe(
      "Keep this unsaved task",
    );
  });

  it.each([
    {
      kind: "output_json" as const,
      label: "Expected JSON value",
      invalid: '{"status":',
      valid: '{"status":"ready"}',
    },
    {
      kind: "command" as const,
      label: "Command arguments as JSON",
      invalid: "[3]",
      valid: '["--version"]',
    },
  ])(
    "retains invalid $kind drafts across cases, views and back navigation, and blocks stale execution",
    async ({ kind, label, invalid, valid }) => {
      const suite = newSuite("JSON checks", "/skills/report/SKILL.md");
      suite.cases[0].prompt = "Write a report";
      suite.cases[0].checks[0].assertion =
        kind === "output_json"
          ? { kind, pointer: "/status", equals: "ok" }
          : { kind, program: "python3", args: [], expected_exit: 0 };
      suite.cases.push({ ...newCase(), name: "Second task", prompt: "Another task" });
      const { called } = installMock([suite]);
      const user = userEvent.setup();
      render(<TestWorkspace />);
      await user.click(screen.getByRole("button", { name: "Assistant" }));
      await screen.findByLabelText("Task");
      await openAdvanced(user);
      await user.click(screen.getByRole("combobox", { name: "Saved suite" }));
      await user.click(await screen.findByRole("option", { name: "JSON checks tests" }));
      await openAdvanced(user);
      await user.clear(screen.getByLabelText(label));
      await user.click(screen.getByLabelText(label));
      await user.paste(invalid);
      await user.click(screen.getByRole("button", { name: "Second task" }));
      await user.click(screen.getByRole("button", { name: "Saved runs" }));
      await user.click(screen.getByRole("button", { name: "Task" }));
      await user.click(screen.getByRole("button", { name: "report" }));
      await user.click(screen.getByRole("button", { name: "Assistant" }));
      await openAdvanced(user);
      expect(screen.getByRole("alert").textContent).toContain("First task");
      for (const name of ["Save suite", "Run 2 tests", "Export suite"]) {
        expect(screen.getByRole<HTMLButtonElement>("button", { name }).disabled).toBe(true);
        await user.click(screen.getByRole("button", { name }));
      }
      expect(
        called.mock.calls.some(([command]) =>
          ["lab_start", "lab_save_suite", "lab_save_export"].includes(command),
        ),
      ).toBe(false);
      await user.click(screen.getByRole("button", { name: "First task · Invalid JSON" }));
      expect(screen.getByLabelText<HTMLTextAreaElement>(label).value).toBe(invalid);
      expect(screen.getByLabelText(label).getAttribute("aria-invalid")).toBe("true");
      await user.clear(screen.getByLabelText(label));
      await user.click(screen.getByLabelText(label));
      await user.paste(valid);
      await openAdvanced(user);
      await user.click(screen.getByRole("button", { name: "Save suite" }));
      await screen.findByText(/Suite saved/);
      const saved = called.mock.calls.find(([command]) => command === "lab_save_suite")?.[1].suite;
      expect(saved.cases[0].checks[0].assertion[kind === "command" ? "args" : "equals"]).toEqual(
        JSON.parse(valid),
      );
      const checkType = screen.getByRole("combobox", {
        name: "Check type for Task quality",
      });
      await user.click(checkType);
      await user.click(await screen.findByRole("option", { name: "File exists" }));
      await user.click(checkType);
      await user.click(
        await screen.findByRole("option", {
          name: kind === "command" ? "Acceptance command" : "Final response JSON matches",
        }),
      );
      const shown = JSON.parse(screen.getByLabelText<HTMLTextAreaElement>(label).value);
      await openAdvanced(user);
      await user.click(screen.getByRole("button", { name: "Save suite" }));
      await waitFor(() =>
        expect(called.mock.calls.filter(([command]) => command === "lab_save_suite")).toHaveLength(
          2,
        ),
      );
      const reset = called.mock.calls.filter(([command]) => command === "lab_save_suite")[1][1]
        .suite;
      expect(reset.cases[0].checks[0].assertion[kind === "command" ? "args" : "equals"]).toEqual(
        shown,
      );
    },
  );

  it("saves a reusable case without launching a client", async () => {
    const { called } = installMock();
    const user = userEvent.setup();
    render(<SkillLab skillName="report" skillPath="/skills/report/SKILL.md" />);
    await user.type(await screen.findByLabelText("Task"), "Write a report from input.txt");
    await openAdvanced(user);
    await user.click(screen.getByRole("button", { name: "Save suite" }));
    await screen.findByText(/Suite saved/);
    expect(called.mock.calls.some(([command]) => command === "lab_start")).toBe(false);
    const save = called.mock.calls.find(([command]) => command === "lab_save_suite");
    expect(save?.[1].suite.cases[0].prompt).toBe("Write a report from input.txt");
  });

  it("keeps a backend run active when returning to the skill", async () => {
    const { called } = installMock();
    const user = userEvent.setup();
    render(<TestWorkspace />);
    await user.click(screen.getByRole("button", { name: "Assistant" }));
    await user.type(await screen.findByLabelText("Task"), "Write a report");
    await user.click(screen.getByRole("button", { name: "Run task" }));
    await screen.findByRole("button", { name: "Stop run" });
    await user.click(screen.getByRole("button", { name: "report" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Assistant" }));
    expect(screen.getByRole("button", { name: "Stop run" })).toBeTruthy();
    expect(called.mock.calls.some(([command]) => command === "lab_cancel")).toBe(false);
  });

  it("reports an invalid import and does not launch its commands", async () => {
    const { called } = installMock();
    const user = userEvent.setup();
    render(<SkillLab skillName="report" skillPath="/skills/report/SKILL.md" />);
    await screen.findByLabelText("Task");
    await openAdvanced(user);
    await user.click(screen.getByRole("button", { name: "Import suite" }));
    await user.type(screen.getByLabelText("Suite JSON to import"), "invalid suite");
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "Suite version is not supported",
    );
    expect(called.mock.calls.some(([command]) => command === "lab_start")).toBe(false);
  });

  it("shows the frozen input again when editing a previous run of the same suite", async () => {
    const suite = newSuite("report", "/skills/report/SKILL.md");
    suite.cases[0].prompt = "Rewrite input.md";
    suite.cases[0].files = [{ path: "input.md", content: "Original input" }];
    const { called, run } = installMock([suite]);
    run.suite = suite;
    run.state = "completed";
    run.trials = [];
    const user = userEvent.setup();
    render(<SkillLab skillName="report" skillPath="/skills/report/SKILL.md" />);
    await screen.findByLabelText("Task");
    await openAdvanced(user);
    await user.click(screen.getByRole("combobox", { name: "Saved suite" }));
    await user.click(await screen.findByRole("option", { name: "report tests" }));
    await user.click(screen.getByRole("button", { name: "Run task" }));
    await screen.findByRole("button", { name: "Edit and run again" });
    await user.click(screen.getByRole("button", { name: "Task" }));
    await user.clear(screen.getByLabelText("File 1 contents"));
    await user.type(screen.getByLabelText("File 1 contents"), "Changed draft");
    await user.click(screen.getByRole("button", { name: "Result" }));
    await user.click(screen.getByRole("button", { name: "Edit and run again" }));
    expect(screen.getByLabelText<HTMLTextAreaElement>("File 1 contents").value).toBe(
      "Original input",
    );
    await user.click(screen.getByRole("button", { name: "Run task" }));
    await waitFor(() =>
      expect(called.mock.calls.filter(([command]) => command === "lab_start")).toHaveLength(2),
    );
    expect(
      called.mock.calls.filter(([command]) => command === "lab_start")[1][1].suite.cases[0].files[0]
        .content,
    ).toBe("Original input");
  });

  it("opens saved native results and exposes the actual report artifact", async () => {
    installSkillLabPreview();
    const user = userEvent.setup();
    render(<SkillLab skillName="report-writer" skillPath={recordedRun.suite.skill_path} />);
    await screen.findByLabelText("Task");
    await user.click(screen.getByRole("button", { name: "Saved runs" }));
    await user.click(
      await screen.findByRole("button", { name: /Report writer across installed clients/ }),
    );
    await user.click(await screen.findByText(/Compare results/));
    expect(
      (await screen.findByRole("table", { name: "Results by variant" })).textContent,
    ).toContain("Claude Code");
    const artifact = screen.getByText(/report.md · created/);
    if (!artifact.parentElement?.hasAttribute("open")) await user.click(artifact);
    expect(artifact.parentElement?.hasAttribute("open")).toBe(true);
    expect(artifact.parentElement?.textContent).toContain("Verified from the source facts.");
    expect(artifact.parentElement?.textContent).toContain("two fixes");
    expect(screen.getByText("Client execution").parentElement?.textContent).toContain("completed");
    expect(screen.getByText("Task checks").parentElement?.textContent).toContain("passed");
    await user.type(screen.getByLabelText("Review reason"), "Keep this review draft");
    await user.click(screen.getByRole("button", { name: /Write a project report · Claude Code/ }));
    expect(screen.getByLabelText<HTMLTextAreaElement>("Review reason").value).toBe("");
    await user.click(screen.getByRole("button", { name: /Write a project report · Codex/ }));
    expect(screen.getByLabelText<HTMLTextAreaElement>("Review reason").value).toBe(
      "Keep this review draft",
    );
    await user.click(screen.getByRole("button", { name: "Task" }));
    await user.click(screen.getByRole("button", { name: "Result" }));
    expect(screen.getByLabelText<HTMLTextAreaElement>("Review reason").value).toBe(
      "Keep this review draft",
    );
  });

  it("exports suite JSON through the native save operation and scopes history to the selected skill", async () => {
    const { called } = installMock();
    const user = userEvent.setup();
    render(<SkillLab skillName="report" skillPath="/skills/report/SKILL.md" />);
    await screen.findByLabelText("Task");
    await openAdvanced(user);
    await user.click(screen.getByRole("button", { name: "Export suite" }));
    await screen.findByText("JSON file saved.");
    const exported = called.mock.calls.find(([command]) => command === "lab_save_export");
    expect(JSON.parse(exported?.[1].json).skill_path).toBe("/skills/report/SKILL.md");
    expect(called.mock.calls.find(([command]) => command === "lab_runs")?.[1]).toEqual({
      skillPath: "/skills/report/SKILL.md",
    });
    expect(called.mock.calls.find(([command]) => command === "lab_suites")?.[1]).toEqual({
      skillPath: "/skills/report/SKILL.md",
    });
  });

  it("does not count an unevaluated run as a passing test", () => {
    const row = comparison(savedRun())[0];
    expect(row.passed).toBe(0);
    expect(row.evaluated).toBe(0);
    expect(row.completed).toBe(0);
    expect(row.costs).toEqual([]);
  });
});

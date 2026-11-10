import { z } from "zod";
import type { Assertion, Check, RunRecord, TestCase, TestSuite } from "@skill-studio/lib";

export const CLIENTS = [
  { value: "codex", label: "Codex" },
  { value: "claude-code", label: "Claude Code" },
  { value: "open-code", label: "OpenCode" },
  { value: "pi", label: "pi" },
];

export const CHECK_TYPES = [
  { value: "file_exists", label: "File exists" },
  { value: "file_contains", label: "File contains text" },
  { value: "output_contains", label: "Final response contains" },
  { value: "output_excludes", label: "Final response excludes" },
  { value: "output_json", label: "Final response JSON matches" },
  { value: "tool_called", label: "Tool was used" },
  { value: "skill_loaded", label: "Skill was loaded" },
  { value: "rubric", label: "Quality criterion" },
  { value: "command", label: "Acceptance command" },
];

export function id(prefix: string): string {
  return `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
}

export function newCheck(): Check {
  return {
    id: id("check"),
    name: "Expected output file",
    assertion: { kind: "file_exists", path: "report.md" },
  };
}

export function newCase(): TestCase {
  return {
    id: id("case"),
    name: "New case",
    prompt: "",
    mode: "explicit",
    files: [],
    checks: [
      {
        id: id("check"),
        name: "Task quality",
        assertion: {
          kind: "rubric",
          criteria: "Does the output fulfill the task and preserve the supplied facts?",
        },
      },
    ],
  };
}

export function newSuite(name: string, skillPath: string): TestSuite {
  return {
    schema_version: 1,
    id: id("suite"),
    name: `${name} tests`,
    skill_path: skillPath,
    cases: [{ ...newCase(), name: "First task" }],
    variants: [
      {
        id: "candidate",
        name: "Current skill",
        client: "codex",
        model: null,
        skill: { kind: "candidate" },
      },
    ],
    repetitions: 1,
    timeout_seconds: 180,
    judge: null,
  };
}

export function defaultAssertion(kind: string): Assertion {
  switch (kind) {
    case "file_contains":
      return { kind, path: "report.md", text: "" };
    case "output_contains":
    case "output_excludes":
      return { kind, text: "" };
    case "output_json":
      return { kind, pointer: "/status", equals: "ok" };
    case "tool_called":
      return { kind, name: "read" };
    case "skill_loaded":
      return { kind, expected: true };
    case "rubric":
      return { kind, criteria: "" };
    case "command":
      return {
        kind,
        program: "python3",
        args: ["-c", "from pathlib import Path; assert Path('report.md').is_file()"],
        expected_exit: 0,
      };
    default:
      return { kind: "file_exists", path: "report.md" };
  }
}

export function activeRun(run: RunRecord | null): boolean {
  return run?.state === "queued" || run?.state === "running";
}

export function comparison(run: RunRecord) {
  return run.suite.variants.map((variant) => {
    const trials = run.trials.filter((trial) => trial.variant_id === variant.id);
    const passed = trials.filter((trial) => trial.outcome === "passed").length;
    const failed = trials.filter((trial) => trial.outcome === "failed").length;
    const completed = trials.filter((trial) => trial.result?.state === "completed").length;
    const costs = trials.flatMap((trial) =>
      trial.result?.usage.cost_usd == null ? [] : [trial.result.usage.cost_usd],
    );
    return {
      variant,
      passed,
      failed,
      evaluated: passed + failed,
      total: trials.length,
      completed,
      costs,
      needsReview: trials.filter(
        (trial) => trial.outcome === "needs_review" || trial.outcome === "check_error",
      ).length,
    };
  });
}

export function parseClient(value: string) {
  return z.enum(["codex", "claude-code", "open-code", "pi"]).parse(value);
}

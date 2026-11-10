import { z } from "zod";
import type { Check, TestSuite } from "@skill-studio/lib";

export type JsonDrafts = Record<string, string>;

export function jsonDraftKey(caseId: string, check: Check): string {
  return JSON.stringify([caseId, check.id, check.assertion.kind]);
}

export function parseCheckJson(check: Check, text: string): Check {
  const assertion = check.assertion;
  if (assertion.kind === "command") {
    return {
      ...check,
      assertion: { ...assertion, args: z.array(z.string()).parse(JSON.parse(text)) },
    };
  }
  if (assertion.kind === "output_json") {
    return { ...check, assertion: { ...assertion, equals: z.json().parse(JSON.parse(text)) } };
  }
  return check;
}

export function jsonDraftErrors(suite: TestSuite, drafts: JsonDrafts) {
  return suite.cases.flatMap((testCase) =>
    testCase.checks.flatMap((check) => {
      const key = jsonDraftKey(testCase.id, check);
      const text = drafts[key];
      if (text === undefined) return [];
      try {
        parseCheckJson(check, text);
        return [];
      } catch {
        return [
          {
            key,
            caseId: testCase.id,
            caseName: testCase.name,
            message:
              check.assertion.kind === "command"
                ? "Enter command arguments as a JSON array of strings."
                : "Enter a valid JSON value.",
          },
        ];
      }
    }),
  );
}

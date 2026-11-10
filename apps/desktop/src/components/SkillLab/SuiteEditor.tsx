import { useState, type ReactNode } from "react";
import { Button, Input, Textarea } from "@skill-studio/ui";
import type { ClientInfo, TestCase, TestSuite } from "@skill-studio/lib";
import { SelectControl } from "../ui/SelectControl";
import { CheckEditor } from "./CheckEditor";
import { VariantEditor } from "./VariantEditor";
import { FixtureEditor } from "./FixtureEditor";
import { CLIENTS, newCase, newCheck, parseClient } from "./lab-model";
import { jsonDraftErrors, jsonDraftKey, type JsonDrafts } from "./lab-json-drafts";

interface Props {
  suite: TestSuite;
  clients: ClientInfo[];
  busy: boolean;
  onChange: (suite: TestSuite) => void;
  onSave: () => void;
  onRun: () => void;
  savedTests: ReactNode;
  jsonDrafts: JsonDrafts;
  onJsonChange: (key: string, text: string | undefined) => void;
}

export function SuiteEditor({
  suite,
  clients,
  busy,
  onChange,
  onSave,
  onRun,
  jsonDrafts,
  onJsonChange,
  savedTests,
}: Props) {
  const [caseId, setCaseId] = useState(suite.cases[0]?.id);
  const selectedCase = suite.cases.find((item) => item.id === caseId) ?? suite.cases[0];
  const updateCase = (next: TestCase) =>
    onChange({ ...suite, cases: suite.cases.map((item) => (item.id === next.id ? next : item)) });
  const count = suite.cases.length * suite.variants.length * suite.repetitions;
  const jsonErrors = jsonDraftErrors(suite, jsonDrafts);
  const unavailable = suite.variants.some(
    (variant) => !clients.find((client) => client.id === variant.client)?.available,
  );
  return (
    <form
      className="space-y-6"
      onInvalidCapture={(event) => {
        if (event.target instanceof HTMLElement) {
          const details = event.target.closest("details");
          if (details) details.open = true;
        }
      }}
      onSubmit={(event) => {
        event.preventDefault();
        if (!jsonErrors.length) onRun();
      }}
    >
      {jsonErrors.length > 0 && (
        <div role="alert" className="rounded-md border border-border p-3 text-body">
          Fix the JSON in{" "}
          {Array.from(new Set(jsonErrors.map((error) => error.caseName))).join(", ")} before saving,
          exporting, or running this suite.
        </div>
      )}
      <div className="space-y-2">
        <h2 className="text-title font-semibold">Try a real task</h2>
        <p className="max-w-[65ch] text-body text-text-secondary">
          Describe what you need. The assistant will use this skill in a fresh working folder.
        </p>
      </div>
      {suite.cases.length > 1 && (
        <SelectControl
          size="lg"
          ariaLabel="Task to edit"
          value={selectedCase?.id}
          items={suite.cases.map((item) => ({ value: item.id, label: item.name }))}
          onValueChange={setCaseId}
        />
      )}
      {selectedCase && (
        <div key={selectedCase.id} className="space-y-5">
          <label className="block space-y-1 text-caption text-text-secondary">
            Task
            <Textarea
              value={selectedCase.prompt}
              required
              rows={5}
              className="min-h-32"
              placeholder="For example: Rewrite input.md as a short product update. Keep the facts and save the result to report.md."
              onChange={(event) => updateCase({ ...selectedCase, prompt: event.target.value })}
            />
          </label>
          <FixtureEditor
            files={selectedCase.files}
            onChange={(files) => updateCase({ ...selectedCase, files })}
          />
        </div>
      )}
      <details className="rounded-md border border-border">
        <summary className="cursor-pointer px-4 text-body font-medium">Advanced options</summary>
        <div className="space-y-6 border-t border-border p-4">
          <p className="text-caption text-text-secondary">
            Checks, clients, comparisons, and saved tests.
          </p>
          {savedTests}
          <label className="block space-y-1 text-caption text-text-secondary">
            Suite name
            <Input
              value={suite.name}
              required
              onChange={(event) => onChange({ ...suite, name: event.target.value })}
            />
          </label>
          <section aria-label="Test cases" className="space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="text-emphasis font-semibold">Cases and expected results</h3>
              <Button
                type="button"
                variant="ghost"
                onClick={() => {
                  const next = newCase();
                  onChange({ ...suite, cases: [...suite.cases, next] });
                  setCaseId(next.id);
                }}
              >
                Add case
              </Button>
            </div>
            <div className="flex flex-wrap gap-2" aria-label="Choose a test case">
              {suite.cases.map((item) => (
                <Button
                  key={item.id}
                  type="button"
                  variant={item.id === selectedCase?.id ? "secondary" : "ghost"}
                  aria-pressed={item.id === selectedCase?.id}
                  onClick={() => setCaseId(item.id)}
                >
                  {item.name}
                  {jsonErrors.some((error) => error.caseId === item.id) && " · Invalid JSON"}
                </Button>
              ))}
            </div>
            {selectedCase && (
              <div key={selectedCase.id} className="space-y-4 rounded-md border border-border p-4">
                <div className="flex flex-wrap gap-2">
                  <Input
                    aria-label="Case name"
                    value={selectedCase.name}
                    required
                    onChange={(event) => updateCase({ ...selectedCase, name: event.target.value })}
                    className="flex-1"
                  />
                  <SelectControl
                    size="lg"
                    ariaLabel="Skill loading mode"
                    value={selectedCase.mode}
                    items={[
                      { value: "explicit", label: "Ask to use the skill" },
                      { value: "discovery", label: "Test natural selection" },
                    ]}
                    onValueChange={(mode) =>
                      updateCase({
                        ...selectedCase,
                        mode: mode === "discovery" ? "discovery" : "explicit",
                      })
                    }
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={suite.cases.length === 1}
                    onClick={() =>
                      onChange({
                        ...suite,
                        cases: suite.cases.filter((item) => item.id !== selectedCase.id),
                      })
                    }
                  >
                    Remove case
                  </Button>
                </div>
                <div className="space-y-2">
                  <div className="flex items-center justify-between">
                    <h4 className="text-body font-medium">Expected results</h4>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() =>
                        updateCase({
                          ...selectedCase,
                          checks: [...selectedCase.checks, newCheck()],
                        })
                      }
                    >
                      Add check
                    </Button>
                  </div>
                  {selectedCase.checks.map((check) => (
                    <CheckEditor
                      key={check.id}
                      check={check}
                      jsonText={jsonDrafts[jsonDraftKey(selectedCase.id, check)]}
                      jsonError={
                        jsonErrors.find(
                          (error) => error.key === jsonDraftKey(selectedCase.id, check),
                        )?.message
                      }
                      onJsonChange={(text) =>
                        onJsonChange(jsonDraftKey(selectedCase.id, check), text)
                      }
                      onChange={(next) =>
                        updateCase({
                          ...selectedCase,
                          checks: selectedCase.checks.map((item) =>
                            item.id === next.id ? next : item,
                          ),
                        })
                      }
                      onRemove={() =>
                        updateCase({
                          ...selectedCase,
                          checks: selectedCase.checks.filter((item) => item.id !== check.id),
                        })
                      }
                    />
                  ))}
                </div>
              </div>
            )}
          </section>
          <VariantEditor suite={suite} clients={clients} onChange={onChange} />
          <section className="space-y-3" aria-label="Separate judge">
            <h3 className="text-emphasis font-semibold">Quality review</h3>
            <p className="text-body text-text-secondary">
              File and text checks run first. Quality criteria need human review or a separate
              client with tools disabled. The judge uses another model request.
            </p>
            <div className="flex flex-wrap gap-2">
              <SelectControl
                size="lg"
                ariaLabel="Quality judge"
                value={suite.judge?.client ?? "human"}
                items={[
                  { value: "human", label: "Human review" },
                  { value: "claude-code", label: "Claude Code judge" },
                  { value: "pi", label: "pi judge" },
                ]}
                onValueChange={(client) =>
                  onChange({
                    ...suite,
                    judge: client === "human" ? null : { client: parseClient(client), model: null },
                  })
                }
              />
              {suite.judge && (
                <Input
                  aria-label="Judge model"
                  placeholder="Client default model"
                  value={suite.judge.model ?? ""}
                  onChange={(event) =>
                    onChange({
                      ...suite,
                      judge: {
                        client: suite.judge?.client ?? "pi",
                        model: event.target.value || null,
                      },
                    })
                  }
                />
              )}
            </div>
          </section>
          <Button
            type="button"
            variant="outline"
            disabled={busy || jsonErrors.length > 0}
            onClick={(event) => {
              if (event.currentTarget.form?.reportValidity()) onSave();
            }}
          >
            Save suite
          </Button>
        </div>
      </details>
      {unavailable && (
        <p role="alert" className="text-body text-text-secondary">
          A selected client is unavailable. Open Advanced options to choose an installed client.
        </p>
      )}
      <div className="sticky bottom-0 flex flex-wrap items-center justify-between gap-3 border-t border-border bg-bg-primary py-4">
        <p className="text-caption text-text-secondary">
          {suite.variants
            .map((variant) => CLIENTS.find((client) => client.value === variant.client)?.label)
            .join(" + ")}
          {" · "}
          {count} {count === 1 ? "run" : "runs"}
          {" · "}
          {suite.cases.every((item) =>
            item.checks.every((check) => check.assertion.kind === "rubric"),
          ) && !suite.judge
            ? "You review the output"
            : "Checks included"}
        </p>
        <Button
          type="submit"
          disabled={
            busy ||
            unavailable ||
            jsonErrors.length > 0 ||
            suite.cases.some((item) => !item.prompt.trim())
          }
        >
          {busy ? "Starting…" : count === 1 ? "Run task" : `Run ${count} tests`}
        </Button>
      </div>
    </form>
  );
}

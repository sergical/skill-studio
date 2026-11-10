import { useId } from "react";
import { Button, Input, Textarea } from "@skill-studio/ui";
import type { Check } from "@skill-studio/lib";
import { SelectControl } from "../ui/SelectControl";
import { CheckboxControl } from "../ui/CheckboxControl";
import { CHECK_TYPES, defaultAssertion } from "./lab-model";
import { parseCheckJson } from "./lab-json-drafts";

export function CheckEditor({
  check,
  onChange,
  onRemove,
  jsonText,
  jsonError,
  onJsonChange,
}: {
  check: Check;
  onChange: (check: Check) => void;
  onRemove: () => void;
  jsonText: string | undefined;
  jsonError: string | undefined;
  onJsonChange: (text: string | undefined) => void;
}) {
  const assertion = check.assertion;
  const changeJson = (text: string) => {
    onJsonChange(text);
    let next: Check;
    try {
      next = parseCheckJson(check, text);
    } catch {
      return;
    }
    onChange(next);
  };
  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <Input
          aria-label="Expected result name"
          value={check.name}
          required
          onChange={(event) => onChange({ ...check, name: event.target.value })}
          className="min-w-40 flex-1"
        />
        <SelectControl
          size="lg"
          ariaLabel={`Check type for ${check.name}`}
          value={assertion.kind}
          items={CHECK_TYPES}
          onValueChange={(kind) => {
            onJsonChange(undefined);
            onChange({ ...check, assertion: defaultAssertion(kind) });
          }}
        />
        <Button
          type="button"
          variant="ghost"
          onClick={onRemove}
          aria-label={`Remove ${check.name}`}
        >
          Remove
        </Button>
      </div>
      {"path" in assertion && (
        <Input
          aria-label="Expected file path"
          placeholder="output/report.md"
          value={assertion.path}
          required
          onChange={(event) =>
            onChange({ ...check, assertion: { ...assertion, path: event.target.value } })
          }
        />
      )}
      {"text" in assertion && (
        <Textarea
          aria-label="Expected text"
          placeholder="Text to check for"
          value={assertion.text}
          required
          onChange={(event) =>
            onChange({ ...check, assertion: { ...assertion, text: event.target.value } })
          }
        />
      )}
      {assertion.kind === "tool_called" && (
        <Input
          aria-label="Tool name"
          value={assertion.name}
          required
          onChange={(event) =>
            onChange({ ...check, assertion: { ...assertion, name: event.target.value } })
          }
        />
      )}
      {assertion.kind === "skill_loaded" && (
        <label className="flex items-center gap-2 text-body">
          <CheckboxControl
            checked={assertion.expected}
            onCheckedChange={(expected) =>
              onChange({ ...check, assertion: { ...assertion, expected } })
            }
          />
          The client should load this skill. Missing evidence needs review.
        </label>
      )}
      {assertion.kind === "rubric" && (
        <Textarea
          aria-label="Quality criterion"
          placeholder="State the required quality and the evidence that would prove it."
          value={assertion.criteria}
          required
          onChange={(event) =>
            onChange({ ...check, assertion: { ...assertion, criteria: event.target.value } })
          }
        />
      )}
      {assertion.kind === "output_json" && (
        <div className="space-y-2">
          <Input
            aria-label="JSON pointer"
            placeholder="/status"
            value={assertion.pointer}
            onChange={(event) =>
              onChange({ ...check, assertion: { ...assertion, pointer: event.target.value } })
            }
          />
          <JsonInput
            key={`${check.id}-equals`}
            label="Expected JSON value"
            value={jsonText ?? JSON.stringify(assertion.equals, null, 2)}
            error={jsonError}
            onChange={changeJson}
          />
        </div>
      )}
      {assertion.kind === "command" && (
        <div className="space-y-2">
          <Input
            aria-label="Command program"
            value={assertion.program}
            required
            onChange={(event) =>
              onChange({ ...check, assertion: { ...assertion, program: event.target.value } })
            }
          />
          <JsonInput
            key={`${check.id}-args`}
            label="Command arguments as JSON"
            value={jsonText ?? JSON.stringify(assertion.args, null, 2)}
            error={jsonError}
            onChange={changeJson}
          />
          <label className="flex items-center gap-2 text-body">
            Expected exit code
            <Input
              aria-label="Expected exit code"
              type="number"
              value={assertion.expected_exit}
              onChange={(event) =>
                onChange({
                  ...check,
                  assertion: { ...assertion, expected_exit: Number(event.target.value) },
                })
              }
            />
          </label>
          <p className="text-caption text-text-secondary">
            This command runs on your computer after the client finishes. Keep verifier code in
            these arguments or outside the files the client can edit.
          </p>
        </div>
      )}
    </div>
  );
}

function JsonInput({
  label,
  value,
  onChange,
  error,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  error: string | undefined;
}) {
  const errorId = useId();
  return (
    <div className="space-y-1">
      <Textarea
        aria-label={label}
        value={value}
        aria-invalid={Boolean(error)}
        aria-describedby={error ? errorId : undefined}
        required
        className="font-mono text-caption"
        onChange={(event) => onChange(event.target.value)}
      />
      {error && (
        <p id={errorId} className="text-body text-text-secondary">
          {error}
        </p>
      )}
    </div>
  );
}

import { Button, Input } from "@skill-studio/ui";
import type { ClientInfo, TestSuite } from "@skill-studio/lib";
import { SelectControl } from "../ui/SelectControl";
import { CLIENTS, id, parseClient } from "./lab-model";

export function VariantEditor({
  suite,
  clients,
  onChange,
}: {
  suite: TestSuite;
  clients: ClientInfo[];
  onChange: (suite: TestSuite) => void;
}) {
  return (
    <section aria-label="Clients and versions" className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-emphasis font-semibold">Compare clients and skill versions</h3>
        <Button
          type="button"
          variant="ghost"
          onClick={() =>
            onChange({
              ...suite,
              variants: [
                ...suite.variants,
                {
                  id: id("variant"),
                  name: "Comparison",
                  client: "codex",
                  model: null,
                  skill: { kind: "candidate" },
                },
              ],
            })
          }
        >
          Add variant
        </Button>
      </div>
      {suite.variants.map((variant) => {
        const client = clients.find((item) => item.id === variant.client);
        const update = (next: typeof variant) =>
          onChange({
            ...suite,
            variants: suite.variants.map((item) => (item.id === next.id ? next : item)),
          });
        return (
          <div key={variant.id} className="space-y-2 rounded-md border border-border p-3">
            <div className="flex flex-wrap gap-2">
              <Input
                aria-label="Variant name"
                value={variant.name}
                required
                onChange={(event) => update({ ...variant, name: event.target.value })}
                className="min-w-36 flex-1"
              />
              <SelectControl
                size="lg"
                ariaLabel={`Client for ${variant.name}`}
                items={CLIENTS}
                value={variant.client}
                onValueChange={(value) =>
                  update({
                    ...variant,
                    client: parseClient(value),
                    skill:
                      variant.skill.kind === "without_skill" && value !== "pi"
                        ? { kind: "candidate" }
                        : variant.skill,
                  })
                }
              />
              <SelectControl
                size="lg"
                ariaLabel={`Skill version for ${variant.name}`}
                value={variant.skill.kind}
                items={[
                  { value: "candidate", label: "Current skill" },
                  { value: "baseline", label: "Previous version" },
                  ...(variant.client === "pi"
                    ? [{ value: "without_skill", label: "No skill" }]
                    : []),
                ]}
                onValueChange={(kind) =>
                  update({
                    ...variant,
                    skill:
                      kind === "baseline"
                        ? { kind, path: "" }
                        : kind === "without_skill"
                          ? { kind }
                          : { kind: "candidate" },
                  })
                }
              />
              <Input
                aria-label={`Model for ${variant.name}`}
                placeholder="Client default model"
                value={variant.model ?? ""}
                onChange={(event) => update({ ...variant, model: event.target.value || null })}
                className="min-w-44 flex-1"
              />
              <Button
                type="button"
                variant="ghost"
                disabled={suite.variants.length === 1}
                onClick={() =>
                  onChange({
                    ...suite,
                    variants: suite.variants.filter((item) => item.id !== variant.id),
                  })
                }
                aria-label={`Remove ${variant.name}`}
              >
                Remove
              </Button>
            </div>
            {variant.skill.kind === "baseline" && (
              <Input
                aria-label={`Previous skill folder for ${variant.name}`}
                placeholder="Absolute path to the previous skill folder"
                value={variant.skill.path}
                required
                onChange={(event) =>
                  update({ ...variant, skill: { kind: "baseline", path: event.target.value } })
                }
              />
            )}
            <p className="text-caption text-text-secondary">
              {client?.available
                ? `${client.version}. ${client.note}`
                : (client?.note ?? "Checking client…")}
            </p>
          </div>
        );
      })}
      <div className="flex flex-wrap gap-4">
        <label className="flex flex-col gap-1 text-caption text-text-secondary">
          Runs per case and variant
          <Input
            aria-label="Repetitions"
            type="number"
            min={1}
            max={20}
            required
            value={suite.repetitions}
            onChange={(event) => onChange({ ...suite, repetitions: Number(event.target.value) })}
            className="w-24"
          />
        </label>
        <label className="flex flex-col gap-1 text-caption text-text-secondary">
          Time limit per client, seconds
          <Input
            aria-label="Time limit"
            type="number"
            min={15}
            max={3600}
            required
            value={suite.timeout_seconds}
            onChange={(event) =>
              onChange({ ...suite, timeout_seconds: Number(event.target.value) })
            }
            className="w-28"
          />
        </label>
      </div>
    </section>
  );
}

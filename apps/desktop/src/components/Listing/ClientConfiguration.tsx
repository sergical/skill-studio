import { useState } from "react";
import { ExternalLink } from "lucide-react";
import { Button, Input } from "@skill-studio/ui";
import { deploymentLabelFromAgentId, homeRelativePath } from "@skill-studio/lib";
import type { HarnessListing, ListingEntry } from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";
import { PageSection } from "../Shell/PageSection";
import { SummaryCard, SummaryMetric } from "../ui/SummaryCard";
import {
  budgetRule,
  entrySource,
  hasAssumedClaudeWindow,
  listedEntries,
  sourceCounts,
} from "./client-allocation";

type Setting = NonNullable<HarnessListing["setting"]>;

function SettingRow({
  setting,
  value,
  project,
  label,
  inputValue,
  onSaved,
}: {
  setting: Setting;
  value: string;
  project: string | null;
  label: string;
  inputValue: number | null;
  onSaved: () => void;
}) {
  const addToast = useAppStore((s) => s.addToast);
  const openListingSetting = useAppStore((s) => s.openListingSetting);
  const saveListingSetting = useAppStore((s) => s.saveListingSetting);
  const [opening, setOpening] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState("");
  const percent = setting.key === "skillListingBudgetFraction";
  const codex = setting.key === "skills.max_context_tokens";
  const max = percent ? 100 : codex ? 10000 : Number.MAX_SAFE_INTEGER;
  const n = Number(draft);
  const valid = draft != null && n > 0 && n <= max && (percent || Number.isSafeInteger(n));
  const save = async (reset: boolean) => {
    if (!setting.edit_revision || (!reset && !valid)) return;
    setOpening(true);
    setError("");
    try {
      await saveListingSetting(
        setting.key,
        project,
        reset ? null : String(percent ? Number((n / 100).toPrecision(15)) : n),
        setting.edit_revision,
      );
      setDraft(null);
      onSaved();
      addToast({
        type: "success",
        title: reset ? "Override removed" : "Setting saved",
      });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
    setOpening(false);
  };
  const open = async () => {
    setOpening(true);
    try {
      await openListingSetting(setting.key, project);
    } catch (error) {
      addToast({
        type: "error",
        title: "Couldn’t open configuration",
        message: error instanceof Error ? error.message : String(error),
      });
    }
    setOpening(false);
  };
  return (
    <div className="grid grid-cols-[minmax(9rem,1fr)_minmax(9rem,1.1fr)_auto] items-center gap-3 border-t border-border-subtle py-2.5">
      <span>
        <strong className="block text-small font-medium">{label}</strong>
        <code className="text-caption text-text-tertiary">{setting.key}</code>
      </span>
      <span>
        <strong className="block text-small font-medium">{value}</strong>
        <span title={setting.file} className="block break-all text-caption text-text-secondary">
          {setting.current == null ? "Default" : homeRelativePath(setting.file)}
        </span>
      </span>
      {setting.edit_revision ? (
        <Button
          variant="outline"
          size="sm"
          aria-label={`Edit ${label}`}
          className="justify-self-end"
          disabled={draft !== null}
          onClick={() => {
            setDraft(inputValue == null ? "" : String(inputValue));
            setError("");
          }}
        >
          Edit
        </Button>
      ) : (
        <Button
          size="sm"
          variant="ghost"
          // -mr-2.5 cancels the ghost padding, so the icon ends on the section's right edge.
          className="-mr-2.5"
          onClick={() => void open()}
          disabled={opening}
          aria-label={`Open configuration source for ${setting.key}`}
          title={homeRelativePath(setting.chezmoi_source ?? setting.file)}
        >
          {opening ? "Opening…" : "Open source"} <ExternalLink size={12} />
        </Button>
      )}
      {draft !== null && (
        <form
          className="col-span-3 flex flex-wrap items-center gap-2 rounded-md bg-bg-secondary p-3"
          onSubmit={(event) => {
            event.preventDefault();
            void save(false);
          }}
        >
          <label className="text-small" htmlFor={setting.key}>
            {percent ? "Context window (%)" : codex ? "Tokens" : "Characters"}
          </label>
          <Input
            id={setting.key}
            inputMode={percent ? "decimal" : "numeric"}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            className="w-28"
          />
          <Button type="submit" size="sm" disabled={!valid || opening}>
            Save
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={opening}
            onClick={() => setDraft(null)}
          >
            Cancel
          </Button>
          {setting.current != null && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={opening}
              onClick={() => void save(true)}
            >
              Remove override
            </Button>
          )}
          <p className="w-full break-all text-caption text-text-secondary">
            Saves to {homeRelativePath(setting.file)}. New sessions use the updated setting.
          </p>
          {error && (
            <p role="alert" className="w-full text-small text-destructive-text">
              {error}
            </p>
          )}
        </form>
      )}
    </div>
  );
}

function sourceExample(entry: ListingEntry) {
  return entry.plugin ?? (entry.path && homeRelativePath(entry.path)) ?? "Seen only in a session";
}

export function ClientConfiguration({
  row,
  onSaved,
}: {
  row: HarnessListing;
  onSaved: () => void;
}) {
  const name = deploymentLabelFromAgentId(row.harness);
  const entries = listedEntries(row);
  const sources = sourceCounts(entries);
  const project = row.scope.scope === "global" ? null : row.scope.project;
  const preview = row.evidence === "preview";
  const basePrompt = row.base_prompt_skills != null;
  const shortened = entries.filter((e) => e.state === "truncated").length;
  const namesOnly = entries.filter((e) => e.state === "name_only").length;
  const omitted = row.entries.filter((e) => e.state === "omitted").length;
  const described = entries.length - namesOnly;
  const session = row.observed_session;
  const assumedWindow = preview && hasAssumedClaudeWindow(row);
  return (
    <>
      <SummaryCard title={basePrompt ? `${row.budget_source} preview` : `What ${name} sees`}>
        {assumedWindow ? (
          <SummaryMetric value={budgetRule(row)} unit="budget, model window unknown" />
        ) : !preview || row.budget === null ? (
          <SummaryMetric
            value={entries.length.toLocaleString()}
            unit={`${entries.length === 1 ? "skill" : "skills"} ${
              basePrompt
                ? "in the skills section"
                : preview
                  ? "in the estimate"
                  : `in folders ${name} reads`
            }`}
          />
        ) : (
          <SummaryMetric
            value={described.toLocaleString()}
            unit={`of ${entries.length.toLocaleString()} skills keep a description`}
          />
        )}
        {preview && row.budget !== null && !assumedWindow && (
          <p className="mt-2 text-small text-text-secondary">
            Budget {row.budget.toLocaleString()} {row.budget_unit}
            {budgetRule(row).startsWith(row.budget.toLocaleString()) ? "" : ` · ${budgetRule(row)}`}
            {row.context_window_tokens
              ? ` · ${row.context_window_tokens.toLocaleString()}-token context window`
              : ""}
          </p>
        )}
        <p className="mt-3 border-t border-border-subtle pt-2.5 text-small">
          {assumedWindow ? (
            <>
              {name}’s settings give a percentage. The model catalog could not confirm how many
              characters that allows.
            </>
          ) : basePrompt ? (
            row.notes.join(" ")
          ) : !preview ? (
            `${name} lists skills from these folders. Skill Studio can’t estimate how it shortens them yet.`
          ) : (
            <>
              Estimate · <strong>{entries.length} skills</strong>
              {shortened ? ` · ${shortened} shortened` : ""}
              {namesOnly ? ` · ${namesOnly} name only` : ""}
              {!shortened && !namesOnly && entries.length > 0 ? " · Every description fits" : ""}
              {omitted ? ` · ${omitted} not listed` : ""}
            </>
          )}
        </p>
        {session && (
          <p className="mt-1 text-small text-text-secondary">
            Last session listed {session.listed} skills
            {session.model ? ` · ${session.model}` : ""}
            {session.at && Number.isFinite(Date.parse(session.at))
              ? ` · ${new Date(session.at).toLocaleString()}`
              : ""}
          </p>
        )}
      </SummaryCard>
      {row.base_prompt_skills && (
        <details className="rounded-md border border-border-subtle px-3 py-2">
          <summary className="cursor-pointer text-small">Preview the skills section</summary>
          <pre className="mt-2 whitespace-pre-wrap break-all text-caption">
            {row.base_prompt_skills}
          </pre>
        </details>
      )}
      {row.setting && (
        <PageSection title={`${name} settings`}>
          {row.configured_model && (
            <div className="grid grid-cols-[minmax(9rem,1fr)_minmax(9rem,1.1fr)_auto] gap-3 border-t border-border-subtle py-2.5 text-small">
              <span>Configured model</span>
              <span className="col-span-2">
                <strong className="font-medium">{row.configured_model.name}</strong>
                <span className="ml-2 text-caption text-text-secondary">
                  {homeRelativePath(row.configured_model.file)}
                </span>
              </span>
            </div>
          )}
          <SettingRow
            key={(row.setting.edit_revision ?? row.setting.file) + row.harness}
            onSaved={onSaved}
            inputValue={
              row.harness === "claude-code"
                ? Number((Number(row.setting.current ?? 0.01) * 100).toPrecision(12))
                : row.setting.current != null
                  ? Number(row.setting.current)
                  : row.budget_unit === "tokens"
                    ? Math.min(row.budget ?? 0, 10000)
                    : null
            }
            setting={row.setting}
            label="Skill list budget"
            value={budgetRule(row)}
            project={project}
          />
          {row.description_limit_setting && row.description_char_limit != null && (
            <SettingRow
              key={
                row.description_limit_setting.edit_revision ?? row.description_limit_setting.file
              }
              onSaved={onSaved}
              inputValue={row.description_char_limit}
              setting={row.description_limit_setting}
              label="Description limit"
              value={`${row.description_char_limit.toLocaleString()} characters`}
              project={project}
            />
          )}
          {(row.setting.chezmoi_source || row.description_limit_setting?.chezmoi_source) && (
            <p className="text-caption text-text-secondary">
              Managed by chezmoi. Edit the source, then run <code>chezmoi apply</code>.
            </p>
          )}
        </PageSection>
      )}
      {preview && (
        <PageSection title={`Where the ${entries.length} skills come from`}>
          <div className="divide-y divide-border-subtle rounded-md border border-border-subtle px-3">
            {(["Skill folder", "Plugin", "Session record"] as const).map((source) => {
              const count = sources[source];
              const title = {
                "Skill folder": "Skill folders",
                Plugin: "Plugins",
                "Session record": "Seen only in sessions",
              }[source];
              const heading = (
                <span className="inline-flex w-[calc(100%-1.5rem)] justify-between text-small">
                  <span>{title}</span>
                  <strong className="tabular-nums">{count}</strong>
                </span>
              );
              if (!count)
                return (
                  <div key={source} className="py-1.5 pl-4 text-text-tertiary">
                    {heading}
                  </div>
                );
              return (
                <details key={source} className="py-1.5">
                  <summary className="cursor-pointer">{heading}</summary>
                  <div className="my-2 ml-4 space-y-1.5">
                    {source === "Session record" && (
                      <p className="text-caption text-text-secondary">
                        A recorded session listed these skills. The folder scan could not match them
                        to an installed copy.
                      </p>
                    )}
                    {entries
                      .filter((e) => entrySource(e) === source)
                      .slice(0, 3)
                      .map((e) => (
                        <p className="text-small" key={e.name + (e.path ?? "")}>
                          {e.name}{" "}
                          <span className="break-all text-caption text-text-secondary">
                            {sourceExample(e)}
                          </span>
                        </p>
                      ))}
                    {count > 3 && (
                      <p className="text-caption text-text-secondary">
                        3 of {count} shown. See every skill below.
                      </p>
                    )}
                  </div>
                </details>
              );
            })}
          </div>
        </PageSection>
      )}
    </>
  );
}

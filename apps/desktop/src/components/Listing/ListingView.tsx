import { useEffect, useReducer, useState } from "react";
import { Copy } from "lucide-react";
import { Button, Input } from "@skill-studio/ui";
import { HARNESS_CATALOG, deploymentLabelFromAgentId, homeRelativePath } from "@skill-studio/lib";
import type {
  HarnessListing,
  ListingEntry,
  ListingReport,
  ListingRequest,
  ListingState,
  SkillSnapshot,
} from "@skill-studio/lib";
import { useClientRows } from "../../hooks/useClientRows";
import { CompareClients } from "./CompareClients";
import { PageShell } from "../Shell/PageShell";
import { SelectControl } from "../ui/SelectControl";
import { ClientConfiguration } from "./ClientConfiguration";
import { entrySource, hasAssumedClaudeWindow } from "./client-allocation";
import { currentListingRequest } from "./useSkillVisibility";
import { listingReportKey, useAppStore } from "../../store/appStore";

const STATE_LABEL = {
  described: "Described",
  truncated: "Shortened",
  name_only: "Name only",
  omitted: "Omitted",
  hidden: "Hidden",
} satisfies Record<ListingState, string>;

const STATE_CLASS = {
  described: "text-text-primary",
  truncated: "text-text-secondary",
  name_only: "text-text-tertiary",
  omitted: "text-destructive-text",
  hidden: "text-text-quaternary",
} satisfies Record<ListingState, string>;

const GRID = "[grid-template-columns:minmax(0,1fr)_96px_96px_96px_76px] gap-x-3 px-3";
const GLOBAL = "global";
const STATE_ORDER = { omitted: 0, name_only: 1, truncated: 2, described: 3, hidden: 4 };

const hasDescription = (state: ListingState | undefined) =>
  state === "described" || state === "truncated";

/** One entry per deployment: Codex keeps same-name skills from different paths. */
const entryKey = (entry: ListingEntry) => `${entry.name}\u0000${entry.path ?? ""}`;

/** Any difference the model would see: state or description length. */
const differs = (now: ListingEntry, tried: ListingEntry | undefined) =>
  tried !== undefined &&
  (tried.state !== now.state || tried.shown_description_chars !== now.shown_description_chars);

function scopeKey(row: HarnessListing): string {
  return row.scope.scope === "global" ? GLOBAL : row.scope.project;
}

function listingRequest(scope: string): ListingRequest {
  return currentListingRequest(scope === GLOBAL ? null : scope);
}

/** Simulators for the settings supported by the native listing request. */
const BUDGET_INPUTS = new Map<
  string,
  {
    label: string;
    placeholder: string;
    request: (value: number) => Partial<ListingRequest> | null;
  }
>([
  [
    "skillListingBudgetFraction",
    {
      label: "Try a budget, in percent of the context window",
      placeholder: "2 (percent)",
      request: (value) => (value <= 100 ? { claude_budget_fraction: value / 100 } : null),
    },
  ],
  [
    "skills.max_context_tokens",
    {
      label: "Try skills.max_context_tokens",
      placeholder: "8000 (tokens)",
      request: (value) =>
        Math.round(value) > 0 ? { codex_max_context_tokens: Math.round(value) } : null,
    },
  ],
]);

function StateText({ state }: { state: ListingState | null | undefined }) {
  if (!state) return <span className="text-caption text-text-quaternary">-</span>;
  return <span className={`text-caption ${STATE_CLASS[state]}`}>{STATE_LABEL[state]}</span>;
}

const TIER_LABEL = {
  all_described: "Every description fits",
  descriptions_shortened: "Descriptions are shortened",
  names_only: "Names only, some skills left out",
} satisfies Record<NonNullable<HarnessListing["thresholds"]>["tier"], string>;

function ClientReference({ harness }: { harness: string }) {
  const client = HARNESS_CATALOG.find((item) => item.id === harness);
  if (!client?.verifiedVisibility.length) return null;
  return (
    <details className="border-t border-border-subtle py-2">
      <summary className="cursor-pointer text-small">How {client.label} lists skills</summary>
      {client.verifiedVisibility.map((reference) => (
        <div key={reference.revision} className="mt-3 space-y-1 text-small">
          <p className="text-caption text-text-tertiary">
            Verified in {client.label}{" "}
            <span title={reference.revision}>
              {/^[a-f0-9]{40}$/.test(reference.revision)
                ? `source revision ${reference.revision.slice(0, 8)}`
                : reference.revision}
            </span>
          </p>
          <p>{reference.exposure}</p>
          {reference.savedListing && (
            <p className="text-text-secondary">Session format · {reference.savedListing}</p>
          )}
          <div className="flex flex-wrap gap-x-3 text-caption">
            {reference.sources.map((source) => {
              if (!source.startsWith("https://")) return null;
              const url = new URL(source);
              return (
                <a
                  href={source}
                  key={source}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-accent hover:underline"
                >
                  {url.hostname === "github.com"
                    ? `Source · ${url.pathname.split("/").pop()}`
                    : `${client.label} documentation`}
                </a>
              );
            })}
          </div>
        </div>
      ))}
    </details>
  );
}

function Summary({ row, budget }: { row: HarnessListing; budget: number }) {
  const count = (state: ListingState) => row.entries.filter((e) => e.state === state).length;
  const listed = row.entries.length - count("hidden");
  return (
    <div className="flex flex-col gap-1 text-small text-text-secondary">
      <p className="m-0 text-body text-text-primary">
        {count("described") + count("truncated")} of {listed} listed skills keep a description.{" "}
        {count("truncated") > 0 && `${count("truncated")} are shortened. `}
        {count("name_only") > 0 && `${count("name_only")} show a name only. `}
        {count("omitted") > 0 && `${count("omitted")} are left out. `}
        {count("hidden") > 0 && `${count("hidden")} are hidden.`}
      </p>
      <p className="m-0">
        Budget {budget.toLocaleString()} {row.budget_unit}.{" "}
        {!row.thresholds && `The full listing is ${row.full_size.toLocaleString()}. `}
        {row.budget_source}.
      </p>
      {row.description_char_limit != null && (
        <p className="m-0">
          Per-skill limit {row.description_char_limit.toLocaleString()} characters (
          <code>skillListingMaxDescChars</code>).
        </p>
      )}
      {row.context_window_tokens !== null && (
        <p className="m-0">
          {hasAssumedClaudeWindow(row)
            ? `Estimate assumes ${row.context_window_tokens.toLocaleString()} tokens because the model window is unknown.`
            : `Context window ${row.context_window_tokens.toLocaleString()} tokens, from ${row.context_source}.`}
        </p>
      )}
      {row.thresholds && (
        <section
          aria-label="Listing tier"
          className="mt-1 flex max-w-[68ch] flex-col gap-1 rounded-md border border-border-subtle p-3"
        >
          <p className="m-0 text-body text-text-primary">{TIER_LABEL[row.thresholds.tier]}</p>
          <p className="m-0 tabular-nums">
            Full listing {row.thresholds.full_cost.toLocaleString()} {row.budget_unit}. Names and
            paths only {row.thresholds.names_only_cost.toLocaleString()}, of which the skill roots
            table is {row.thresholds.roots_table_cost.toLocaleString()}.
          </p>
          <p className="m-0">{row.thresholds.summary}</p>
        </section>
      )}
      {row.notes.map((note) => (
        <p key={note} className="m-0 text-caption text-text-tertiary">
          {note}
        </p>
      ))}
    </div>
  );
}

function SettingCard({ row, tried }: { row: HarnessListing; tried: HarnessListing | null }) {
  const setting = (tried ?? row).setting;
  const [copied, setCopied] = useState(false);
  if (!setting) return null;
  const file = setting.chezmoi_source ?? setting.file;
  const copy = () => {
    void navigator.clipboard.writeText(setting.snippet).then(() => setCopied(true));
  };
  return (
    <section
      aria-label="Listing setting"
      className="flex flex-col gap-2 rounded-md border border-border-subtle p-3 text-small"
    >
      <p className="m-0 text-text-secondary">
        Example override for <code>{setting.key}</code>. Its configuration source is{" "}
        <code className="break-all">{file}</code>.
      </p>
      {setting.chezmoi_source && (
        <p className="m-0 text-caption text-text-tertiary">
          chezmoi manages <code className="break-all">{setting.file}</code>. An edit there is undone
          by the next chezmoi apply.
        </p>
      )}
      <div className="flex items-start gap-2">
        <pre className="m-0 min-w-0 flex-1 overflow-x-auto rounded-sm bg-bg-secondary px-2 py-1 text-caption">
          {setting.snippet}
        </pre>
        <Button
          variant="outline"
          size="sm"
          className="rounded-sm px-2 text-text-secondary"
          onClick={copy}
        >
          <Copy size={12} />
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
    </section>
  );
}

function EntryRow({
  entry,
  tried,
  scanOnly,
  hasRecording,
  onOpen,
}: {
  entry: ListingEntry;
  tried: ListingEntry | undefined;
  scanOnly: boolean;
  hasRecording: boolean;
  onOpen?: () => void;
}) {
  const changed = differs(entry, tried);
  return (
    <li
      aria-label={entry.name}
      data-changed={changed || undefined}
      className={`grid min-h-8 items-center ${scanOnly ? "grid-cols-[minmax(8rem,1fr)_minmax(0,2fr)] gap-3 px-3" : GRID} ${changed ? "bg-bg-hover" : ""}`}
    >
      <span
        className="min-w-0 truncate text-body text-text-primary"
        title={entry.path ?? undefined}
      >
        {onOpen ? (
          <Button
            size="sm"
            variant="ghost"
            className="max-w-full justify-start px-0"
            onClick={onOpen}
          >
            {entry.name}
          </Button>
        ) : (
          entry.name
        )}
        <span className="text-caption text-text-tertiary">
          {" "}
          · {entry.plugin ?? entrySource(entry)}
        </span>
        {entry.hidden_reason && (
          <span className="text-caption text-text-tertiary"> ({entry.hidden_reason})</span>
        )}
        {entry.invocation_mismatch && (
          <span className="text-caption text-destructive-text" title={entry.invocation_mismatch}>
            {" "}
            (manual-only in Claude Code)
          </span>
        )}
      </span>
      {scanOnly ? (
        <span className="truncate text-caption text-text-secondary" title={entry.path ?? undefined}>
          {entry.path && homeRelativePath(entry.path)}
          {hasRecording && entry.observed && (
            <span className="block">Last recorded listing · {STATE_LABEL[entry.observed]}</span>
          )}
        </span>
      ) : (
        <>
          <StateText state={entry.state} />
          {tried ? <StateText state={tried.state} /> : <span />}
          <StateText state={hasRecording ? entry.observed : null} />
          <span className="text-right text-caption tabular-nums text-text-tertiary">
            {entry.shown_description_chars.toLocaleString()}
            {tried && tried.shown_description_chars !== entry.shown_description_chars && (
              <> → {tried.shown_description_chars.toLocaleString()}</>
            )}
          </span>
        </>
      )}
    </li>
  );
}

interface ListingViewProps {
  snapshot: SkillSnapshot | undefined;
  initialHarness?: string;
  initialScope?: string;
}

interface BudgetComparisonProps {
  row: HarnessListing;
  tried: HarnessListing | null;
  changes: ListingEntry[];
  triedByKey: Map<string, ListingEntry>;
  onlyChanges: boolean;
  onToggle: () => void;
  onTry: (request: Partial<ListingRequest>) => void;
}

function BudgetComparison({
  row,
  tried,
  changes,
  triedByKey,
  onlyChanges,
  onToggle,
  onTry,
}: BudgetComparisonProps) {
  const [budgetText, setBudgetText] = useState("");
  const budgetInput = row.setting && BUDGET_INPUTS.get(row.setting.key);
  const value = Number(budgetText);
  const trial =
    budgetInput && Number.isFinite(value) && value > 0 ? budgetInput.request(value) : null;
  if (!budgetInput) return null;
  const gained = changes.filter(
    (e) => !hasDescription(e.state) && hasDescription(triedByKey.get(entryKey(e))?.state),
  ).length;
  const wholeDifference = tried
    ? tried.entries.filter((e) => e.state === "described").length -
      row.entries.filter((e) => e.state === "described").length
    : 0;
  const lost = changes.filter(
    (e) => hasDescription(e.state) && !hasDescription(triedByKey.get(entryKey(e))?.state),
  ).length;
  return (
    <details className="rounded-md border border-border-subtle px-3 py-2">
      <summary className="cursor-pointer text-small">Preview a different budget</summary>
      <p className="my-2 text-caption text-text-secondary">
        This preview estimates a different budget. Changes to your client are made under Effective
        settings.
      </p>
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (trial) onTry(trial);
        }}
      >
        <label htmlFor="listing-budget" className="text-small text-text-secondary">
          {budgetInput.label}
        </label>
        <Input
          id="listing-budget"
          placeholder={budgetInput.placeholder}
          inputMode="decimal"
          value={budgetText}
          onChange={(event) => setBudgetText(event.target.value)}
          className="w-40"
        />
        <Button
          type="submit"
          variant="outline"
          size="sm"
          className="rounded-sm px-2"
          disabled={!trial}
        >
          Compare
        </Button>
        {tried && (
          <>
            <span className="text-small text-text-secondary">
              At {tried.budget?.toLocaleString()} {tried.budget_unit},{" "}
              {wholeDifference !== 0 ? (
                <strong className="text-text-primary">
                  {Math.abs(wholeDifference)} {wholeDifference > 0 ? "more" : "fewer"} fit in full.
                </strong>
              ) : (
                <>
                  {changes.length} {changes.length === 1 ? "entry changes" : "entries change"}.
                </>
              )}
              {gained > 0 && <> {gained} gain a description.</>}
              {lost > 0 && <> {lost} lose a description.</>}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="rounded-sm px-2 text-text-secondary"
              aria-pressed={onlyChanges}
              onClick={onToggle}
            >
              {onlyChanges ? "Show all" : "Show only changes"}
            </Button>
          </>
        )}
      </form>
      {tried && (
        <div className="mt-3">
          <SettingCard row={row} tried={tried} />
        </div>
      )}
    </details>
  );
}

interface ListingFilters {
  query: string;
  limit: number;
  tryRequest: Partial<ListingRequest> | null;
  onlyChanges: boolean;
  showHidden: boolean;
}
const INITIAL_FILTERS: ListingFilters = {
  query: "",
  limit: 25,
  tryRequest: null,
  onlyChanges: false,
  showHidden: false,
};
function updateFilters(current: ListingFilters, patch: Partial<ListingFilters>): ListingFilters {
  return { ...current, ...patch };
}

export function ListingView({ snapshot, initialHarness, initialScope }: ListingViewProps) {
  const [selected, setSelected] = useState(initialHarness);
  const openSkill = useAppStore((s) => s.openSkill);
  const [scope, setScope] = useState(initialScope ?? GLOBAL);
  const [{ query, limit, tryRequest, onlyChanges, showHidden }, change] = useReducer(
    updateFilters,
    INITIAL_FILTERS,
  );
  const refreshListing = useAppStore((s) => s.refreshListing);
  const base = listingRequest(scope);
  const comparison = tryRequest && { ...base, ...tryRequest };
  const current = useAppStore((s) => s.listingReports[listingReportKey(base)]);
  const compared = useAppStore((s) =>
    comparison ? s.listingReports[listingReportKey(comparison)] : undefined,
  );

  useEffect(() => refreshListing(listingRequest(scope)), [scope, refreshListing]);

  const clients = useClientRows(current?.data?.listings.filter((r) => scopeKey(r) === scope) ?? []);
  const harness = clients.find((r) => r.harness === selected)?.harness ?? clients[0]?.harness ?? "";
  const byPath = new Map(
    snapshot?.skills.flatMap((skill) =>
      skill.deployments.map((d) => [d.path, skill.name] as const),
    ),
  );
  const pick = (report: ListingReport | undefined) =>
    report?.listings.find((r) => r.harness === harness && scopeKey(r) === scope) ?? null;
  const row = pick(current?.data);
  const tried = pick(compared?.data);
  const refreshing =
    (current?.refreshing ?? true) || (comparison !== null && (compared?.refreshing ?? true));
  const error = compared?.error ?? current?.error;
  const triedByKey = new Map(tried?.entries.map((e) => [entryKey(e), e]) ?? []);
  const changes = row?.entries.filter((e) => differs(e, triedByKey.get(entryKey(e)))) ?? [];
  const hiddenCount = row?.entries.filter((e) => e.state === "hidden").length ?? 0;
  const stateChanges = (entry: ListingEntry) => {
    const next = triedByKey.get(entryKey(entry));
    return Number(next !== undefined && next.state !== entry.state);
  };
  const shown = (onlyChanges && tried ? changes : (row?.entries ?? []))
    .filter(
      (e) =>
        (showHidden || e.state !== "hidden") &&
        (e.name + " " + entrySource(e) + " " + (e.plugin ?? "") + " " + (e.path ?? ""))
          .toLowerCase()
          .includes(query.toLowerCase()),
    )
    .sort(
      (a, b) => stateChanges(b) - stateChanges(a) || STATE_ORDER[a.state] - STATE_ORDER[b.state],
    );
  const harnesses = clients.map((r) => ({
    value: r.harness,
    label: deploymentLabelFromAgentId(r.harness),
  }));
  const scopes = [
    { value: GLOBAL, label: "Global" },
    ...(snapshot?.projects ?? []).map((p) => ({ value: p, label: p })),
  ];
  const preview = row?.evidence === "preview";

  return (
    <PageShell
      title="Client configuration"
      refreshing={refreshing}
      subtitle="Skills available to each client"
      actions={
        <>
          <Button
            size="sm"
            variant="ghost"
            disabled={refreshing}
            onClick={() => {
              refreshListing(base);
              if (comparison) refreshListing(comparison);
            }}
          >
            Reload from disk
          </Button>
          <SelectControl
            value={harness}
            onValueChange={(value) => {
              setSelected(value);
              change(INITIAL_FILTERS);
            }}
            items={harnesses}
            ariaLabel="Agent"
            disabled={!harnesses.length}
          />
          <SelectControl
            value={scope}
            onValueChange={(value) => {
              setScope(value);
              change(INITIAL_FILTERS);
            }}
            items={scopes}
            ariaLabel="Scope"
          />
        </>
      }
    >
      {error && (
        <p role="alert" className="m-0 text-small text-destructive-text">
          {error}
        </p>
      )}
      {!row ? (
        current?.data && (
          <p className="m-0 text-small text-text-secondary">
            No selected client has a report in this scope. Choose clients in Settings.
          </p>
        )
      ) : (
        <div className="flex flex-col gap-4">
          <ClientConfiguration
            row={row}
            onSaved={() => {
              change({ tryRequest: null, onlyChanges: false });
              refreshListing(base);
            }}
          />
          {preview && row.budget !== null && (
            <BudgetComparison
              key={scope + harness}
              row={row}
              tried={tried}
              changes={changes}
              triedByKey={triedByKey}
              onlyChanges={onlyChanges}
              onToggle={() => change({ onlyChanges: !onlyChanges })}
              onTry={(request) => {
                change({ tryRequest: request });
                refreshListing({ ...base, ...request });
              }}
            />
          )}
          {preview && row.budget !== null && (
            <details className="rounded-md border border-border-subtle px-3 py-2">
              <summary className="cursor-pointer text-small text-text-secondary">
                How the estimate was calculated
              </summary>
              <div className="mt-3 flex flex-col gap-3">
                <Summary row={row} budget={row.budget} />
              </div>
            </details>
          )}
          <details
            open={!preview || undefined}
            key={scope + harness + "entries"}
            className="rounded-md border border-border-subtle px-3 py-2"
          >
            <summary className="cursor-pointer text-small">
              {!preview ? "Skills found in folders" : "Browse individual entries"}
            </summary>
            <Input
              aria-label="Find an entry"
              placeholder={
                !preview ? "Search by name or path" : "Search by name, plugin, source, or path"
              }
              value={query}
              onChange={(event) => {
                change({ query: event.target.value, limit: 25 });
              }}
              className="my-2"
            />

            {preview && (
              <div
                className={`grid h-8 items-center border-b border-border-subtle text-caption text-text-tertiary ${GRID}`}
              >
                <span>Skill</span>
                <span>Now</span>
                <span>{tried ? "At new budget" : ""}</span>
                <span>{row.observed_session ? "Last session" : ""}</span>
                <span className="text-right">Chars shown</span>
              </div>
            )}
            <ul aria-label="Listing" className="m-0 list-none p-0">
              {shown.slice(0, limit).map((entry) => {
                const owner = entry.path && byPath.get(entry.path);
                return (
                  <EntryRow
                    key={entryKey(entry)}
                    entry={entry}
                    tried={triedByKey.get(entryKey(entry))}
                    scanOnly={!preview}
                    hasRecording={Boolean(row.observed_session)}
                    onOpen={
                      owner && entry.path
                        ? () =>
                            openSkill(owner, entry.path ?? undefined, undefined, {
                              kind: "listing",
                              harness,
                              scope,
                            })
                        : undefined
                    }
                  />
                );
              })}
            </ul>
            {!shown.length && (
              <p className="my-2 text-small text-text-secondary">No matching entries.</p>
            )}
            {shown.length > limit && (
              <Button size="sm" variant="ghost" onClick={() => change({ limit: limit + 25 })}>
                Show more ({limit} of {shown.length})
              </Button>
            )}
            {hiddenCount > 0 && !onlyChanges && (
              <Button
                variant="ghost"
                size="sm"
                className="mt-1 rounded-sm px-2 text-text-secondary"
                aria-pressed={showHidden}
                onClick={() => change({ showHidden: !showHidden })}
              >
                {showHidden ? "Hide hidden skills" : `Show ${hiddenCount} hidden skills`}
              </Button>
            )}
          </details>
          <ClientReference key={row.harness} harness={row.harness} />
          <CompareClients row={row} clients={clients} />
        </div>
      )}
    </PageShell>
  );
}

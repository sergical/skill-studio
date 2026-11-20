// ============================================================================
// ListingView - The Model listing place: for each agent and scope, which
// skills the model sees with a description, with a name only, or not at all,
// from the core's `listing`. A budget field predicts the listing at another
// budget and marks the skills that change. Skill Studio never writes an
// agent's settings file, so the setting card shows the text to paste and the
// file (or chezmoi source) to paste it into.
// ============================================================================

import { useEffect, useState } from "react";
import { Copy } from "lucide-react";
import { Button, Input } from "@skill-studio/ui";
import { deploymentLabelFromAgentId } from "@skill-studio/lib";
import type {
  HarnessListing,
  ListingEntry,
  ListingReport,
  ListingRequest,
  ListingState,
  SkillSnapshot,
} from "@skill-studio/lib";
import { PageShell } from "../Shell/PageShell";
import { SelectControl } from "../ui/SelectControl";
import { useAppStore } from "../../store/appStore";

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

const GRID = "[grid-template-columns:minmax(0,1fr)_96px_96px_96px_64px] gap-x-3 px-3";
const GLOBAL = "global";

type Load =
  | { kind: "loading" }
  | { kind: "ready"; now: ListingReport; tried: ListingReport | null }
  | { kind: "error"; message: string };

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

/** The budget field's value as the request override for this harness. */
function override(harness: string, text: string): Partial<ListingRequest> | null {
  const value = Number(text);
  if (text.trim() === "" || !Number.isFinite(value) || value <= 0) return null;
  if (harness === "claude-code") {
    return value <= 100 ? { claude_budget_fraction: value / 100 } : null;
  }
  if (harness === "codex") return { codex_max_context_tokens: Math.round(value) };
  return null;
}

function StateText({ state }: { state: ListingState | null | undefined }) {
  if (!state) return <span className="text-caption text-text-quaternary">-</span>;
  return <span className={`text-caption ${STATE_CLASS[state]}`}>{STATE_LABEL[state]}</span>;
}

function Summary({ row }: { row: HarnessListing }) {
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
        {row.budget === null
          ? "This agent has no known listing limit."
          : `Budget ${row.budget.toLocaleString()} ${row.budget_unit}; the full listing is ${row.full_size.toLocaleString()}. ${row.budget_source}.`}
      </p>
      {row.context_window_tokens !== null && (
        <p className="m-0">
          Context window {row.context_window_tokens.toLocaleString()} tokens, from{" "}
          {row.context_source}.
          {row.uses_default_context_window && " The model was not found, so this is a default."}
        </p>
      )}
      {row.observed_session && (
        <p className="m-0">
          Last session: {row.observed_session.described} of {row.observed_session.listed} with a
          description ({row.observed_session.at ?? "time unknown"}).
        </p>
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
        <code>{setting.key}</code> is {setting.current ?? "not set"}. Skill Studio does not write
        agent settings. To change it, put this in <code className="break-all">{file}</code>
        {setting.chezmoi_source ? ", then run chezmoi apply." : "."}
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

function EntryRow({ entry, tried }: { entry: ListingEntry; tried: ListingEntry | undefined }) {
  const changed = differs(entry, tried);
  return (
    <li
      aria-label={entry.name}
      data-changed={changed || undefined}
      className={`grid h-8 items-center ${GRID} ${changed ? "bg-bg-hover" : ""}`}
    >
      <span
        className="min-w-0 truncate text-body text-text-primary"
        title={entry.path ?? undefined}
      >
        {entry.name}
        {!entry.scanned && <span className="text-caption text-text-tertiary"> (built in)</span>}
        {entry.hidden_reason && (
          <span className="text-caption text-text-tertiary"> ({entry.hidden_reason})</span>
        )}
      </span>
      <StateText state={entry.state} />
      <StateText state={tried?.state} />
      <StateText state={entry.observed} />
      <span className="text-right text-caption tabular-nums text-text-tertiary">
        {entry.usage_score ? entry.usage_score.toFixed(1) : ""}
      </span>
    </li>
  );
}

interface ListingViewProps {
  snapshot: SkillSnapshot | undefined;
}

export function ListingView({ snapshot }: ListingViewProps) {
  const [harness, setHarness] = useState("claude-code");
  const [scope, setScope] = useState(GLOBAL);
  const [budgetText, setBudgetText] = useState("");
  const [tryRequest, setTryRequest] = useState<Partial<ListingRequest> | null>(null);
  const [onlyChanges, setOnlyChanges] = useState(false);
  const [showHidden, setShowHidden] = useState(false);
  const [load, setLoad] = useState<Load>({ kind: "loading" });
  const readListing = useAppStore((s) => s.readListing);

  useEffect(() => {
    let live = true;
    const base: ListingRequest = {
      project_path: scope === GLOBAL ? null : scope,
      claude_budget_fraction: null,
      claude_context_window_tokens: null,
      codex_max_context_tokens: null,
      claude_transcript: null,
    };
    Promise.all([readListing(base), tryRequest ? readListing({ ...base, ...tryRequest }) : null])
      .then(([now, tried]) => live && setLoad({ kind: "ready", now, tried }))
      .catch(
        (err) =>
          live &&
          setLoad({ kind: "error", message: err instanceof Error ? err.message : String(err) }),
      );
    return () => {
      live = false;
    };
  }, [readListing, scope, tryRequest]);

  const pick = (report: ListingReport | null) =>
    report?.listings.find((r) => r.harness === harness && scopeKey(r) === scope) ?? null;
  const row = load.kind === "ready" ? pick(load.now) : null;
  const tried = load.kind === "ready" ? pick(load.tried) : null;
  const triedByKey = new Map(tried?.entries.map((e) => [entryKey(e), e]) ?? []);
  const changes = row?.entries.filter((e) => differs(e, triedByKey.get(entryKey(e)))) ?? [];
  const gained = changes.filter(
    (e) => !hasDescription(e.state) && hasDescription(triedByKey.get(entryKey(e))?.state),
  ).length;
  const lost = changes.filter(
    (e) => hasDescription(e.state) && !hasDescription(triedByKey.get(entryKey(e))?.state),
  ).length;
  const hiddenCount = row?.entries.filter((e) => e.state === "hidden").length ?? 0;
  const shown = (onlyChanges && tried ? changes : (row?.entries ?? [])).filter(
    (e) => showHidden || e.state !== "hidden",
  );
  const harnesses =
    load.kind === "ready"
      ? [...new Set(load.now.listings.map((r) => r.harness))].map((id) => ({
          value: id,
          label: deploymentLabelFromAgentId(id),
        }))
      : [{ value: harness, label: deploymentLabelFromAgentId(harness) }];
  const scopes = [
    { value: GLOBAL, label: "Global sessions" },
    ...(snapshot?.projects ?? []).map((p) => ({ value: p, label: p })),
  ];
  const budgetLabel =
    harness === "claude-code"
      ? "Try a budget, in percent of the context window"
      : "Try skills.max_context_tokens";
  const canTry = override(harness, budgetText) !== null;

  return (
    <PageShell
      title="Model listing"
      subtitle="What each agent shows the model about your skills, and what changes at another budget."
      actions={
        <>
          <SelectControl
            value={harness}
            onValueChange={(value) => {
              setHarness(value);
              setTryRequest(null);
              setBudgetText("");
            }}
            items={harnesses}
            ariaLabel="Agent"
          />
          <SelectControl value={scope} onValueChange={setScope} items={scopes} ariaLabel="Scope" />
        </>
      }
    >
      {load.kind === "loading" ? (
        <p className="m-0 text-small text-text-secondary">Reading the listing…</p>
      ) : load.kind === "error" ? (
        <p role="alert" className="m-0 text-small text-destructive-text">
          {load.message}
        </p>
      ) : !row ? (
        <p className="m-0 text-small text-text-secondary">No listing for this agent and scope.</p>
      ) : (
        <div className="flex flex-col gap-4">
          <Summary row={row} />
          {row.budget !== null && (
            <form
              className="flex flex-wrap items-center gap-2"
              onSubmit={(event) => {
                event.preventDefault();
                setTryRequest(override(harness, budgetText));
              }}
            >
              <label htmlFor="listing-budget" className="text-small text-text-secondary">
                {budgetLabel}
              </label>
              <Input
                id="listing-budget"
                placeholder={harness === "claude-code" ? "2 (percent)" : "8000 (tokens)"}
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
                disabled={!canTry}
              >
                Compare
              </Button>
              {tried && (
                <>
                  <span className="text-small text-text-secondary">
                    At {tried.budget?.toLocaleString()} {tried.budget_unit}: {changes.length}{" "}
                    change: {gained} gain a description, {lost} lose one.
                  </span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="rounded-sm px-2 text-text-secondary"
                    aria-pressed={onlyChanges}
                    onClick={() => setOnlyChanges((v) => !v)}
                  >
                    {onlyChanges ? "Show all" : "Show only changes"}
                  </Button>
                </>
              )}
            </form>
          )}
          <SettingCard row={row} tried={tried} />
          <div>
            <div
              className={`grid h-8 items-center border-b border-border-subtle text-caption text-text-tertiary ${GRID}`}
            >
              <span>Skill</span>
              <span>Now</span>
              <span>{tried ? "At new budget" : ""}</span>
              <span>Last session</span>
              <span className="text-right">Uses</span>
            </div>
            <ul aria-label="Listing" className="m-0 list-none p-0">
              {shown.map((entry) => (
                <EntryRow
                  key={entryKey(entry)}
                  entry={entry}
                  tried={triedByKey.get(entryKey(entry))}
                />
              ))}
            </ul>
            {hiddenCount > 0 && !onlyChanges && (
              <Button
                variant="ghost"
                size="sm"
                className="mt-1 rounded-sm px-2 text-text-secondary"
                aria-pressed={showHidden}
                onClick={() => setShowHidden((v) => !v)}
              >
                {showHidden ? "Hide hidden skills" : `Show ${hiddenCount} hidden skills`}
              </Button>
            )}
          </div>
        </div>
      )}
    </PageShell>
  );
}

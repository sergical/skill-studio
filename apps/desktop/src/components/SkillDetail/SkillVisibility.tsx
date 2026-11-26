import { useState } from "react";
import {
  agentIdFromDeploymentLabel,
  deploymentLabelFromAgentId,
  homeRelativePath,
  LISTING_STATE_LABEL,
} from "@skill-studio/lib";
import type {
  Deployment,
  HarnessListing,
  InstalledSkill,
  ListingEntry,
  ListingState,
} from "@skill-studio/lib";
import { useClientRows } from "../../hooks/useClientRows";
import { SelectControl } from "../ui/SelectControl";
import { hasAssumedClaudeWindow } from "../Listing/client-allocation";
import { useSkillVisibility } from "../Listing/useSkillVisibility";

interface KnownListing {
  /** A recorded session is what the agent showed; an estimate is what it would show now. */
  source: "session" | "estimate";
  state: ListingState;
  shownChars: number | null;
  text: NonNullable<ListingEntry["description_preview"]> | null;
}

/** What is known of this skill in the agent's skill list: the last session's record when
 * there is one, else the estimate. The estimate's text is cut where the session cut it. */
function knownListing(row: HarnessListing, entry: ListingEntry): KnownListing {
  const preview = entry.description_preview ?? null;
  if (!row.observed_session || !entry.observed)
    return {
      source: "estimate",
      state: entry.state,
      shownChars: entry.shown_description_chars,
      text: preview,
    };
  const shownChars = entry.observed_description_chars ?? null;
  const full = preview ? Array.from(preview.shown + preview.remainder) : null;
  const cut = entry.observed === "truncated" ? shownChars : null;
  // The session recorded a state and a length, not the text. Today's text stands in only when
  // the session showed a description and its length still matches today's.
  const shownText =
    entry.observed === "truncated" ||
    (entry.observed === "described" && shownChars === entry.full_description_chars);
  return {
    source: "session",
    state: entry.observed,
    shownChars,
    text:
      full && shownText
        ? {
            shown: (cut == null ? full : full.slice(0, cut)).join(""),
            remainder: cut == null ? "" : full.slice(cut).join(""),
          }
        : null,
  };
}

interface SkillVisibilityProps {
  skill: InstalledSkill;
  deployment: Deployment;
  initialClient?: string;
  listingScope?: string;
}

export function SkillVisibility({
  skill,
  deployment,
  initialClient,
  listingScope,
}: SkillVisibilityProps) {
  const project =
    listingScope === "global" ? null : (listingScope ?? deployment.project_path ?? null);
  const { listings, history, error } = useSkillVisibility(project, deployment.content_hash);
  const clients = useClientRows(listings);
  const [selected, setSelected] = useState(initialClient);
  const source = deployment.resolved_path ?? deployment.symlink_target ?? deployment.path;
  const paths = new Set(
    skill.deployments.flatMap((d) =>
      (d.resolved_path ?? d.symlink_target ?? d.path) === source ? [d.path] : [],
    ),
  );
  paths.add(source);
  // Only agents that list this copy (or a link to it): an agent without one has nothing to show.
  const agents = clients.flatMap((row) => {
    const entry = row.entries.find((e) => e.path && paths.has(e.path));
    return entry ? [{ row, entry }] : [];
  });
  const active =
    agents.find((a) => a.row.harness === selected) ??
    agents.find((a) => a.row.harness === agentIdFromDeploymentLabel(deployment.agent)) ??
    agents[0];
  if (!active)
    return error ? (
      <p role="alert" className="text-small text-text-secondary">
        {error}
      </p>
    ) : null;
  const { row, entry } = active;
  const harness = row.harness;
  const name = deploymentLabelFromAgentId(harness);
  const known = knownListing(row, entry);
  const use = history?.skills.find((r) => r.harness === harness && r.name === entry.name);
  const sessionAgent = history?.agents.find((a) => a.harness === harness);
  const sessions =
    sessionAgent?.records === "listing_and_uses" ? sessionAgent.sessions_read : undefined;
  return (
    <section
      aria-label="What agents see of this skill"
      className="rounded-lg border border-border-subtle bg-bg-secondary p-4"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="text-heading font-semibold">What agents see</h3>
          <p className="mt-0.5 text-caption text-text-tertiary">
            {project ? "Project" : "Global"} ·{" "}
            {row.evidence !== "preview"
              ? "Folders only"
              : row.base_prompt_skills != null
                ? "Base prompt preview"
                : hasAssumedClaudeWindow(row)
                  ? "Estimate, model window assumed"
                  : "Estimate"}
          </p>
        </div>
        <SelectControl
          ariaLabel="Agent for this skill"
          value={harness}
          onValueChange={setSelected}
          items={agents.map((a) => ({
            value: a.row.harness,
            label: deploymentLabelFromAgentId(a.row.harness),
          }))}
        />
      </div>
      {row.evidence !== "preview" ? (
        <div className="mt-3 space-y-1 text-small">
          <p className="font-medium">
            {entry.hidden_reason
              ? `Hidden · ${entry.hidden_reason}`
              : `Found in a folder ${name} reads`}
          </p>
          {entry.path && (
            <p className="break-all text-caption text-text-secondary">
              {homeRelativePath(entry.path)}
            </p>
          )}
          {row.observed_session && entry.observed && (
            <p className="pt-2 text-text-secondary">
              Seen in the last session · <strong>{LISTING_STATE_LABEL[entry.observed]}</strong>
              {row.observed_session.model && ` · ${row.observed_session.model}`}
            </p>
          )}
        </div>
      ) : (
        <>
          {known.state === "truncated" && row.thresholds?.tier === "descriptions_shortened" ? (
            // Codex's budget is full: it cuts every long description to about one length.
            <p className="mt-3 text-small text-text-secondary">
              {name} keeps about{" "}
              <strong className="text-text-primary">
                {row.thresholds.average_description_chars.toLocaleString()} characters
              </strong>{" "}
              of each description
              {row.setting && (
                <code className="ml-2 text-caption text-text-tertiary">{row.setting.key}</code>
              )}
            </p>
          ) : (
            row.description_char_limit != null && (
              <p className="mt-3 text-small text-text-secondary">
                Per-skill limit{" "}
                <strong className="text-text-primary">
                  {row.description_char_limit.toLocaleString()} characters
                </strong>
                {row.description_limit_setting ? (
                  <code className="ml-2 text-caption text-text-tertiary">
                    {row.description_limit_setting.key}
                  </code>
                ) : (
                  <span className="ml-2 text-caption text-text-tertiary">fixed</span>
                )}
              </p>
            )
          )}
          <div className="mt-4 flex items-baseline justify-between gap-2 text-small">
            <strong
              className={known.state === "truncated" ? "text-warning" : "text-text-secondary"}
            >
              {LISTING_STATE_LABEL[known.state]}
            </strong>
            <span className="tabular-nums text-text-tertiary">
              {known.source === "session"
                ? `Last session${row.observed_session?.model ? ` · ${row.observed_session.model}` : ""}`
                : "Estimate"}
              {known.shownChars != null &&
                ` · ${known.shownChars} of ${entry.full_description_chars} characters`}
            </span>
          </div>
          {/* A session records a state and a length, not the words: the text below is today's. */}
          {known.source === "session" && known.text && (
            <p className="mt-2 text-caption text-text-tertiary">Today’s text</p>
          )}
          <div className="mt-2 rounded-md border border-border-subtle bg-bg-primary px-3 py-3 text-body leading-relaxed">
            {known.state === "hidden" || known.state === "omitted" ? (
              <p>{entry.hidden_reason ?? "The model does not see this skill."}</p>
            ) : (
              <p className="select-text">
                <strong className="font-mono text-small">{entry.name}</strong>
                {known.text?.shown && <> · {known.text.shown}</>}
                {!known.text && (known.state === "described" || known.state === "truncated") && (
                  <span className="text-text-secondary">
                    {" "}
                    ·{" "}
                    {known.source === "session" && entry.description_preview
                      ? "The description changed after this session."
                      : "Description text unavailable for this copy."}
                  </span>
                )}
              </p>
            )}
            {known.text?.remainder && known.state === "truncated" && (
              <div className="mt-2 border-t border-dashed border-border pt-2 text-text-tertiary">
                <span className="text-caption font-medium">Beyond the cutoff</span>
                <p className="mt-0.5 select-text text-small">{known.text.remainder}</p>
              </div>
            )}
          </div>
          {known.source === "session" && entry.state !== known.state && (
            <p className="mt-3 text-small text-text-secondary">
              Estimate with the current settings ·{" "}
              <strong>{LISTING_STATE_LABEL[entry.state]}</strong>
            </p>
          )}
        </>
      )}
      {use && sessions !== undefined && sessions > 0 && (
        <p className="mt-3 text-small text-text-secondary">
          {deploymentLabelFromAgentId(harness)} used this skill in{" "}
          <strong className="text-text-primary">
            {use.used} of {sessions}
          </strong>{" "}
          recent sessions.
        </p>
      )}
      {error && <p className="mt-2 text-caption text-text-tertiary">{error}</p>}
    </section>
  );
}

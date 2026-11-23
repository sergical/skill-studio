import { useState } from "react";
import {
  agentIdFromDeploymentLabel,
  deploymentLabelFromAgentId,
  homeRelativePath,
} from "@skill-studio/lib";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";
import { useClientRows } from "../../hooks/useClientRows";
import { SelectControl } from "../ui/SelectControl";
import { hasAssumedClaudeWindow } from "../Listing/client-allocation";
import { useSkillVisibility } from "../Listing/useSkillVisibility";

interface SkillVisibilityProps {
  skill: InstalledSkill;
  deployment: Deployment;
  initialClient?: string;
  listingScope?: string;
}

const STATE_LABEL = {
  described: "Full description",
  truncated: "Shortened",
  name_only: "Name only",
  hidden: "Hidden",
  omitted: "Omitted",
};

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
  const agents = clients.map((row) => ({
    row,
    entry: row.entries.find((e) => e.path && paths.has(e.path)),
  }));
  const active =
    agents.find((a) => a.row.harness === selected) ??
    agents.find((a) => a.row.harness === agentIdFromDeploymentLabel(deployment.agent)) ??
    agents.find((a) => a.entry) ??
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
  const excerpt = entry?.description_preview;
  const use = history?.skills.find((r) => r.harness === harness && r.name === entry?.name);
  const sessionAgent = history?.agents.find((a) => a.harness === harness);
  const sessions =
    sessionAgent?.records === "listing_and_uses" ? sessionAgent.sessions_read : undefined;
  return (
    <section
      aria-label="Skill listing preview"
      className="rounded-lg border border-border-subtle bg-bg-secondary p-4"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <h3 className="text-heading font-semibold">Skill visibility</h3>
          <p className="mt-0.5 text-caption text-text-tertiary">
            {project ? "Project" : "Global"} ·{" "}
            {row.evidence !== "preview"
              ? "Folder scan"
              : hasAssumedClaudeWindow(row)
                ? "Listing estimate with an assumed model window"
                : "Listing estimate"}
          </p>
        </div>
        <SelectControl
          ariaLabel="Client for this skill"
          value={harness}
          onValueChange={setSelected}
          items={clients.map((r) => ({
            value: r.harness,
            label: deploymentLabelFromAgentId(r.harness),
          }))}
        />
      </div>
      {!entry ? (
        <p className="mt-3 text-small">No matching copy was found for {name} in this scope.</p>
      ) : row.evidence !== "preview" ? (
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
        </div>
      ) : (
        <>
          {row.description_char_limit != null && (
            <p className="mt-3 text-small text-text-secondary">
              Per-skill limit{" "}
              <strong className="text-text-primary">
                {row.description_char_limit.toLocaleString()} characters
              </strong>
              <code className="ml-2 text-caption text-text-tertiary">skillListingMaxDescChars</code>
            </p>
          )}
          <div className="mt-4 flex items-baseline justify-between gap-2 text-small">
            <strong
              className={entry.state === "truncated" ? "text-warning" : "text-text-secondary"}
            >
              {STATE_LABEL[entry.state]}
            </strong>
            <span className="tabular-nums text-text-tertiary">
              {entry.shown_description_chars} of {entry.full_description_chars} characters
            </span>
          </div>
          <div className="mt-2 rounded-md border border-border-subtle bg-bg-primary px-3 py-3 text-body leading-relaxed">
            {entry.state === "hidden" || entry.state === "omitted" ? (
              <p>{entry.hidden_reason ?? "The skill is absent from this listing."}</p>
            ) : (
              <p className="select-text">
                <strong className="font-mono text-small">{entry.name}</strong>
                {excerpt?.shown && <> · {excerpt.shown}</>}
                {!excerpt && (entry.state === "described" || entry.state === "truncated") && (
                  <span className="text-text-secondary">
                    {" "}
                    · Description text unavailable for this copy.
                  </span>
                )}
              </p>
            )}
            {excerpt?.remainder && entry.state === "truncated" && (
              <div className="mt-2 border-t border-dashed border-border pt-2 text-text-tertiary">
                <span className="text-caption font-medium">Beyond the cutoff</span>
                <p className="mt-0.5 select-text text-small">{excerpt.remainder}</p>
              </div>
            )}
          </div>
        </>
      )}
      {row.observed_session && entry?.observed && (
        <p className="mt-3 text-small text-text-secondary">
          Last recorded listing · <strong>{STATE_LABEL[entry.observed]}</strong>
          {row.observed_session.model && ` · ${row.observed_session.model}`}
        </p>
      )}
      {use && sessions !== undefined && sessions > 0 && (
        <p className="mt-3 text-small text-text-secondary">
          {deploymentLabelFromAgentId(harness)} loaded this skill in{" "}
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

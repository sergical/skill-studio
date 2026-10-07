// ============================================================================
// InstalledSkillHeader - Title, description, state chips, and any blocking
// violation for the rendered copy. The actions cluster (primary action,
// assistant toggle, ⋯ menu) lives in `PageShell`'s `actions` now; the source
// ledger's facts moved into the properties rail.
// ============================================================================

import { AlertTriangle, ExternalLink } from "lucide-react";
import { Button } from "@skill-studio/ui";
import {
  describeFrontmatterErrorLine,
  describeSpecViolations,
  describeFrontmatterRepair,
  isBlockingSpecViolation,
  ownDeployments,
  parseYamlFrontmatterError,
  proposeFrontmatterQuoteRepair,
  specViolationSeverity,
} from "@skill-studio/lib";
import type {
  Deployment,
  FrontmatterQuoteRepair,
  FrontmatterRepairKind,
  FrontmatterRepairPreview,
  InstalledSkill,
  UpstreamAhead,
} from "@skill-studio/lib";
import { useSkillInstalls } from "../../hooks/useSkillInstalls";
import { TooltipControl } from "../ui/TooltipControl";
import { skillSourceLine } from "./skill-source-line";
import { skillUpstreamNote } from "./skill-upstream-note";
import {
  canOfferLocalQuote,
  fixLineFor,
  frontmatterRepairKindForViolation,
  frontmatterRepairKindsFor,
} from "./skill-frontmatter-repair-policy";

interface InstalledSkillHeaderProps {
  skill: InstalledSkill;
  /** Forks whose original repo is ahead; the header notes the one the rendered copy installs from. */
  upstreamAhead?: UpstreamAhead[];
  /** The deployment whose SKILL.md the page renders - the header's violation line follows it. */
  deployment?: Deployment;
  /** The previews the backend accepted, one per repair kind. */
  frontmatterRepairs?: FrontmatterRepairPreview[];
  /** False while the backend preview is pending; the local quote repair waits for it. */
  isFrontmatterPreviewSettled?: boolean;
  /** The rendered copy's SKILL.md text; the quote repair and the line hint are derived from it. */
  skillMdContent?: string | null;
  /** Omitted when the rendered copy cannot be edited in place (plugin-managed). */
  onQuoteRepair?: (repair: FrontmatterQuoteRepair) => void;
  onFixRepair: (kind: FrontmatterRepairKind) => void;
  /** Opens the editor; `line` is the YAML error's line in SKILL.md, when the message names one. */
  onEditManually: (line?: number) => void;
}

/** "Parked · Aug 25, 2026" / "Parked" when the timestamp is missing or unparseable. */
function parkedChipLabel(parkedAt: string | null | undefined): string {
  if (!parkedAt) return "Parked";
  const date = new Date(parkedAt);
  if (Number.isNaN(date.getTime())) return "Parked";
  return `Parked · ${date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
}

/**
 * The installed skill header is identity only: title, description, and state
 * chips, followed by any blocking violation for the rendered copy. It shows
 * no controls, location, or invocation details.
 */
export function InstalledSkillHeader({
  skill,
  upstreamAhead = [],
  deployment,
  frontmatterRepairs = [],
  isFrontmatterPreviewSettled = false,
  skillMdContent,
  onQuoteRepair,
  onFixRepair,
  onEditManually,
}: InstalledSkillHeaderProps) {
  // Notes from the skill's own copies only, as on list rows - a plugin copy's notes are not the
  // user's to fix. A plugin-only skill has no own copy, so it counts every copy.
  const sourceLine = skillSourceLine(skill, useSkillInstalls(skill));
  const own = ownDeployments(skill);
  const noteSources = own.length > 0 ? own : skill.deployments;
  const nonBlockingNotes = [
    ...new Set(
      noteSources.flatMap((d) =>
        d.spec_violations.filter((v) => specViolationSeverity(v) === "note"),
      ),
    ),
  ];
  const nonBlockingCount = nonBlockingNotes.length;
  // The red violation line names only the deployment whose SKILL.md the page
  // actually renders - other deployments' violations show on their own
  // Locations rows instead (see `SkillLocationsCard`). Home's spec-violation
  // issue still relies on `skill.spec_violations` covering every copy.
  const renderedDeployment = deployment ?? skill.deployments.find((d) => d.content_hash);
  // No fallback here: an opened copy that is gone must not borrow another copy's fork origin.
  const upstreamNote = skillUpstreamNote(deployment, upstreamAhead);
  const blockingViolations = (renderedDeployment?.spec_violations ?? []).filter(
    isBlockingSpecViolation,
  );
  const conflictRepair = frontmatterRepairs.find((repair) => repair.kind === "invocation-conflict");
  // With a repair, the conflict has its own red line and Fix below; without one, it shows here.
  const warningViolations = (renderedDeployment?.spec_violations ?? []).filter(
    (v) =>
      specViolationSeverity(v) === "warning" &&
      !(v === "conflicting invocation keys" && conflictRepair),
  );
  const nameFormatNote = (renderedDeployment?.spec_violations ?? []).find(
    (v) => specViolationSeverity(v) === "note" && v.startsWith('name "'),
  );
  const yamlViolation = blockingViolations.find((violation) =>
    violation.startsWith("invalid YAML frontmatter at line "),
  );
  const hasMalformedYaml = yamlViolation !== undefined;
  const yamlLocation = yamlViolation ? parseYamlFrontmatterError(yamlViolation) : null;
  // Conflicting invocation keys are a non-blocking note with their own line and Fix;
  // every other repair belongs to the red violation line.
  const lineKind = frontmatterRepairKindsFor(renderedDeployment).find(
    (kind) => kind !== "invocation-conflict",
  );
  const lineRepair = frontmatterRepairs.find((repair) => repair.kind === lineKind);
  // A backend "Fix" preview wins; the local quote repair covers what it declines.
  const canQuote = canOfferLocalQuote({
    isPreviewSettled: isFrontmatterPreviewSettled,
    hasPreview: Boolean(lineRepair),
  });
  const quoteRepair =
    yamlLocation && skillMdContent && canQuote && onQuoteRepair
      ? proposeFrontmatterQuoteRepair(skillMdContent, yamlLocation.line)
      : null;
  const lineHint =
    yamlLocation && skillMdContent && canQuote
      ? describeFrontmatterErrorLine(skillMdContent, yamlLocation.line)
      : null;
  const fixLine = fixLineFor({
    lineKind,
    hasMismatchLine: warningViolations.some(
      (v) => frontmatterRepairKindForViolation(v) === "name-mismatch",
    ),
    hasNameFormatNote: Boolean(nameFormatNote),
  });
  const lineFixButton = lineRepair ? (
    <Button size="sm" variant="outline" onClick={() => onFixRepair(lineRepair.kind)}>
      Fix
    </Button>
  ) : null;
  const violationText = quoteRepair
    ? describeFrontmatterRepair(quoteRepair, yamlLocation?.column)
    : (lineHint ?? describeSpecViolations(blockingViolations));

  return (
    <header className="flex flex-col gap-4">
      <div>
        <h2 className="text-heading-lg font-semibold text-text-primary">{skill.name}</h2>
        <p className="mt-1 select-text text-small text-text-tertiary">
          {sourceLine.prefix}
          {sourceLine.href ? (
            <a
              href={sourceLine.href}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-text-tertiary no-underline transition-colors hover:text-accent hover:underline"
            >
              {sourceLine.label}
              <ExternalLink size={12} />
            </a>
          ) : (
            sourceLine.label
          )}
          {sourceLine.installs && ` · ${sourceLine.installs}`}
        </p>
        {upstreamNote && (
          <p className="mt-1 select-text text-small text-text-tertiary">
            {upstreamNote.text}{" "}
            <a
              href={upstreamNote.href}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-text-tertiary no-underline transition-colors hover:text-accent hover:underline"
            >
              See changes
              <ExternalLink size={12} />
            </a>
          </p>
        )}
        {skill.description && (
          <p className="mt-3 select-text text-pretty text-body leading-[1.5] text-text-secondary">
            {skill.description}
          </p>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        {skill.parked && (
          <span className="inline-flex items-center gap-1 rounded-full bg-bg-tertiary px-2 py-0.5 text-caption text-warning">
            {parkedChipLabel(skill.parked_at)}
          </span>
        )}
        {skill.update_owner_ids.length > 0 && (
          <span className="inline-flex items-center gap-1 rounded-full bg-bg-tertiary px-2 py-0.5 text-caption text-accent">
            Update available
          </span>
        )}
        {nonBlockingCount > 0 && (
          <TooltipControl content={nonBlockingNotes.join("; ")}>
            <span className="inline-flex items-center gap-1 rounded-full bg-bg-tertiary px-2 py-0.5 text-caption text-text-tertiary">
              {nonBlockingCount} spec note{nonBlockingCount !== 1 ? "s" : ""}
            </span>
          </TooltipControl>
        )}
      </div>

      {blockingViolations.length > 0 && (
        <div className="flex items-center gap-2 text-small text-error">
          <AlertTriangle size={13} />
          <span>{violationText}</span>
          {quoteRepair && (
            <Button size="sm" onClick={() => onQuoteRepair?.(quoteRepair)}>
              Quote the {quoteRepair.key}
            </Button>
          )}
          {hasMalformedYaml &&
            (fixLine === "error" && lineRepair ? (
              lineFixButton
            ) : (
              <Button
                size="sm"
                variant="outline"
                onClick={() => onEditManually(yamlLocation?.line)}
              >
                Edit manually
              </Button>
            ))}
        </div>
      )}
      {warningViolations.length > 0 && (
        <div className="flex items-center gap-2 text-small text-warning">
          <AlertTriangle size={13} />
          <span>{describeSpecViolations(warningViolations)}</span>
          {fixLine === "warning" && lineFixButton}
        </div>
      )}
      {nameFormatNote && (
        <div className="flex items-center gap-2 text-small text-text-tertiary">
          <span>{describeSpecViolations([nameFormatNote])}</span>
          {fixLine === "note" && lineFixButton}
        </div>
      )}
      {conflictRepair && (
        <div className="flex items-center gap-2 text-small text-error">
          <AlertTriangle size={13} />
          <span>Both invocation keys are set, so nothing can run this skill.</span>
          <Button size="sm" variant="outline" onClick={() => onFixRepair("invocation-conflict")}>
            Fix
          </Button>
        </div>
      )}
    </header>
  );
}

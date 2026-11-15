// ============================================================================
// SkillMarkdownCard - the "SKILL.md" card: header (Edit, or Cancel/Save while
// editing), and the rendered markdown / raw-text editor / loading skeleton /
// error state below it.
// ============================================================================

import { Button } from "@skill-studio/ui";
import { deploymentLabel, pluginInfoForSkill } from "@skill-studio/lib";
import type { Deployment, InstalledSkill } from "@skill-studio/lib";
import { SelectControl } from "../ui/SelectControl";
import { TooltipControl } from "../ui/TooltipControl";
import { SkillMarkdown } from "./SkillMarkdown";
import { SkillMarkdownEditor } from "./SkillMarkdownEditor";

/**
 * Whether the card is showing SKILL.md or editing it, and (while editing)
 * the draft's dirty/saving status - one variant instead of three separate
 * `isEditing`/`isEditorDirty`/`isSaving` booleans, since a dirty or saving
 * draft only ever means anything while `kind` is `"editing"`.
 */
type SkillMarkdownEditState =
  | { kind: "viewing" }
  | { kind: "editing"; openedContent: string; isDirty: boolean; isSaving: boolean };

interface SkillMarkdownCardProps {
  skill: InstalledSkill;
  isPluginManaged: boolean;
  /** True when the caller opened a specific deployment that's no longer installed. */
  deploymentUnresolved: boolean;
  /** Own deployments to offer as a fallback when `deploymentUnresolved` - each opens that copy instead. */
  ownDeploymentOptions: Deployment[];
  /** The copy this card shows and edits - named in the header so a multi-copy skill never edits an ambiguous file. */
  deployment?: Deployment;
  onSelectDeployment: (path: string) => void;
  rawContent: string | null;
  isLoadingContent: boolean;
  loadError: string | null;
  onRetry: () => void;
  editState: SkillMarkdownEditState;
  onStartEdit: () => void;
  saveLabel: string;
  onSave: (content: string) => void;
  onCancelEdit: () => void;
  onDirtyChange: (isDirty: boolean) => void;
  /** Line of SKILL.md to mark in the editor's gutter, e.g. a YAML error's line. */
  highlightLine?: number;
}

/** Strips a leading `---\n...\n---\n` YAML frontmatter block, if present. */
function stripFrontmatter(content: string): string {
  return content.replace(/^---\n[\s\S]*?\n---\n/, "");
}

/** A six-line skeleton shown while SKILL.md loads, instead of a spinner. */
/** Widths mirror the original skeleton's per-line variation, so a placeholder line doesn't read as a full sentence. */
const SKELETON_LINE_WIDTHS = ["100%", "92%", "96%", "60%", "88%", "40%"];

function MarkdownSkeleton() {
  return (
    <div className="flex flex-col gap-2.5 px-5 py-4" aria-hidden="true">
      {SKELETON_LINE_WIDTHS.map((width, i) => (
        <div
          key={i}
          className="h-3 animate-pulse rounded-xs bg-bg-tertiary motion-reduce:animate-none"
          style={{ width }}
        />
      ))}
    </div>
  );
}

interface CardHeaderProps {
  deployment?: Deployment;
  deploymentUnresolved: boolean;
  ownDeploymentOptions: Deployment[];
  isEditing: boolean;
  /** Whether this copy can be edited at all; `canEdit` adds "and its text has loaded". */
  editable: boolean;
  canEdit: boolean;
  onSelectDeployment: (path: string) => void;
  onStartEdit: () => void;
}

/** SKILL.md's own header: title, the copy picker or label, and Edit. Every control keeps its box
 * through loading and editing - Edit waits disabled for the text, stays in place hidden while the
 * editor's own toolbar is up, and the picker locks instead of turning into a label - so the
 * header never changes size and the card below it never moves. */
function CardHeader({
  deployment,
  deploymentUnresolved,
  ownDeploymentOptions,
  isEditing,
  editable,
  canEdit,
  onSelectDeployment,
  onStartEdit,
}: CardHeaderProps) {
  return (
    <div className="flex min-h-7 items-center justify-between gap-3 text-body font-semibold text-text-primary">
      <span>SKILL.md</span>
      <span className="flex min-w-0 items-center gap-2">
        {deployment &&
          !deploymentUnresolved &&
          (ownDeploymentOptions.length > 1 ? (
            <SelectControl
              ariaLabel="Copy to show and edit"
              disabled={isEditing}
              value={deployment.path}
              onValueChange={onSelectDeployment}
              items={ownDeploymentOptions.map((d) => ({
                value: d.path,
                label: deploymentLabel(d),
              }))}
            />
          ) : (
            <TooltipControl content={[{ text: deployment.path, mono: true }]}>
              <span className="truncate text-caption font-normal text-text-tertiary">
                {deploymentLabel(deployment)}
              </span>
            </TooltipControl>
          ))}
        {editable && (
          <Button
            variant="outline"
            size="sm"
            onClick={onStartEdit}
            disabled={!canEdit}
            className={isEditing ? "invisible" : undefined}
            aria-hidden={isEditing || undefined}
          >
            Edit
          </Button>
        )}
      </span>
    </div>
  );
}

interface CardBodyProps {
  deploymentUnresolved: boolean;
  ownDeploymentOptions: Deployment[];
  onSelectDeployment: (path: string) => void;
  isPluginManaged: boolean;
  pluginManagedText: string;
  editState: SkillMarkdownEditState;
  rawContent: string | null;
  saveLabel: string;
  onSave: (content: string) => void;
  onCancelEdit: () => void;
  onDirtyChange: (isDirty: boolean) => void;
  highlightLine?: number;
  isLoadingContent: boolean;
  loadError: string | null;
  onRetry: () => void;
}

/** The card's one content slot: unresolved-copy picker, plugin notice, editor, skeleton, error, rendered markdown, or empty state - in that precedence. */
function CardBody({
  deploymentUnresolved,
  ownDeploymentOptions,
  onSelectDeployment,
  isPluginManaged,
  pluginManagedText,
  editState,
  rawContent,
  saveLabel,
  onSave,
  onCancelEdit,
  onDirtyChange,
  highlightLine,
  isLoadingContent,
  loadError,
  onRetry,
}: CardBodyProps) {
  if (deploymentUnresolved) {
    return (
      <div className="m-0 p-3 text-body leading-[1.5] text-text-secondary">
        <p>The copy you opened is no longer installed.</p>
        {ownDeploymentOptions.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {ownDeploymentOptions.map((d) => (
              <Button
                key={d.path}
                variant="outline"
                size="sm"
                onClick={() => onSelectDeployment(d.path)}
              >
                {deploymentLabel(d)}
              </Button>
            ))}
          </div>
        )}
      </div>
    );
  }
  if (isPluginManaged) {
    return (
      <p className="m-0 select-text p-3 text-body leading-[1.5] text-text-secondary">
        {pluginManagedText}
      </p>
    );
  }
  if (editState.kind === "editing" && rawContent !== null) {
    return (
      <SkillMarkdownEditor
        initialContent={editState.openedContent}
        isSaving={editState.isSaving}
        saveLabel={saveLabel}
        onSave={onSave}
        onCancel={onCancelEdit}
        onDirtyChange={onDirtyChange}
        highlightLine={highlightLine}
      />
    );
  }
  if (isLoadingContent) return <MarkdownSkeleton />;
  if (loadError) {
    return (
      <div className="m-0 flex items-center justify-between gap-3 p-3 text-body leading-[1.5] text-error">
        <span className="select-text">{loadError}</span>
        <Button variant="outline" size="sm" onClick={onRetry}>
          Retry
        </Button>
      </div>
    );
  }
  if (rawContent !== null) {
    return <SkillMarkdown content={stripFrontmatter(rawContent)} className="px-4 py-3" />;
  }
  return <p className="m-0 px-3 py-6 text-small text-text-tertiary">No content available</p>;
}

export function SkillMarkdownCard({
  skill,
  isPluginManaged,
  deploymentUnresolved,
  ownDeploymentOptions,
  deployment,
  onSelectDeployment,
  rawContent,
  isLoadingContent,
  loadError,
  onRetry,
  editState,
  onStartEdit,
  saveLabel,
  onSave,
  onCancelEdit,
  onDirtyChange,
  highlightLine,
}: SkillMarkdownCardProps) {
  const isEditing = editState.kind === "editing";
  const editable = !isPluginManaged && !deploymentUnresolved;
  const canEdit = editable && !isEditing && rawContent !== null;
  const plugin = isPluginManaged ? pluginInfoForSkill(skill) : undefined;
  const pluginManagedText = plugin
    ? `Managed by the ${plugin.name} plugin for ${plugin.harness}.`
    : "Managed by a plugin.";

  return (
    <div className="flex flex-col gap-1 rounded-lg border border-border-subtle p-4">
      <CardHeader
        deployment={deployment}
        deploymentUnresolved={deploymentUnresolved}
        ownDeploymentOptions={ownDeploymentOptions}
        isEditing={isEditing}
        editable={editable}
        canEdit={canEdit}
        onSelectDeployment={onSelectDeployment}
        onStartEdit={onStartEdit}
      />
      <CardBody
        deploymentUnresolved={deploymentUnresolved}
        ownDeploymentOptions={ownDeploymentOptions}
        onSelectDeployment={onSelectDeployment}
        isPluginManaged={isPluginManaged}
        pluginManagedText={pluginManagedText}
        editState={editState}
        rawContent={rawContent}
        saveLabel={saveLabel}
        onSave={onSave}
        onCancelEdit={onCancelEdit}
        onDirtyChange={onDirtyChange}
        highlightLine={highlightLine}
        isLoadingContent={isLoadingContent}
        loadError={loadError}
        onRetry={onRetry}
      />
    </div>
  );
}

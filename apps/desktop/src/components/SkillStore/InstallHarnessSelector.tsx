// ============================================================================
// InstallHarnessSelector - the Destination section of an install: a Universal
// row for the shared `.agents/skills` folder, then one row per harness. A tick
// always means "this harness gets the skill". With Universal ticked, the
// harnesses that read the shared folder get it from there, so they show as one
// "Read by" line under Universal instead of locked rows. With Universal
// unticked, each ticked harness gets its own copy in its own folder.
// ============================================================================

import { Folder } from "lucide-react";
import {
  installHarness,
  installHarnessLocked,
  toggleInstallHarness,
  universalDestinationPath,
} from "@skill-studio/lib";
import type { AgentId, InstallScope } from "@skill-studio/lib";
import { CheckboxControl } from "../ui/CheckboxControl";
import { HarnessIcon } from "../ui/HarnessIcon";
import { TooltipControl } from "../ui/TooltipControl";

const ROW_CLASS =
  "grid h-9 grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-2 rounded-sm px-2 hover:bg-bg-hover";

interface InstallRowProps {
  icon: React.ReactNode;
  label: string;
  folder: string;
  checked: boolean;
  onCheckedChange: (on: boolean) => void;
  disabled: boolean;
  /** Shown on hover when the checkbox is disabled. */
  reason: string | null;
}

function InstallRow({
  icon,
  label,
  folder,
  checked,
  onCheckedChange,
  disabled,
  reason,
}: InstallRowProps) {
  const checkbox = (
    <CheckboxControl
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      ariaLabel={`Install for ${label}`}
    />
  );
  // A <label> row: the whole row toggles, and the drawer's swipe-to-close leaves
  // presses on a label alone (it captures presses on a bare role="checkbox").
  return (
    <label className={`${ROW_CLASS} ${disabled ? "" : "cursor-pointer"}`}>
      {icon}
      <span className="flex min-w-0 flex-col">
        <span className="truncate text-body text-text-primary">{label}</span>
        <span className="truncate font-mono text-caption text-text-tertiary">{folder}</span>
      </span>
      {reason ? (
        <TooltipControl content={reason}>
          <span className="inline-flex">{checkbox}</span>
        </TooltipControl>
      ) : (
        checkbox
      )}
    </label>
  );
}

/** The agents that get the skill from the ticked Universal folder, as marks with names. */
function ReadByLine({ harnesses }: { harnesses: readonly AgentId[] }) {
  return (
    <p className="m-0 flex flex-wrap items-center gap-x-3 gap-y-1 pb-1 pl-8 text-caption text-text-tertiary">
      <span>Read by</span>
      {harnesses.map((id) => (
        <span key={id} className="inline-flex items-center gap-1 text-text-secondary">
          <HarnessIcon harness={id} size={12} />
          {installHarness(id)?.label ?? id}
        </span>
      ))}
    </p>
  );
}

interface InstallHarnessSelectorProps {
  offered: readonly AgentId[];
  chosen: readonly AgentId[];
  onChosenChange: (chosen: AgentId[]) => void;
  universal: boolean;
  onUniversalChange: (universal: boolean) => void;
  /** Set when Universal cannot be unticked, and why. */
  universalLockedReason?: string | null;
  /** True when `.claude/skills` is a whole-folder link to the shared folder. */
  claudeReadsShared: boolean;
  scope: InstallScope;
  disabled?: boolean;
  /** Set when the install method picks the harnesses itself. */
  lockedReason?: string;
  /** Set when the choice cannot be installed, for example no harness ticked. */
  error?: string | null;
  /** False where the caller already says that copies get no updates. */
  copyNote?: boolean;
}

export function InstallHarnessSelector({
  offered,
  chosen,
  onChosenChange,
  universal,
  onUniversalChange,
  universalLockedReason = null,
  claudeReadsShared,
  scope,
  disabled = false,
  lockedReason,
  error = null,
  copyNote = true,
}: InstallHarnessSelectorProps) {
  const chosenSet = new Set(chosen);
  const rowsLocked = disabled || (universal && !!lockedReason);
  const readers = offered.filter((id) => installHarnessLocked(id, claudeReadsShared, universal));
  const readerSet = new Set(readers);
  const ownRows = offered.filter((id) => !readerSet.has(id));
  return (
    <div className="flex flex-col gap-2">
      <span className="text-small font-medium text-text-secondary">Destination</span>
      {universal && lockedReason && (
        <p className="m-0 text-caption text-text-tertiary">{lockedReason}</p>
      )}

      <div className="-mx-2 flex flex-col">
        <InstallRow
          icon={<Folder size={16} className="text-text-secondary" aria-hidden="true" />}
          label="Universal"
          folder={universalDestinationPath(scope)}
          checked={universal}
          onCheckedChange={onUniversalChange}
          disabled={disabled || !!universalLockedReason}
          reason={universalLockedReason}
        />
        {readers.length > 0 && <ReadByLine harnesses={readers} />}
        {ownRows.map((id) => {
          const harness = installHarness(id);
          const label = harness?.label ?? id;
          return (
            <InstallRow
              key={id}
              icon={<HarnessIcon harness={id} size={16} />}
              label={label}
              folder={harness?.folder[scope] ?? ""}
              checked={chosenSet.has(id)}
              onCheckedChange={(on) =>
                onChosenChange(toggleInstallHarness(offered, chosen, id, on))
              }
              disabled={rowsLocked}
              reason={null}
            />
          );
        })}
      </div>

      {error && <p className="m-0 text-caption text-error">{error}</p>}
      {!universal && copyNote && (
        <p className="m-0 text-caption text-text-tertiary">
          Copies aren&apos;t tracked, so they don&apos;t get updates.
        </p>
      )}
    </div>
  );
}

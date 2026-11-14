// ============================================================================
// HomeInboxGroups - Home's five inbox groups (Broken, Warnings, Updates, Not
// used in 30 days, Recently used): the shared row layout, each group's
// header and rows, and Update all. Rows share their cells, fix button and
// selection with the Skills page; an acted-on row keeps its slot.
// ============================================================================

import { useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode, SyntheticEvent } from "react";
import { Button, Collapsible, CollapsiblePanel, cn } from "@skill-studio/ui";
import { formatRelativeTime, shortSha } from "@skill-studio/lib";
import type { HealthIssue, InstalledSkill, RecentlyUsedSkill } from "@skill-studio/lib";
import { skillLocalEdits } from "../../lib/skill-api";
import {
  conflictedSkillsNote,
  skillCanPark,
  partitionForkableSkills,
  skillsWithLocalEdits,
} from "../../lib/skill-lifecycle-target";
import { useAppStore } from "../../store/appStore";
import {
  startHomeUpdateAll,
  stopHomeUpdateAll,
  useActiveUpdateAll,
} from "../../hooks/skillBatchUpdates";
import { UpdateOverwritesEditsDialog } from "../SkillDetail/UpdateOverwritesEditsDialog";
import { GroupHead } from "../SkillList/GroupHead";
import { DEFAULT_HARNESS_LIST, whereFacts } from "../SkillList/skill-row-state";
import type { RowState } from "../SkillList/skill-row-state";
import { HarnessStack } from "../SkillList/HarnessStack";
import { NoteCell, SkillNameCell, SlotCell } from "../SkillList/SkillRowCells";
import { FIX_CLASS, ROW_CLASS, SKILL_ROW_GRID } from "../SkillList/skill-row-format";
import { RowFixButton } from "../SkillList/RowFixButton";
import { SkillLocationCell } from "../SkillList/SkillLocationCell";
import { RichTooltipScope } from "../ui/RichTooltip";
import {
  homeRowState,
  issueDeploymentPath,
  issueKey,
  MAX_ROWS_PER_GROUP,
  rowAt,
  skillKey,
  updateAllFailureMessage,
  updateAllTitle,
} from "./home-inbox-data";
import { rowClickOpensSkill } from "../SkillList/skill-row-click";
import type { GroupId, HomeFilter, HomeInboxLists, HomeRowPlan } from "./home-inbox-data";

/** Text link style shared by every "Show all"/"Show everything"/"Learn more" affordance on Home. */
const LINK_CLASS = "h-auto gap-1 p-0 text-small";

/** What every group's rows share, built once by `HomeView`: the roving cursor and the per-skill
 * selection the Skills page uses too. */
export interface HomeRowWiring {
  rowRef: (key: string) => (el: HTMLDivElement | null) => void;
  tabIndexFor: (key: string) => 0 | -1;
  isChecked: (skill: InstalledSkill) => boolean;
  /** Whether any skill on the page is checked. */
  hasSelection: boolean;
  onToggle: (skill: InstalledSkill) => void;
  /** Runs a row's fix - the same dispatch the Skills page and the selection bar use. */
  act: (label: string, skill: InstalledSkill) => void;
}

/**
 * One row of any inbox group - the same cells, grid, and slot as a Skills row. The note is the
 * group's own detail text and the last cell is the group's fix. The state comes from the
 * caller, not the skill's own worst condition, so a Broken/Warnings row always matches the
 * group it sits in.
 */
function HomeRow({
  rowKey,
  wiring,
  skill,
  state,
  detail,
  fix,
  busy,
  custom,
  onOpen,
  rowIndex,
}: {
  rowKey: string;
  wiring: HomeRowWiring;
  skill: InstalledSkill;
  state: RowState | null;
  detail: ReactNode;
  /** The fix's verb, run through `wiring.act`; `null` for none. */
  fix: string | null;
  /** See `RowFixButton`'s `busy`. */
  busy?: boolean;
  /** A fix with its own control, shown in place of the fix button. */
  custom?: ReactNode;
  onOpen: () => void;
  rowIndex: number;
}) {
  const checked = wiring.isChecked(skill);
  return (
    <div
      ref={wiring.rowRef(rowKey)}
      role="row"
      aria-rowindex={rowIndex}
      aria-selected={checked}
      tabIndex={wiring.tabIndexFor(rowKey)}
      onClick={(e) => {
        // SAFETY: a click's target is always an Element; browsers retarget clicks on text
        // to the parent element.
        if (rowClickOpensSkill(e.currentTarget, e.target as Element)) onOpen();
      }}
      className={`${ROW_CLASS} ${SKILL_ROW_GRID} hover:bg-bg-secondary focus-visible:outline-2 focus-visible:outline-accent -outline-offset-2`}
    >
      <div role="gridcell" className="flex items-center justify-center">
        <SlotCell
          skill={skill}
          state={state}
          checked={checked}
          hasSelection={wiring.hasSelection}
          onCheckedChange={() => wiring.onToggle(skill)}
        />
      </div>
      <div role="gridcell" className="contents">
        <SkillNameCell skill={skill} />
      </div>
      <div role="gridcell" className="contents">
        <SkillLocationCell locations={whereFacts(skill, DEFAULT_HARNESS_LIST).locations} />
      </div>
      <div role="gridcell" className="contents">
        <HarnessStack skill={skill} harnessList={DEFAULT_HARNESS_LIST} />
      </div>
      <div role="gridcell" className="contents">
        <NoteCell skill={skill} note={detail} />
      </div>
      <div role="gridcell" className="flex min-w-0 justify-end">
        <RowFixButton skill={skill} label={fix} onAct={wiring.act} busy={busy}>
          {custom}
        </RowFixButton>
      </div>
    </div>
  );
}

/** "Show all N" footer link for a group, navigating to the matching Skills/Activity filter. */
function ShowAllLink({
  count,
  label,
  onClick,
}: {
  count: number;
  label: string;
  onClick: () => void;
}) {
  return (
    <div className="flex h-9 items-center px-4">
      <Button variant="link" className={LINK_CLASS} onClick={onClick}>
        {label} {count}
      </Button>
    </div>
  );
}

/** "Convert" for a linked-root warning: opens the dialog that turns the linked folder into copies. */
function ConvertButton({ onConvert }: { onConvert: () => void }) {
  return (
    <Button
      variant="ghost"
      className={FIX_CLASS}
      onClick={(e) => {
        e.stopPropagation();
        onConvert();
      }}
    >
      Convert
    </Button>
  );
}

/** Home's "Broken" group: one row per broken-symlink/parked-but-reinstalled/spec-violation issue. */
function BrokenGroup({
  broken,
  count,
  start,
  isExpanded,
  onToggle,
  onSelectSkill,
  onShowAll,
  wiring,
}: {
  wiring: HomeRowWiring;
  broken: readonly HealthIssue[];
  /** The live count, which a held row does not add to. */
  count: number;
  start: number;
  isExpanded: boolean;
  onToggle: () => void;
  onSelectSkill: (name: string, deploymentPath?: string) => void;
  onShowAll: () => void;
}) {
  return (
    <Collapsible data-group="broken" role="rowgroup" open={isExpanded} onOpenChange={onToggle}>
      <div role="row">
        <div role="gridcell">
          <GroupHead label="Broken" count={count} groupId="broken" />
        </div>
      </div>
      <CollapsiblePanel>
        <div className="flex flex-col">
          {broken.slice(0, MAX_ROWS_PER_GROUP).map((issue, i) => {
            const key = issueKey("broken", issue);
            const state = homeRowState("broken", issue.skill, issue);
            return (
              <HomeRow
                key={key}
                skill={issue.skill}
                state={state}
                rowIndex={rowAt(start, i)}
                rowKey={key}
                wiring={wiring}
                onOpen={() => onSelectSkill(issue.skill.name, issueDeploymentPath(issue))}
                detail={<span>{issue.detail}</span>}
                fix={state?.action ?? null}
              />
            );
          })}
          {broken.length > MAX_ROWS_PER_GROUP && (
            <ShowAllLink count={broken.length} label="Show all" onClick={onShowAll} />
          )}
        </div>
      </CollapsiblePanel>
    </Collapsible>
  );
}

/** Home's "Warnings" group: one row per duplicate/linked-root/lock-only issue. */
function WarningsGroup({
  warnings,
  count,
  start,
  isExpanded,
  onToggle,
  onSelectSkill,
  onShowAll,
  onConvertLinkedRoot,
  wiring,
}: {
  wiring: HomeRowWiring;
  warnings: readonly HealthIssue[];
  count: number;
  start: number;
  isExpanded: boolean;
  onToggle: () => void;
  onSelectSkill: (name: string, deploymentPath?: string) => void;
  onShowAll: () => void;
  onConvertLinkedRoot: (
    skill: InstalledSkill,
    harness: string,
    harnessLabel: string,
    root: string,
  ) => void;
}) {
  return (
    <Collapsible data-group="warn" role="rowgroup" open={isExpanded} onOpenChange={onToggle}>
      <div role="row">
        <div role="gridcell">
          <GroupHead label="Warnings" count={count} groupId="warn" />
        </div>
      </div>
      <CollapsiblePanel>
        <div className="flex flex-col">
          {warnings.slice(0, MAX_ROWS_PER_GROUP).map((issue, i) => {
            const key = issueKey("warn", issue);
            const state = homeRowState("warn", issue.skill, issue);
            const { harness, root } = issue;
            return (
              <HomeRow
                key={key}
                skill={issue.skill}
                state={state}
                rowIndex={rowAt(start, i)}
                rowKey={key}
                wiring={wiring}
                onOpen={() => onSelectSkill(issue.skill.name, issueDeploymentPath(issue))}
                detail={<span>{issue.detail}</span>}
                fix={state?.action ?? null}
                custom={
                  issue.kind === "linked-root" &&
                  harness &&
                  root && (
                    <ConvertButton
                      onConvert={() =>
                        onConvertLinkedRoot(
                          issue.skill,
                          harness,
                          issue.harnessLabel ?? harness,
                          root,
                        )
                      }
                    />
                  )
                }
              />
            );
          })}
          {warnings.length > MAX_ROWS_PER_GROUP && (
            <ShowAllLink count={warnings.length} label="Show all" onClick={onShowAll} />
          )}
        </div>
      </CollapsiblePanel>
    </Collapsible>
  );
}

/** Home's "Not used in the last 30 days" group: one row per idle skill, with a one-click Park. */
function UnusedGroup({
  unused,
  count,
  start,
  isExpanded,
  onToggle,
  onSelectSkill,
  onShowAll,
  wiring,
}: {
  wiring: HomeRowWiring;
  unused: readonly InstalledSkill[];
  count: number;
  start: number;
  isExpanded: boolean;
  onToggle: () => void;
  onSelectSkill: (name: string) => void;
  onShowAll: () => void;
}) {
  return (
    <Collapsible data-group="unused" role="rowgroup" open={isExpanded} onOpenChange={onToggle}>
      <div role="row">
        <div role="gridcell">
          <GroupHead label="Not used in the last 30 days" count={count} groupId="unused" />
        </div>
      </div>
      <CollapsiblePanel>
        <div className="flex flex-col">
          {unused.slice(0, MAX_ROWS_PER_GROUP).map((skill, i) => {
            const projectDeployment = skill.deployments.find((d) => d.project_path);
            const scopeLabel = projectDeployment?.project_path
              ? (projectDeployment.project_path.split("/").filter(Boolean).pop() ?? "Global")
              : "Global";
            const modelInvocable = skill.invocation !== "user-only";
            const key = skillKey("unused", skill);
            return (
              <HomeRow
                key={key}
                skill={skill}
                state={homeRowState("unused", skill, null)}
                rowIndex={rowAt(start, i)}
                rowKey={key}
                wiring={wiring}
                onOpen={() => onSelectSkill(skill.name)}
                detail={
                  <span>
                    {scopeLabel} · installed {formatRelativeTime(skill.installed_at)} ·{" "}
                    {modelInvocable ? (
                      "description in every prompt"
                    ) : (
                      <span className="text-text-tertiary">user-only, not in the prompt</span>
                    )}
                  </span>
                }
                fix={modelInvocable && skillCanPark(skill) ? "Park" : null}
              />
            );
          })}
          {unused.length > MAX_ROWS_PER_GROUP && (
            <ShowAllLink count={unused.length} label="Show all" onClick={onShowAll} />
          )}
        </div>
      </CollapsiblePanel>
    </Collapsible>
  );
}

/** Home's "Recently used" group: one row per skill used in the last 30 days, newest first. */
function RecentGroup({
  recent,
  start,
  isExpanded,
  onToggle,
  onSelectSkill,
  onShowAll,
  wiring,
}: {
  wiring: HomeRowWiring;
  recent: readonly RecentlyUsedSkill[];
  start: number;
  isExpanded: boolean;
  onToggle: () => void;
  onSelectSkill: (name: string) => void;
  onShowAll: () => void;
}) {
  return (
    <Collapsible data-group="rec" role="rowgroup" open={isExpanded} onOpenChange={onToggle}>
      <div role="row">
        <div role="gridcell">
          <GroupHead label="Recently used" count={recent.length} groupId="rec" />
        </div>
      </div>
      <CollapsiblePanel>
        <div className="flex flex-col">
          {recent.map(({ skill, lastUsed, projectLabel, usesIn30Days }, i) => {
            const key = skillKey("rec", skill);
            return (
              <HomeRow
                key={key}
                skill={skill}
                state={homeRowState("rec", skill, null)}
                rowIndex={rowAt(start, i)}
                rowKey={key}
                wiring={wiring}
                onOpen={() => onSelectSkill(skill.name)}
                detail={
                  <span className="tabular-nums">
                    {projectLabel ?? "Global"} · {formatRelativeTime(lastUsed)} · {usesIn30Days}{" "}
                    uses
                  </span>
                }
                fix={null}
              />
            );
          })}
          <ShowAllLink count={0} label="See all activity" onClick={onShowAll} />
        </div>
      </CollapsiblePanel>
    </Collapsible>
  );
}

/**
 * Home's "Updates" group: one row per skill with a newer commit, "Update all" in the header. While
 * Update all runs, its rows stay put: the one being updated spins, the rest wait, and each done
 * row says "Updated" until the list is laid out again.
 */
function UpdatesGroup({
  updates,
  liveUpdates,
  isExpanded,
  onToggle,
  onSelectSkill,
  onShowAll,
  start,
  wiring,
}: {
  wiring: HomeRowWiring;
  /** The rows shown, held rows included. */
  updates: readonly InstalledSkill[];
  /** The skills that still have an update; what the count says and what Update all runs on. */
  liveUpdates: readonly InstalledSkill[];
  isExpanded: boolean;
  onToggle: () => void;
  onSelectSkill: (name: string) => void;
  onShowAll: () => void;
  /** This group's offset into the page's continuous `aria-rowindex` sequence. */
  start: number;
}) {
  const addToast = useAppStore((state) => state.addToast);
  const [editedSkills, setEditedSkills] = useState<InstalledSkill[]>([]);
  const [isCheckingEdits, setIsCheckingEdits] = useState(false);
  const activeBatch = useActiveUpdateAll();
  const progress = activeBatch?.progress ?? null;
  const isStopping = activeBatch?.isStopping ?? false;

  const runUpdateAll = async (
    forkNames?: ReadonlySet<string>,
    skippedNames: ReadonlySet<string> = new Set(),
  ) => {
    const { holdSkills, settleSkills, releaseSkills } = useAppStore.getState();
    // A row already updating on its own stays with that act.
    const names = holdSkills(
      liveUpdates.flatMap((skill) => (skippedNames.has(skill.name) ? [] : [skill.name])),
    );
    const held = new Set(names);
    const batch = liveUpdates.filter((skill) => held.has(skill.name));
    // `updateAllOutdatedSkills` catches every `pullFork`/`updateAllOwners`
    // rejection itself and folds it into `failures`, so this never throws.
    const tally = await startHomeUpdateAll(batch, forkNames);
    if (!tally) {
      releaseSkills(names);
      return;
    }
    const updated = new Set(tally.updated);
    settleSkills(tally.updated, "Updated");
    releaseSkills(names.filter((name) => !updated.has(name)));
    const { failures } = tally;
    const conflictNote = conflictedSkillsNote(tally.conflicted ?? []);
    addToast({
      type: failures > 0 || conflictNote || tally.stopped ? "warning" : "success",
      title: updateAllTitle(tally),
      message:
        [
          updateAllFailureMessage(tally),
          conflictNote,
          skippedNames.size > 0 ? `Skipped ${[...skippedNames].join(", ")}` : null,
        ]
          .filter(Boolean)
          .join(". ") || undefined,
    });
  };

  const handleStop = async () => {
    try {
      await stopHomeUpdateAll();
    } catch (error) {
      addToast({
        type: "error",
        title: "Could not stop Update all",
        message: error instanceof Error ? error.message : "Unknown error",
      });
    }
  };

  const handleUpdateAll = async () => {
    setIsCheckingEdits(true);
    // `skillsWithLocalEdits` treats a failed check as "no edits", so it never rejects.
    const edited = await skillsWithLocalEdits([...liveUpdates], skillLocalEdits);
    setIsCheckingEdits(false);
    if (edited.length > 0) {
      setEditedSkills(edited);
      return;
    }
    await runUpdateAll();
  };

  const closeEditsDialog = () => setEditedSkills([]);
  const editedChoices = partitionForkableSkills(editedSkills);
  const forkNames = editedChoices.forkable.map((skill) => skill.name);
  const unforkableNames = editedChoices.unforkable.map((skill) => skill.name);

  return (
    <Collapsible data-group="upd" role="rowgroup" open={isExpanded} onOpenChange={onToggle}>
      <div role="row">
        <div role="gridcell">
          <GroupHead
            label="Updates"
            count={liveUpdates.length}
            groupId="upd"
            extra={
              updates.length > 1 && (
                <Button
                  variant="link"
                  className="h-auto p-0 text-small disabled:text-text-quaternary"
                  busy={isCheckingEdits || isStopping ? true : undefined}
                  disabled={!activeBatch && liveUpdates.length === 0}
                  onClick={(e) => {
                    e.stopPropagation();
                    void (activeBatch ? handleStop() : handleUpdateAll());
                  }}
                >
                  {/* Both labels share one cell, so the header keeps one width whichever shows. */}
                  <span className="grid">
                    <span
                      className={cn("col-start-1 row-start-1", activeBatch && "invisible")}
                      aria-hidden={activeBatch ? true : undefined}
                    >
                      Update all
                    </span>
                    <span
                      className={cn("col-start-1 row-start-1", !activeBatch && "invisible")}
                      aria-hidden={activeBatch ? undefined : true}
                    >
                      Stop
                    </span>
                  </span>
                </Button>
              )
            }
          />
          <div role="status" className="sr-only">
            {progress && progress.total > 0 ? `Updating ${progress.done} of ${progress.total}` : ""}
          </div>
        </div>
      </div>
      <CollapsiblePanel>
        <div className="flex flex-col">
          {updates.slice(0, MAX_ROWS_PER_GROUP).map((skill, i) => {
            const key = skillKey("upd", skill);
            return (
              <HomeRow
                key={key}
                skill={skill}
                state={homeRowState("upd", skill, null)}
                rowIndex={rowAt(start, i)}
                rowKey={key}
                wiring={wiring}
                onOpen={() => onSelectSkill(skill.name)}
                detail={
                  <>
                    {skill.content_hash && skill.update_commit && skill.update_commit_at && (
                      <span className="font-mono text-caption whitespace-nowrap text-text-tertiary">
                        {shortSha(skill.content_hash)} → {shortSha(skill.update_commit)}
                      </span>
                    )}
                    {skill.update_commit && !skill.update_commit_at && (
                      <span className="text-caption whitespace-nowrap text-text-tertiary">
                        Update available
                      </span>
                    )}
                  </>
                }
                fix="Update"
                busy={progress ? progress.current === skill.name : undefined}
              />
            );
          })}
          {updates.length > MAX_ROWS_PER_GROUP && (
            <ShowAllLink count={liveUpdates.length} label="Show all" onClick={onShowAll} />
          )}
        </div>
      </CollapsiblePanel>
      <UpdateOverwritesEditsDialog
        skillNames={editedSkills.map((skill) => skill.name)}
        isBulk
        canFork={forkNames.length > 0}
        unforkableSkillNames={unforkableNames}
        onFork={(choice) => {
          closeEditsDialog();
          void runUpdateAll(
            new Set(forkNames),
            choice === "skip" ? new Set(unforkableNames) : undefined,
          );
        }}
        onOverwrite={() => {
          closeEditsDialog();
          void runUpdateAll();
        }}
        onCancel={closeEditsDialog}
      />
    </Collapsible>
  );
}

/**
 * Home's grid: the filter banner, the all-clear empty state, and the five inbox groups (each shown
 * only while visible under the active filter) - pulled out of `HomeView` since this is where all of
 * its per-group branching lived.
 */
export function HomeInboxGrid({
  groups,
  live,
  starts,
  filter,
  onClearFilter,
  isGroupVisible,
  isGroupExpanded,
  toggleGroup,
  onSelectSkill,
  onShowAllIssues,
  onShowAllUpdates,
  onShowAllUnused,
  onShowAllRecent,
  onConvertLinkedRoot,
  containerRef,
  onGridKeyDown,
  followRow,
  wiring,
}: {
  wiring: HomeRowWiring;
  /** The rows shown: acted-on rows held where they were. */
  groups: HomeInboxLists;
  /** The current lists, for the counts and for Update all. */
  live: HomeInboxLists;
  starts: HomeRowPlan["starts"];
  filter: HomeFilter | null;
  onClearFilter: () => void;
  isGroupVisible: (id: GroupId) => boolean;
  isGroupExpanded: (id: GroupId) => boolean;
  toggleGroup: (id: GroupId) => void;
  onSelectSkill: (name: string, deploymentPath?: string) => void;
  onShowAllIssues: () => void;
  onShowAllUpdates: () => void;
  onShowAllUnused: () => void;
  onShowAllRecent: () => void;
  onConvertLinkedRoot: (
    skill: InstalledSkill,
    harness: string,
    harnessLabel: string,
    root: string,
  ) => void;
  containerRef: (el: HTMLDivElement | null) => void;
  onGridKeyDown: (e: ReactKeyboardEvent) => void;
  followRow: (e: SyntheticEvent) => void;
}) {
  const { broken, warnings, updates, unused, recent } = groups;
  const allClear = broken.length === 0 && warnings.length === 0 && updates.length === 0;

  return (
    <RichTooltipScope>
      <div
        ref={containerRef}
        className="flex flex-col"
        role="grid"
        aria-label="Home"
        onKeyDown={onGridKeyDown}
        onPointerDown={followRow}
        onFocus={followRow}
      >
        {filter && (
          <div className="flex h-9 items-center gap-2.5 px-3 text-small text-text-tertiary">
            Showing one group ·{" "}
            <Button variant="link" className={LINK_CLASS} onClick={onClearFilter}>
              Show everything
            </Button>
          </div>
        )}

        {allClear && !filter && (
          <p className="flex h-full items-center justify-center text-wrap-pretty text-text-tertiary">
            All clear. Nothing needs attention.
          </p>
        )}

        {broken.length > 0 && isGroupVisible("broken") && (
          <BrokenGroup
            broken={broken}
            count={live.broken.length}
            start={starts.broken}
            isExpanded={isGroupExpanded("broken")}
            onToggle={() => toggleGroup("broken")}
            onSelectSkill={onSelectSkill}
            onShowAll={onShowAllIssues}
            wiring={wiring}
          />
        )}

        {warnings.length > 0 && isGroupVisible("warn") && (
          <WarningsGroup
            warnings={warnings}
            count={live.warnings.length}
            start={starts.warn}
            isExpanded={isGroupExpanded("warn")}
            onToggle={() => toggleGroup("warn")}
            onSelectSkill={onSelectSkill}
            onShowAll={onShowAllIssues}
            onConvertLinkedRoot={onConvertLinkedRoot}
            wiring={wiring}
          />
        )}

        {updates.length > 0 && isGroupVisible("upd") && (
          <UpdatesGroup
            updates={updates}
            liveUpdates={live.updates}
            isExpanded={isGroupExpanded("upd")}
            onToggle={() => toggleGroup("upd")}
            onSelectSkill={onSelectSkill}
            onShowAll={onShowAllUpdates}
            start={starts.upd}
            wiring={wiring}
          />
        )}

        {unused.length > 0 && isGroupVisible("unused") && (
          <UnusedGroup
            unused={unused}
            count={live.unused.length}
            start={starts.unused}
            isExpanded={isGroupExpanded("unused")}
            onToggle={() => toggleGroup("unused")}
            onSelectSkill={onSelectSkill}
            onShowAll={onShowAllUnused}
            wiring={wiring}
          />
        )}

        {recent.length > 0 && isGroupVisible("rec") && (
          <RecentGroup
            recent={recent}
            start={starts.rec}
            isExpanded={isGroupExpanded("rec")}
            onToggle={() => toggleGroup("rec")}
            onSelectSkill={onSelectSkill}
            onShowAll={onShowAllRecent}
            wiring={wiring}
          />
        )}
      </div>
    </RichTooltipScope>
  );
}

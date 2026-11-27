// ============================================================================
// HomeView - What needs the user, the skills they use most and each agent's
// skill space (HomeOverview), then the grouped list of their skills (Broken,
// Warnings, Updates, Not used in 30 days, Recently used) the to-dos filter.
// ============================================================================

import { useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { Button } from "@skill-studio/ui";
import type { InstalledSkill, LifecycleTarget, SkillSnapshot } from "@skill-studio/lib";
import { useHeldLayout } from "../../lib/held-layout";
import { lifecycleTargetForHarnessRoot } from "../../lib/skill-lifecycle-target";
import { useAppStore } from "../../store/appStore";
import { PageShell } from "../Shell/PageShell";
import { SkillListSelectionBar } from "../SkillList/SkillListSelectionBar";
import { useSkillListAct } from "../SkillList/useSkillListAct";
import { MaterializeRootDialog } from "../ui/MaterializeRootDialog";
import { useUpdateCheck } from "./useUpdateCheck";
import { HomeOverview } from "./HomeOverview";
// PROTOTYPE home-library
import { HomeLibraryPrototype } from "../../prototype/home-library/HomeLibraryPrototype";
import { HomeInboxGrid } from "./HomeInboxGroups";
import { buildHomeRowPlan, computeHomeGroups } from "./home-inbox-data";
import type { HomeInboxLists } from "./home-inbox-data";
import { useHomeGroupVisibility } from "./useHomeGroupVisibility";
import { useHomeRowCursor } from "./useHomeRowCursor";

const NO_UPDATE_CHECK: SkillSnapshot["update_check"] = {
  checked_at: null,
  gh_status: "ok",
  message: null,
  updates_available: 0,
  upstream_ahead: [],
};

interface HomeViewProps {
  snapshot: SkillSnapshot | undefined;
  isLoading: boolean;
  usageRefreshing?: boolean;
  onSelectSkill: (name: string, deploymentPath?: string) => void;
  /** Whether Home is the view on screen right now - `false` while it's kept mounted but hidden
   * behind an open skill's page, so its window-level keyboard shortcuts stay off. */
  active: boolean;
}

/**
 * First-scan loading state, mirroring the loaded layout's dimensions (stat
 * tiles, lane card, list rows) so the dashboard fades in without a layout
 * jump - and so "empty" text only ever means the scan really found nothing.
 */
function HomeSkeleton() {
  const bar = "animate-pulse rounded-xs bg-bg-tertiary motion-reduce:animate-none";
  return (
    <PageShell title="Home">
      <div className="grid grid-cols-3 gap-3" aria-hidden="true">
        {["Broken", "Warnings", "Updates"].map((label) => (
          <div
            key={label}
            className="flex flex-1 flex-col gap-1 rounded-md border border-border-subtle bg-bg-elevated px-4 py-3.5"
          >
            <span className="text-small text-text-tertiary">{label}</span>
            <span className="text-display leading-[1.1] font-semibold">
              <span className={`inline-block h-[1em] w-7 align-middle ${bar}`} />
            </span>
          </div>
        ))}
      </div>
      <section
        className="flex flex-col gap-2 rounded-md border border-border-subtle bg-bg-elevated px-4 py-3.5"
        aria-hidden="true"
      >
        {["Who can invoke", "Prompt cost"].map((label) => (
          <div key={label} className="grid grid-cols-[210px_minmax(0,1fr)] items-center gap-3">
            <span className="text-small whitespace-nowrap text-text-secondary">{label}</span>
            <div className={`h-7 ${bar}`} />
          </div>
        ))}
      </section>
      <div className="flex flex-col gap-px pt-2" aria-hidden="true">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className="flex h-11 items-center gap-3 px-3">
            <div className={`size-5 shrink-0 rounded-full ${bar}`} />
            <div className={`h-3.5 ${bar}`} style={{ width: `${34 - i * 6}%` }} />
          </div>
        ))}
      </div>
      <span className="sr-only" role="status">
        Scanning installed skills…
      </span>
    </PageShell>
  );
}

/**
 * Home: what needs the user (HomeOverview), then the grouped list of their
 * skills that each to-do filters.
 */
export function HomeView({ snapshot, isLoading, onSelectSkill, active }: HomeViewProps) {
  const setActiveView = useAppStore((state) => state.setActiveView);
  const inboxRef = useRef<HTMLElement>(null);
  const updateCheck = useUpdateCheck(snapshot?.update_check ?? NO_UPDATE_CHECK);
  const replaceSkillListFilter = useAppStore((state) => state.replaceSkillListFilter);

  const [linkedRootDialog, setLinkedRootDialog] = useState<{
    target: LifecycleTarget;
    harness: string;
    harnessLabel: string;
    root: string;
  } | null>(null);

  // Every Hook below must run on every render (Home's skeleton/empty states return early, further
  // down, only after they've all been called), so the derived data they depend on falls back to
  // empty rather than gating on `snapshot` here.
  const groups = computeHomeGroups(snapshot);
  const { broken, warnings, updates } = groups;

  const { filter, setFilter, setCollapsedGroups, isGroupVisible, isGroupExpanded, toggleGroup } =
    useHomeGroupVisibility();

  // An acted-on row keeps its place until the user changes what the list shows; see `rowActs`.
  const resetKey = String(filter);
  // A row parked here leaves `groups.unused`; its held row takes the parked skill's data from
  // `groups.own`, so the row selects the copy at its new path.
  const heldSkills = useHeldLayout(
    // With no readable history, "unused" would only mean "not seen", so the group stays empty.
    { updates, unused: snapshot?.invocations.length ? groups.unused : [] },
    (skill) => skill.name,
    (skill) => skill.name,
    resetKey,
    (name) => groups.own.find((skill) => skill.parked && skill.name === name),
  );
  const heldIssues = useHeldLayout(
    { broken, warnings },
    (issue) => `${issue.kind}:${issue.skill.name}:${issue.detail}`,
    (issue) => issue.skill.name,
    resetKey,
  );
  const shown: HomeInboxLists = { ...heldSkills, ...heldIssues, recent: groups.recent };

  const goToSkills = (patch: Parameters<typeof replaceSkillListFilter>[0]) => {
    replaceSkillListFilter(patch);
    setActiveView({ kind: "skills" });
  };

  // Row semantics match Skills: one continuous `aria-rowindex` across every visible group, and
  // `tabIndex={0}` on only the very first rendered row.
  const { starts, visibleKeys, openByKey, skillByKey } = buildHomeRowPlan({
    groups: shown,
    isGroupVisible,
    isGroupExpanded,
    onSelectSkill,
  });

  // Selection is per skill, as on Skills: a skill in two groups shows checked in both.
  const selectedPaths = useAppStore((state) => state.selectedSkillPaths);
  const toggleSkillSelection = useAppStore((state) => state.toggleSkillSelection);
  const clearSkillSelection = useAppStore((state) => state.clearSkillSelection);
  const rowPath = (skill: InstalledSkill) => skill.deployments[0]?.path;
  const toggleSkill = (skill: InstalledSkill) => {
    const path = rowPath(skill);
    if (path) toggleSkillSelection(path);
  };
  const isChecked = (skill: InstalledSkill) => selectedPaths.has(rowPath(skill) ?? "");
  const selectedSkills = groups.own.filter(isChecked);
  const { handleAct, dialog: updateDialog } = useSkillListAct(onSelectSkill, undefined);

  const { rowRef, containerRef, tabIndexFor, onGridKeyDown, followRow, statusText } =
    useHomeRowCursor({
      visibleKeys,
      openByKey,
      onToggle: (key) => {
        const skill = skillByKey.get(key);
        if (skill) toggleSkill(skill);
      },
      onEscape: () => {
        if (selectedPaths.size > 0) clearSkillSelection();
      },
      active,
      setCollapsedGroups,
    });

  if (!snapshot) {
    if (isLoading) {
      return <HomeSkeleton />;
    }
    return (
      <PageShell title="Home">
        <p className="flex h-full items-center justify-center text-wrap-pretty text-text-tertiary">
          No skill snapshot yet.
        </p>
      </PageShell>
    );
  }

  const inbox = (
    <section ref={inboxRef} aria-labelledby="home-items-title" className="scroll-mt-4">
      <div className="mb-3 flex min-h-7 items-center justify-between gap-3">
        <h2 id="home-items-title" className="text-heading font-semibold">
          Your skills
        </h2>
        <span className="flex items-center gap-2 text-small text-text-secondary">
          <span className={updateCheck.failed ? "text-error" : undefined}>
            {updateCheck.caption}
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="-mr-2.5"
            disabled={updateCheck.pending}
            onClick={updateCheck.check}
          >
            <RefreshCw
              aria-hidden
              className={updateCheck.pending ? "motion-safe:animate-spin" : undefined}
            />
            {updateCheck.pending ? "Checking…" : "Check for updates"}
          </Button>
        </span>
        {updateCheck.checkError ? (
          <span role="alert" className="sr-only">
            {updateCheck.checkError}
          </span>
        ) : (
          <span role="status" className="sr-only">
            {updateCheck.caption}
          </span>
        )}
      </div>
      <div className="flex flex-col gap-3">
        <HomeInboxGrid
          groups={shown}
          live={groups}
          starts={starts}
          filter={filter}
          onClearFilter={() => setFilter(null)}
          isGroupVisible={isGroupVisible}
          isGroupExpanded={isGroupExpanded}
          toggleGroup={toggleGroup}
          onSelectSkill={onSelectSkill}
          onShowAllIssues={(attention) => goToSkills({ attention })}
          onShowAllUpdates={() => goToSkills({ attention: "update" })}
          onShowAllUnused={() => goToSkills({ usage: "unused-30d" })}
          onShowAllRecent={() => setActiveView({ kind: "activity" })}
          onConvertLinkedRoot={(skill, harness, harnessLabel, root) =>
            setLinkedRootDialog({
              target: lifecycleTargetForHarnessRoot(skill, harness, root),
              harness,
              harnessLabel,
              root,
            })
          }
          containerRef={containerRef}
          onGridKeyDown={onGridKeyDown}
          followRow={followRow}
          wiring={{
            rowRef,
            tabIndexFor,
            isChecked,
            hasSelection: selectedSkills.length > 0,
            onToggle: toggleSkill,
            act: (label, skill) => void handleAct(label, skill),
          }}
        />
      </div>
    </section>
  );

  return (
    <PageShell title="Home">
      {/* Sections sit 32px apart and keep 12px inside, so the gaps say what belongs together. */}
      <div className="flex flex-col gap-8">
        {/* PROTOTYPE home-library: dev-only picker over four library views; delete with
            src/prototype/home-library. */}
        {import.meta.env.DEV ? (
          <HomeLibraryPrototype
            snapshot={snapshot}
            groups={groups}
            active={active}
            open={onSelectSkill}
            updateCheck={updateCheck}
            inbox={inbox}
            baseline={
              <HomeOverview
                snapshot={snapshot}
                groups={groups}
                active={active}
                updateCaption={updateCheck.caption}
                open={onSelectSkill}
                showFilter={(next) => {
                  setFilter(next);
                  inboxRef.current?.scrollIntoView({ block: "start", behavior: "smooth" });
                }}
              />
            }
          />
        ) : (
          <>
            <HomeOverview
              snapshot={snapshot}
              groups={groups}
              active={active}
              updateCaption={updateCheck.caption}
              open={onSelectSkill}
              showFilter={(next) => {
                setFilter(next);
                inboxRef.current?.scrollIntoView({ block: "start", behavior: "smooth" });
              }}
            />
            {inbox}
          </>
        )}
        {/* /PROTOTYPE home-library */}
      </div>
      {/* Visually-hidden live region: announces the cursor's position, debounced to the last move. */}
      <div role="status" aria-live="polite" className="sr-only">
        {statusText}
      </div>

      {selectedSkills.length > 0 && (
        <SkillListSelectionBar
          selectedSkills={selectedSkills}
          packProjectPath={null}
          onCancel={clearSkillSelection}
          onAct={(label, skill) => void handleAct(label, skill)}
          onOpen={(skill) => onSelectSkill(skill.name)}
          onActionFinished={(hadFailures) => {
            // A failure keeps the selection so the user can retry.
            if (!hadFailures) clearSkillSelection();
          }}
        />
      )}
      {updateDialog}

      {linkedRootDialog && (
        <MaterializeRootDialog
          target={linkedRootDialog.target}
          harness={linkedRootDialog.harness}
          harnessLabel={linkedRootDialog.harnessLabel}
          root={linkedRootDialog.root}
          intent={{ kind: "convert-only" }}
          onClose={() => setLinkedRootDialog(null)}
        />
      )}
    </PageShell>
  );
}

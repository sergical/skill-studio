// ============================================================================
// Sidebar - Left-hand navigation: places only (Home, Skills, Activity,
// Parked). Filters (scope, harness, source, issue) live in the Skills view's
// filter bar instead - see the design rule in spec-ux-1.md section B.
// ============================================================================

import { useEffect, useState } from "react";
import {
  Activity as ActivityIcon,
  AlertTriangle,
  BookOpen,
  Boxes,
  Layers,
  LayoutDashboard,
  ListTree,
  Moon,
  PackageOpen,
  Plus,
  Puzzle,
  RefreshCw,
  Search,
  Settings as SettingsIcon,
  Sun,
} from "lucide-react";
import { Button, Popover, PopoverContent, PopoverTrigger } from "@skill-studio/ui";
import { ownSkillsView, pluginSkillsView } from "@skill-studio/lib";
import { SHORTCUTS } from "../../lib/app-shortcuts";
import {
  backgroundRefreshError,
  hasNewerSkillSnapshotEmission,
  rescanTooltip,
  sidebarAnchorView,
} from "../../lib/sidebar-nav";
import { useAppStore } from "../../store/appStore";
import type { ActiveView } from "../../store/appStore";
import { TooltipControl } from "../ui/TooltipControl";
import type { ResolvedTheme, Theme } from "../../lib/theme";
import type { BackgroundStatus, SkillListFilter, SkillSnapshot } from "@skill-studio/lib";

interface SidebarProps {
  snapshot: SkillSnapshot | undefined;
  emittedSnapshotRevision: number | undefined;
  backgroundStatus?: BackgroundStatus;
  requestRescan: () => Promise<void>;
}

const itemClass = (active: boolean) =>
  `grid h-6.5 w-full grid-cols-[14px_minmax(0,1fr)_auto] items-center gap-2 rounded-sm px-2 text-left text-body ${active ? "bg-bg-active text-text-primary" : "text-text-secondary hover:bg-bg-hover hover:text-text-primary"} active:bg-bg-pressed`;
const iconButtonClass =
  "rounded-sm text-text-tertiary hover:text-text-primary active:bg-bg-pressed";

/**
 * Tracks one rescan at a time: `spinning` is fully derived from the pending
 * revision and the store's latest emission, so once a newer snapshot lands
 * this reads `false` on its own, with no effect needed to clear it back to
 * null.
 */
function useSidebarRescan(
  snapshotRevision: number | undefined,
  emittedSnapshotRevision: number | undefined,
  requestRescan: () => Promise<void>,
) {
  const [pendingRescanSnapshotRevision, setPendingRescanSnapshotRevision] = useState<number | null>(
    null,
  );
  const spinning =
    pendingRescanSnapshotRevision !== null &&
    !hasNewerSkillSnapshotEmission(pendingRescanSnapshotRevision, emittedSnapshotRevision);

  const handleRefresh = async () => {
    if (spinning) return;
    // A sync asks for the lists as they are now, so rows held after an act take their new place.
    useAppStore.getState().clearRowActs();
    setPendingRescanSnapshotRevision(snapshotRevision ?? 0);
    try {
      await requestRescan();
    } catch {
      setPendingRescanSnapshotRevision(null);
    }
  };

  return { spinning, handleRefresh };
}

interface SidebarNavItemsProps {
  anchorView: ActiveView;
  skillsActive: boolean;
  skillsCount: number;
  pluginCount: number;
  inParked: boolean;
  setActiveView: (view: ActiveView) => void;
  replaceSkillListFilter: (patch: Partial<SkillListFilter>) => void;
}

/** The places: Home, Skills (with an optional Plugins place beside it), Packs, Activity. */
function SidebarNavItems({
  anchorView,
  skillsActive,
  skillsCount,
  pluginCount,
  inParked,
  setActiveView,
  replaceSkillListFilter,
}: SidebarNavItemsProps) {
  return (
    <div className="flex flex-col gap-px pt-2.5">
      <Button
        variant="ghost"
        className={itemClass(anchorView.kind === "home")}
        aria-current={anchorView.kind === "home" ? "page" : undefined}
        onClick={() => setActiveView({ kind: "home" })}
      >
        <LayoutDashboard size={14} />
        <span className="min-w-0 truncate">Home</span>
      </Button>
      <Button
        variant="ghost"
        className={itemClass(skillsActive)}
        aria-current={skillsActive ? "page" : undefined}
        onClick={() => {
          if (inParked) replaceSkillListFilter({});
          setActiveView({ kind: "skills" });
        }}
      >
        <Layers size={14} />
        <span className="min-w-0 truncate">Skills</span>
        {skillsCount > 0 && (
          <span className="text-right text-caption tabular-nums text-text-tertiary">
            {skillsCount}
          </span>
        )}
      </Button>
      {pluginCount > 0 && (
        <Button
          variant="ghost"
          className={itemClass(anchorView.kind === "plugins")}
          aria-current={anchorView.kind === "plugins" ? "page" : undefined}
          onClick={() => setActiveView({ kind: "plugins" })}
        >
          <Puzzle size={14} />
          <span className="min-w-0 truncate">Plugins</span>
          <span className="text-right text-caption tabular-nums text-text-tertiary">
            {pluginCount}
          </span>
        </Button>
      )}
      <Button
        variant="ghost"
        className={itemClass(anchorView.kind === "packs")}
        aria-current={anchorView.kind === "packs" ? "page" : undefined}
        onClick={() => setActiveView({ kind: "packs" })}
      >
        <Boxes size={14} />
        <span className="min-w-0 truncate">Packs</span>
      </Button>
      <Button
        variant="ghost"
        className={itemClass(anchorView.kind === "listing")}
        aria-current={anchorView.kind === "listing" ? "page" : undefined}
        onClick={() => setActiveView({ kind: "listing" })}
      >
        <ListTree size={14} />
        <span className="min-w-0 truncate">Model listing</span>
      </Button>
      <Button
        variant="ghost"
        className={itemClass(anchorView.kind === "activity")}
        aria-current={anchorView.kind === "activity" ? "page" : undefined}
        onClick={() => setActiveView({ kind: "activity" })}
      >
        <ActivityIcon size={14} />
        <span className="min-w-0 truncate">Activity</span>
      </Button>
    </div>
  );
}

interface SidebarParkedSectionProps {
  anchorView: ActiveView;
  inParked: boolean;
  parkedCount: number;
  setActiveView: (view: ActiveView) => void;
  replaceSkillListFilter: (patch: Partial<SkillListFilter>) => void;
}

/** Parked is a sub-section of the skills list, shown only when non-empty. */
function SidebarParkedSection({
  anchorView,
  inParked,
  parkedCount,
  setActiveView,
  replaceSkillListFilter,
}: SidebarParkedSectionProps) {
  if (parkedCount === 0) return null;
  const active = anchorView.kind === "skills" && inParked;
  return (
    <div className="flex flex-col gap-px pt-3">
      <Button
        variant="ghost"
        className={itemClass(active)}
        aria-current={active ? "page" : undefined}
        onClick={() => {
          replaceSkillListFilter({ scope: "parked" });
          setActiveView({ kind: "skills" });
        }}
      >
        <PackageOpen size={14} />
        <span className="min-w-0 truncate">Parked</span>
        <span className="text-right text-caption tabular-nums text-text-tertiary">
          {parkedCount}
        </span>
      </Button>
    </div>
  );
}

interface SidebarFooterProps {
  anchorView: ActiveView;
  scannedAt: string | undefined;
  spinning: boolean;
  /** The last background refresh's error; the Sync button turns into a warning that shows it. */
  refreshError: string | null;
  onRefresh: () => void;
  setActiveView: (view: ActiveView) => void;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: Theme) => void;
}

interface SyncButtonProps {
  scannedAt: string | undefined;
  spinning: boolean;
  refreshError: string | null;
  onRefresh: () => void;
}

/**
 * Manual rescan. Background refreshes never show here, so the app stays still while agents write
 * transcripts. Only a failed refresh does: the icon turns into a warning that opens the error.
 * One button serves both states, so focus stays on it when a retry starts or the error clears.
 */
function SyncButton({ scannedAt, spinning, refreshError, onRefresh }: SyncButtonProps) {
  const showError = refreshError !== null && !spinning;
  const [isErrorOpen, setIsErrorOpen] = useState(false);
  // An open error closes with the error, so the next failure never opens it without a click.
  if (isErrorOpen && !showError) setIsErrorOpen(false);

  return (
    <Popover
      open={isErrorOpen}
      onOpenChange={(open) => {
        if (!showError) {
          if (open) onRefresh();
          return;
        }
        setIsErrorOpen(open);
      }}
    >
      <TooltipControl content={showError ? "Show why sync failed" : rescanTooltip(scannedAt)}>
        <PopoverTrigger
          render={
            <Button
              variant="ghost"
              size="xs"
              className="shrink-0 gap-1.5 rounded-sm px-1.5 text-text-tertiary"
              aria-disabled={spinning}
              // Without an error the click only starts a sync, so it opens no dialog.
              aria-haspopup={showError ? "dialog" : undefined}
              aria-expanded={showError ? isErrorOpen : undefined}
              aria-label={
                showError
                  ? "Sync failed, show error"
                  : spinning
                    ? "Syncing installed skills"
                    : "Sync installed skills"
              }
            />
          }
        >
          {showError ? (
            <AlertTriangle size={13} className="text-warning" />
          ) : (
            <RefreshCw size={13} className={spinning ? "animate-spin" : ""} />
          )}
          <span className="whitespace-nowrap">Sync</span>
        </PopoverTrigger>
      </TooltipControl>
      <PopoverContent
        side="top"
        align="start"
        aria-label="Sync error"
        className="w-72 gap-2 bg-bg-elevated text-small text-text-primary ring-border"
      >
        <p className="m-0 font-medium">Couldn't refresh your skills</p>
        {/* A long error scrolls; the region takes focus so the keyboard can scroll it too. */}
        <div
          role="region"
          aria-label="Error details"
          tabIndex={0}
          className="max-h-40 select-text overflow-y-auto break-words rounded-sm text-text-secondary focus-visible:outline-2 focus-visible:outline-accent"
        >
          {refreshError}
        </div>
        <p className="m-0 text-text-tertiary">The lists show the last results that loaded.</p>
        <Button
          size="sm"
          variant="outline"
          className="self-start"
          onClick={() => {
            setIsErrorOpen(false);
            onRefresh();
          }}
        >
          Try again
        </Button>
      </PopoverContent>
    </Popover>
  );
}

/** The snapshot's age and a manual rescan button, plus Learn, Settings, and the theme toggle. */
function SidebarFooter({
  anchorView,
  scannedAt,
  spinning,
  refreshError,
  onRefresh,
  setActiveView,
  resolvedTheme,
  setTheme,
}: SidebarFooterProps) {
  return (
    <div className="mt-auto flex select-none items-center justify-between gap-2 px-2 py-1.5">
      <SyncButton
        scannedAt={scannedAt}
        spinning={spinning}
        refreshError={refreshError}
        onRefresh={onRefresh}
      />
      <div className="flex items-center gap-0.5">
        <TooltipControl content="Learn">
          <Button
            variant="ghost"
            size="icon-xs"
            className={`${iconButtonClass} aria-[current=page]:bg-accent-softer aria-[current=page]:text-accent`}
            onClick={() => setActiveView({ kind: "learn" })}
            aria-current={anchorView.kind === "learn" ? "page" : undefined}
            aria-label="Learn"
          >
            <BookOpen size={13} />
          </Button>
        </TooltipControl>
        <TooltipControl content="Settings" shortcut={SHORTCUTS.settings.keys}>
          <Button
            variant="ghost"
            size="icon-xs"
            className={`${iconButtonClass} aria-[current=page]:bg-accent-softer aria-[current=page]:text-accent`}
            onClick={() => setActiveView({ kind: "settings" })}
            aria-current={anchorView.kind === "settings" ? "page" : undefined}
            aria-label="Settings"
          >
            <SettingsIcon size={13} />
          </Button>
        </TooltipControl>
        <TooltipControl
          content={resolvedTheme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
        >
          <Button
            variant="ghost"
            size="icon-xs"
            className={iconButtonClass}
            onClick={() => setTheme(resolvedTheme === "dark" ? "light" : "dark")}
            aria-label={resolvedTheme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          >
            {resolvedTheme === "dark" ? <Sun size={13} /> : <Moon size={13} />}
          </Button>
        </TooltipControl>
      </div>
    </div>
  );
}

/**
 * Left-hand navigation: a search box that jumps into Skills with a query,
 * Add skill, the three places (Home, Skills, Activity), Parked (when
 * non-empty), and a footer with the snapshot's age and a manual rescan
 * button.
 */
export function Sidebar({
  snapshot,
  emittedSnapshotRevision,
  backgroundStatus,
  requestRescan,
}: SidebarProps) {
  // Forces the footer to re-render so "just now" ages into "1m ago" and
  // beyond without waiting for the next snapshot - relativeScanTime() itself
  // stays a pure function of scannedAt and the current clock.
  const [, forceTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => forceTick((n) => n + 1), 30_000);
    return () => clearInterval(id);
  }, []);
  const activeView = useAppStore((state) => state.activeView);
  const anchorView = sidebarAnchorView(activeView);
  const setActiveView = useAppStore((state) => state.setActiveView);
  const skillListFilter = useAppStore((state) => state.skillListFilter);
  const replaceSkillListFilter = useAppStore((state) => state.replaceSkillListFilter);
  const openAddSkillSheet = useAppStore((state) => state.openAddSkillSheet);
  const resolvedTheme = useAppStore((state) => state.resolvedTheme);
  const setTheme = useAppStore((state) => state.setTheme);
  const requestSkillSearchFocus = useAppStore((state) => state.requestSkillSearchFocus);

  const own = ownSkillsView(snapshot?.skills ?? []);
  const parkedCount = own.filter((s) => s.parked).length;
  const skillsCount = own.length - parkedCount;
  const pluginCount = pluginSkillsView(snapshot?.skills ?? []).length;
  // Parked is a sub-section of the skills list, so the Skills row is only
  // "current" when that partition isn't selected. Plugin skills are their
  // own place (see PluginSkillsView), not a filter on Skills.
  const inParked = skillListFilter.scope === "parked";
  const skillsActive = anchorView.kind === "skills" && !inParked;

  function goToSearch() {
    if (anchorView.kind !== "skills") setActiveView({ kind: "skills" });
    requestSkillSearchFocus();
  }

  const { spinning, handleRefresh } = useSidebarRescan(
    snapshot?.revision,
    emittedSnapshotRevision,
    requestRescan,
  );

  return (
    <nav className="flex w-60 shrink-0 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-9 shrink-0" />
      {/* 41px = the page panel's 1px top border plus PageShell's 40px header, so this title sits on
          the same line as the page title. */}
      <div className="flex h-[41px] shrink-0 items-center justify-between pr-1.5 pl-3.5">
        <span className="text-small font-semibold text-text-primary">Skill Studio</span>
        <div className="flex items-center gap-0.5">
          {/* The sidebar switcher row only fits two icons without crowding, so the command
              palette's ⌘K hint rides along on this tooltip instead of a third icon button. */}
          <TooltipControl
            content={["Search skills", "Command palette ⌘K"]}
            shortcut={SHORTCUTS.filterSkills.keys}
          >
            <Button
              variant="ghost"
              size="icon-xs"
              className={iconButtonClass}
              aria-label="Search skills"
              onClick={goToSearch}
            >
              <Search size={14} />
            </Button>
          </TooltipControl>
          <TooltipControl content="Add skill" shortcut={SHORTCUTS.addSkill.keys}>
            <Button
              variant="ghost"
              size="icon-xs"
              className={iconButtonClass}
              aria-label="Add skill"
              onClick={() => openAddSkillSheet()}
            >
              <Plus size={14} />
            </Button>
          </TooltipControl>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-y-scroll pl-2 gutter-pr-2">
        <SidebarNavItems
          anchorView={anchorView}
          skillsActive={skillsActive}
          skillsCount={skillsCount}
          pluginCount={pluginCount}
          inParked={inParked}
          setActiveView={setActiveView}
          replaceSkillListFilter={replaceSkillListFilter}
        />
        <SidebarParkedSection
          anchorView={anchorView}
          inParked={inParked}
          parkedCount={parkedCount}
          setActiveView={setActiveView}
          replaceSkillListFilter={replaceSkillListFilter}
        />
      </div>

      <SidebarFooter
        anchorView={anchorView}
        scannedAt={snapshot?.scanned_at}
        spinning={spinning}
        refreshError={backgroundRefreshError(backgroundStatus)}
        onRefresh={handleRefresh}
        setActiveView={setActiveView}
        resolvedTheme={resolvedTheme}
        setTheme={setTheme}
      />
    </nav>
  );
}

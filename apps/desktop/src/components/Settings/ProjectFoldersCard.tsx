// ============================================================================
// ProjectFoldersCard - Settings' "Project folders" section: every folder
// discovery found or the user added by hand, and the actions to stop
// tracking or remove a folder. The per-agent search switches that decide
// what discovery looks at live in the Agents table; `useDiscoverySources`
// holds their state for both. Shares its add/stop-tracking/remove logic
// with the Skills filter bar through `useProjectFolderActions`, so both
// surfaces agree on one set of tracked folders.
// ============================================================================

import { useRef, useState } from "react";
import type { RefObject } from "react";
import { AlertTriangle, Asterisk, ChevronDown, FolderOpen, Plus } from "lucide-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  buttonVariants,
  Collapsible,
  CollapsiblePanel,
  CollapsibleTrigger,
} from "@skill-studio/ui";
import {
  deploymentLabelFromAgentId,
  homeRelativePath,
  type DiscoverySourceSetting,
  type ProjectFolder,
  type SkillSnapshot,
} from "@skill-studio/lib";
import { invokeErrorMessage } from "../../lib/skill-command-error";
import { useAppStore } from "../../store/appStore";
import { useClientIds } from "../../hooks/useClientRows";
import { useLoad } from "../../hooks/useLoad";
import { useProjectFolderActions } from "../../hooks/useProjectFolderActions";
import { MenuControl, MenuItem } from "../ui/MenuControl";
import { ProjectFolderAddForm } from "./ProjectFolderAddForm";
import { PageSection } from "../Shell/PageSection";

export interface DiscoverySources {
  /** Whether each agent's history is searched for project folders; `null` until read. */
  sources: DiscoverySourceSetting[] | null;
  error: string | null;
  /** Counts saved changes, so the folder list reads again after each one. */
  saved: number;
  toggle: (agent: string, enabled: boolean) => void;
}

/** The discovery switches, read once and shared by the Agents table and the folder list. */
export function useDiscoverySources(): DiscoverySources {
  const addToast = useAppStore((state) => state.addToast);
  const getDiscoverySources = useAppStore((state) => state.getDiscoverySources);
  const setDiscoverySource = useAppStore((state) => state.setDiscoverySource);
  const load = useLoad("discovery-sources", getDiscoverySources, {
    onFailed: (message) =>
      addToast({ type: "error", title: "Couldn't read discovery settings", message }),
  });
  const [saved, setSaved] = useState(0);
  const sources = load.data ?? null;

  const toggle = (agent: string, enabled: boolean) => {
    if (!sources) return;
    const previous = sources;
    load.setData(sources.map((s) => (s.harness === agent ? { ...s, enabled } : s)));
    setDiscoverySource(agent, enabled)
      .then((updated) => {
        load.setData(updated);
        setSaved((count) => count + 1);
        // Activity can't keep filtering by an agent that is now off, and must not go back to it
        // by itself when the agent is turned on again.
        const { activityFilter, setActivityFilter } = useAppStore.getState();
        if (!enabled && activityFilter.harness === agent)
          setActivityFilter({ harness: "all", trigger: activityFilter.trigger });
      })
      .catch((err) => {
        load.setData(previous);
        addToast({
          type: "error",
          title: "Couldn't change discovery setting",
          message: invokeErrorMessage(err),
        });
      });
  };

  return {
    sources,
    error: load.error !== null ? "Couldn't read discovery settings." : null,
    saved,
    toggle,
  };
}

interface ProjectFoldersCardProps {
  snapshot: SkillSnapshot | undefined;
  discovery: DiscoverySources;
}

/** "A", "A and B", "A, B and C" - the empty state's list of searched harnesses. */
function joinWithAnd(items: string[]): string {
  if (items.length === 0) return "";
  if (items.length === 1) return items[0];
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/** One skill per project path its deployments touch, counted once even when a skill deploys to
 * the same project through more than one harness - matches `SkillsView.tsx`'s own scope match. */
function countSkillsByProject(snapshot: SkillSnapshot | undefined): Map<string, number> {
  const counts = new Map<string, number>();
  for (const skill of snapshot?.skills ?? []) {
    const projectPaths = new Set(
      skill.deployments.map((d) => d.project_path).filter((path): path is string => Boolean(path)),
    );
    for (const path of projectPaths) {
      counts.set(path, (counts.get(path) ?? 0) + 1);
    }
  }
  return counts;
}

/** The row's secondary line: a missing-folder warning, a pattern's match count, or a plain
 * folder's discovered/added label and skill count. Split out of `FolderRow` to keep it a single
 * flat branch instead of nested ternaries. */
function FolderSecondaryText({
  folder,
  isDiscovered,
  isPattern,
  skillCount,
}: {
  folder: ProjectFolder;
  isDiscovered: boolean;
  isPattern: boolean;
  skillCount: number;
}) {
  if (folder.missing) {
    return (
      <>
        <AlertTriangle size={12} className="text-warning" aria-hidden="true" />
        Folder not found · Added by you
      </>
    );
  }
  if (isPattern) {
    return (
      <>
        Added by you
        {" · "}
        <span className="tabular-nums">
          {folder.matches === 0
            ? "No matching folders"
            : `${folder.matches} ${folder.matches === 1 ? "folder" : "folders"}`}
        </span>
      </>
    );
  }
  return (
    <>
      {isDiscovered ? "Found in agent history" : "Added by you"}
      {" · "}
      <span className="tabular-nums">
        {skillCount} {skillCount === 1 ? "skill" : "skills"}
      </span>
    </>
  );
}

function FolderRow({
  folder,
  skillCount,
  onStopTracking,
  onRemove,
}: {
  folder: ProjectFolder;
  skillCount: number;
  onStopTracking: (path: string) => void;
  onRemove: (path: string) => void;
}) {
  const displayPath = homeRelativePath(folder.path);
  const isDiscovered = folder.source === "discovered";
  const isPattern = folder.matches != null;

  return (
    <li className="flex min-w-0 items-center gap-3 px-3 py-2">
      <div className="min-w-0 flex-1">
        <p
          className={`m-0 truncate text-body text-text-primary ${isPattern ? "font-mono" : ""}`}
          title={folder.path}
        >
          {displayPath}
        </p>
        <p className="m-0 flex items-center gap-1 overflow-hidden whitespace-nowrap text-small text-text-tertiary">
          <FolderSecondaryText
            folder={folder}
            isDiscovered={isDiscovered}
            isPattern={isPattern}
            skillCount={skillCount}
          />
        </p>
      </div>
      <Button
        variant="outline"
        size="sm"
        className="shrink-0"
        aria-label={`${isDiscovered ? "Stop tracking" : "Remove"} ${displayPath}`}
        onClick={() => (isDiscovered ? onStopTracking(folder.path) : onRemove(folder.path))}
      >
        {isDiscovered ? "Stop tracking" : "Remove"}
      </Button>
    </li>
  );
}

/** The folder list, closed behind a one-line summary until opened. */
function FolderList({
  folders,
  sources,
  loadError,
  skillCounts,
  open,
  onOpenChange,
  onStopTracking,
  onRemove,
  addFormOpen,
  onAddTypedPath,
  onCloseAddForm,
  triggerRef,
}: {
  folders: ProjectFolder[] | null;
  sources: DiscoverySourceSetting[] | null;
  loadError: string | null;
  skillCounts: Map<string, number>;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onStopTracking: (path: string) => void;
  onRemove: (path: string) => void;
  addFormOpen: boolean;
  onAddTypedPath: (value: string) => Promise<string | null>;
  onCloseAddForm: () => void;
  /** Focused after the add form closes, so focus doesn't drop to the body. */
  triggerRef: RefObject<HTMLButtonElement | null>;
}) {
  if (folders === null || sources === null) {
    return (
      <p className="m-0 rounded-md border border-border-subtle px-3 py-2 text-small text-text-tertiary">
        {loadError ?? "Looking for project folders…"}
      </p>
    );
  }

  if (folders.length === 0 && !addFormOpen) {
    const enabledLabels: string[] = [];
    for (const source of sources) {
      if (source.enabled) enabledLabels.push(deploymentLabelFromAgentId(source.harness));
    }
    return (
      <p className="m-0 rounded-md border border-border-subtle px-3 py-2 text-small text-text-tertiary">
        {enabledLabels.length === 0
          ? "History search is off for every agent. Add a folder by hand."
          : `No folders with skills in the history of ${joinWithAnd(enabledLabels)}. Add one by hand.`}
      </p>
    );
  }

  const missingCount = folders.filter((folder) => folder.missing).length;
  // A plain folder counts as one; a pattern counts as however many of its matched folders made
  // the resolved set, so this line reads like "N folders" even though one pattern is one row.
  const folderCount = folders.reduce(
    (total, folder) => (folder.missing ? total : total + (folder.matches ?? 1)),
    0,
  );
  return (
    // No overflow-hidden here: the global :focus-visible outline sits 2px outside the trigger and
    // would be clipped.
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      className="rounded-md border border-border-subtle"
    >
      <CollapsibleTrigger
        ref={triggerRef}
        className="group/folders flex h-9 w-full items-center gap-2 rounded-md px-3 text-left text-small text-text-secondary transition-colors hover:bg-bg-hover data-panel-open:rounded-b-none"
      >
        <ChevronDown
          aria-hidden
          className="size-3.5 shrink-0 -rotate-90 text-text-tertiary transition-transform motion-reduce:transition-none group-data-panel-open/folders:rotate-0"
        />
        <span className="font-medium tabular-nums">
          {folderCount} {folderCount === 1 ? "folder" : "folders"}
        </span>
        {missingCount > 0 && (
          <span className="flex items-center gap-1 text-text-tertiary">
            <span aria-hidden="true">·</span>
            <AlertTriangle size={12} className="text-warning" aria-hidden="true" />
            <span className="tabular-nums">{missingCount}</span> not found
          </span>
        )}
      </CollapsibleTrigger>
      <CollapsiblePanel>
        {addFormOpen && <ProjectFolderAddForm onAdd={onAddTypedPath} onClose={onCloseAddForm} />}
        <ul className="m-0 list-none border-t border-border-subtle p-0">
          {folders.map((folder) => (
            <FolderRow
              key={folder.path}
              folder={folder}
              skillCount={skillCounts.get(folder.path) ?? 0}
              onStopTracking={onStopTracking}
              onRemove={onRemove}
            />
          ))}
        </ul>
      </CollapsiblePanel>
    </Collapsible>
  );
}

export function ProjectFoldersCard({ snapshot, discovery }: ProjectFoldersCardProps) {
  const addToast = useAppStore((state) => state.addToast);
  const listProjectFolders = useAppStore((state) => state.listProjectFolders);
  const { addProject, addTypedPath, stopTracking, removeProject } = useProjectFolderActions();
  const [pendingRemoval, setPendingRemoval] = useState<{
    path: string;
    discovered: boolean;
  } | null>(null);
  const { sources } = discovery;
  // History is read only from agents whose skills are shown (see `activity_sources` in
  // skill_refresh.rs), so the empty state names only those.
  const agents = useClientIds();
  const agentSet = agents && new Set(agents);
  const shownSources =
    sources && agentSet ? sources.filter((s) => agentSet.has(s.harness)) : sources;
  const [foldersOpen, setFoldersOpen] = useState(false);
  const [addFormOpen, setAddFormOpen] = useState(false);
  const foldersTriggerRef = useRef<HTMLButtonElement>(null);

  // A scan that adds or removes a tracked project, or a saved search switch, changes the key,
  // which lists the folders again.
  const folders = useLoad(
    `folders:${discovery.saved}:${(snapshot?.projects ?? []).join("\n")}`,
    listProjectFolders,
    {
      keepPreviousData: true,
      onFailed: (message) =>
        addToast({ type: "error", title: "Couldn't list project folders", message }),
    },
  );
  const refetchFolders = folders.reload;
  const loadError = folders.error !== null ? "Couldn't list project folders." : null;

  const skillCounts = countSkillsByProject(snapshot);

  const handleAddFolder = async () => {
    const added = await addProject();
    if (!added) return;
    setFoldersOpen(true);
    refetchFolders();
  };

  const handleStartTyping = () => {
    setFoldersOpen(true);
    setAddFormOpen(true);
  };

  // Cancel, Escape, and a successful Add all close the form through this one path, so focus
  // always lands back on the "N folders" trigger instead of dropping to the body.
  const closeAddForm = () => {
    setAddFormOpen(false);
    foldersTriggerRef.current?.focus();
  };

  const handleAddTypedPath = async (value: string): Promise<string | null> => {
    const error = await addTypedPath(value);
    if (!error) refetchFolders();
    return error;
  };

  const handleStopTracking = async (path: string) => {
    await stopTracking(path);
    refetchFolders();
  };

  const handleRemove = async (path: string) => {
    await removeProject(path);
    refetchFolders();
  };

  return (
    <PageSection
      title="Project folders"
      meta={
        <MenuControl
          triggerClassName={buttonVariants({ variant: "outline", size: "sm" })}
          triggerAriaLabel="Add folder"
          trigger={
            <>
              <Plus size={14} aria-hidden="true" />
              Add folder
              <ChevronDown size={12} aria-hidden="true" />
            </>
          }
        >
          <MenuItem closeOnClick onClick={handleAddFolder}>
            <FolderOpen size={14} aria-hidden="true" />
            Choose a folder…
          </MenuItem>
          <MenuItem closeOnClick onClick={handleStartTyping}>
            <Asterisk size={14} aria-hidden="true" />
            Type a path or pattern…
          </MenuItem>
        </MenuControl>
      }
    >
      <FolderList
        folders={folders.data ?? null}
        sources={shownSources}
        loadError={loadError ?? discovery.error}
        skillCounts={skillCounts}
        open={foldersOpen}
        onOpenChange={setFoldersOpen}
        onStopTracking={(path) => setPendingRemoval({ path, discovered: true })}
        onRemove={(path) => setPendingRemoval({ path, discovered: false })}
        addFormOpen={addFormOpen}
        onAddTypedPath={handleAddTypedPath}
        onCloseAddForm={closeAddForm}
        triggerRef={foldersTriggerRef}
      />
      <AlertDialog
        open={pendingRemoval !== null}
        onOpenChange={(open) => {
          if (!open) setPendingRemoval(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {pendingRemoval?.discovered ? "Stop tracking project?" : "Remove project folder?"}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {pendingRemoval?.discovered
                ? `Stop tracking ${pendingRemoval.path}? Its skills stay on disk.`
                : `Remove ${pendingRemoval?.path} from your added folders? This does not delete the folder or its skills.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pendingRemoval?.discovered) void handleStopTracking(pendingRemoval.path);
                else if (pendingRemoval) void handleRemove(pendingRemoval.path);
                setPendingRemoval(null);
              }}
            >
              {pendingRemoval?.discovered ? "Stop tracking" : "Remove"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </PageSection>
  );
}

// ============================================================================
// SkillStore - Main skill discovery and management view
// ============================================================================

import { useState, useEffect, useCallback, useRef } from "react";
import { Button, Drawer, Tabs, TabsContent, TabsList, TabsTrigger } from "@skill-studio/ui";
import { SkillSearchBar } from "./SkillSearchBar";
import { SkillBrowser } from "./SkillBrowser";
import { SkillDetailPanel } from "./SkillDetailPanel";
import { InstallProgressModal } from "./InstallProgressModal";
import { searchSkills, getInstalledSkills, getPopularSkills } from "../../lib/skill-api";
import { invokeErrorMessage } from "../../lib/skill-command-error";
import type {
  SkillSearchResult,
  InstalledSkill,
  SkillWithStatus,
  InstallProgressState,
} from "@skill-studio/lib";
import { useAppStore } from "../../store/appStore";

const LIMIT = 50;

function isSearchQuery(query: string): boolean {
  return query.trim() !== "" && query.length >= 2;
}

/**
 * Extract GitHub owner/repo from a URL or source string
 * e.g., "https://github.com/getsentry/skills" -> "getsentry/skills"
 * e.g., "getsentry/skills" -> "getsentry/skills"
 */
function extractGitHubRepo(
  source: string | undefined,
  sourceUrl: string | null | undefined,
): string | undefined {
  // Try source_url first (more reliable)
  if (sourceUrl) {
    const match = sourceUrl.match(/github\.com\/([^/]+\/[^/]+)/);
    if (match) {
      return match[1].replace(/\.git$/, "");
    }
  }
  // Fall back to source if it looks like owner/repo
  if (source && /^[a-zA-Z0-9_-]+\/[a-zA-Z0-9_.-]+$/.test(source)) {
    return source;
  }
  return undefined;
}

/** A matching name identifies an installed catalog result only within the same repository. */
function mergeWithInstalledStatus(
  results: SkillSearchResult[],
  installed: InstalledSkill[],
): SkillWithStatus[] {
  const byName = new Map(installed.map((skill) => [skill.name, skill]));
  return results.map((skill) => {
    const local = byName.get(skill.name);
    const source = local && extractGitHubRepo(local.source, local.source_url);
    const match =
      source && source.toLowerCase() === skill.top_source?.toLowerCase() ? local : undefined;
    return { ...skill, is_installed: !!match, installed_info: match };
  });
}

const TAB_TRIGGER_CLASS =
  "text-body font-medium text-text-tertiary after:bg-accent data-active:text-accent hover:text-text-secondary";

/** Tab strip (Browse/Installed): a bare `TabsList` - each tab's toolbar row and results live in that tab's own `TabsContent`, below. */
function SkillStoreTabList({
  installedCount,
  tabsPadding,
}: {
  installedCount: number | null;
  tabsPadding: string;
}) {
  return (
    <TabsList variant="line" className={`${tabsPadding} bg-bg-secondary`}>
      <TabsTrigger value="browse" className={TAB_TRIGGER_CLASS}>
        Browse
      </TabsTrigger>
      <TabsTrigger value="installed" className={TAB_TRIGGER_CLASS}>
        {installedCount === null ? "Installed" : `Installed (${installedCount})`}
      </TabsTrigger>
    </TabsList>
  );
}

function LoadError({
  title,
  error,
  onRetry,
}: {
  title: string;
  error: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex min-h-[280px] flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
      <h3 className="m-0 text-body font-semibold text-text-primary">{title}</h3>
      <p role="alert" className="m-0 max-w-sm text-small text-text-tertiary">
        {error}
      </p>
      <Button onClick={onRetry}>Try again</Button>
    </div>
  );
}

type InstalledInventory =
  | { kind: "loading" }
  | { kind: "ready"; skills: InstalledSkill[] }
  | { kind: "failed"; message: string };

function useInstalledInventory() {
  const projects = useAppStore((state) => state.userAddedProjects);
  const [inventory, setInventory] = useState<InstalledInventory>({ kind: "loading" });
  const latestRequest = useRef(0);

  // react-doctor-disable-next-line react-doctor/react-compiler-no-manual-memoization -- project changes and explicit retries use the same local scan
  const loadInstalledSkills = useCallback(() => {
    const request = ++latestRequest.current;
    setInventory((current) => (current.kind === "failed" ? { kind: "loading" } : current));
    return getInstalledSkills().then(
      (skills) => {
        if (latestRequest.current === request) setInventory({ kind: "ready", skills });
      },
      (error) => {
        if (latestRequest.current === request)
          setInventory({ kind: "failed", message: invokeErrorMessage(error) });
      },
    );
  }, []);

  useEffect(() => {
    // react-doctor-disable-next-line react-hooks-js/set-state-in-effect -- the scan reads the tracked projects through the Tauri host
    void loadInstalledSkills();
    // oxlint-disable-next-line react/exhaustive-effect-dependencies -- the host reads the tracked project list; rescan when it changes
  }, [loadInstalledSkills, projects]);

  return { inventory, loadInstalledSkills };
}

function useSkillStoreData(
  installedSkills: InstalledSkill[],
  addToast: ReturnType<typeof useAppStore.getState>["addToast"],
) {
  const [searchQuery, setSearchQuery] = useState("");
  // Raw API results, with no installed-status merged in - `searchResultsWithStatus`
  // below derives that at render time, so an `installedSkills` refresh (after
  // install/remove) can't leave this stale behind a second effect.
  const [rawResults, setRawResults] = useState<SkillSearchResult[]>([]);
  // Starts true: the mount effect below fetches immediately, so the first
  // render already reflects that instead of flipping it on synchronously
  // inside the effect (which the compiler flags as an avoidable extra render).
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  // Set when the results area itself couldn't be loaded (most commonly the
  // Skill Studio server not running) - replaces the results area with
  // `LoadError` instead of a toast, since there's nothing to show
  // behind it. Cleared at the start of every new attempt.
  const [browseError, setBrowseError] = useState<string | null>(null);
  // Never read during render - only `loadMore`'s paging math needs it, so a
  // ref avoids a re-render on every page bump.
  const pageRef = useRef(0);
  const browseRequest = useRef(0);
  const searchQueryRef = useRef("");

  // Every result merged with the current installed status - recomputed
  // whenever either input changes, so an install/remove refreshing
  // `installedSkills` shows up here with no second effect to keep in sync.
  const searchResultsWithStatus = mergeWithInstalledStatus(rawResults, installedSkills);

  // react-doctor-disable-next-line react-doctor/react-compiler-no-manual-memoization -- the mount effect loads the initial page once
  const handleSearch = useCallback((query: string) => {
    const request = ++browseRequest.current;
    searchQueryRef.current = query;
    setSearchQuery(query);
    pageRef.current = 0;
    setIsLoadingMore(false);
    setHasMore(false);
    setBrowseError(null);
    setIsLoading(true);

    const searching = isSearchQuery(query);
    const response = searching ? searchSkills(query, LIMIT) : getPopularSkills(0, LIMIT);
    return response
      .then((page) => {
        if (browseRequest.current !== request) return;
        setRawResults(page.skills);
        setHasMore(!searching && page.has_more);
      })
      .catch((error) => {
        if (browseRequest.current === request) setBrowseError(invokeErrorMessage(error));
      })
      .finally(() => {
        if (browseRequest.current === request) setIsLoading(false);
      });
  }, []);

  useEffect(() => {
    // react-doctor-disable-next-line react-hooks-js/set-state-in-effect -- starts the initial skills.sh request
    void handleSearch("");
  }, [handleSearch]);

  // Re-runs whichever fetch last populated the results area - the
  // `LoadError`'s "Try again" button.
  const retry = () => handleSearch(searchQuery);

  // A Promise chain, not a try/finally statement, so the compiler can still
  // optimize this component (it doesn't support `finally` clauses yet).
  const loadMore = () => {
    // hasMore is always false while a search is active (the v1 search API
    // returns everything up to LIMIT in one shot), so this only ever pages
    // through popular skills.
    if (isLoadingMore || !hasMore || isSearchQuery(searchQueryRef.current)) return;
    const request = browseRequest.current;

    setIsLoadingMore(true);
    const nextPage = pageRef.current + 1;

    return getPopularSkills(nextPage, LIMIT)
      .then((response) => {
        if (browseRequest.current !== request) return;
        setRawResults((prev) => [...prev, ...response.skills]);
        setHasMore(response.has_more);
        pageRef.current = nextPage;
      })
      .catch((err) => {
        if (browseRequest.current !== request) return;
        addToast({
          type: "error",
          title: "Load More Failed",
          message: err instanceof Error ? err.message : "Failed to load more skills",
        });
      })
      .finally(() => {
        if (browseRequest.current === request) setIsLoadingMore(false);
      });
  };

  return {
    searchQuery,
    setSearchQuery,
    isLoading,
    isLoadingMore,
    hasMore,
    browseError,
    retry,
    searchResultsWithStatus,
    handleSearch,
    loadMore,
  };
}

interface SkillStoreProps {
  /** Embedded inside `AddSkillSheet`'s "Browse skills.sh" tab: drops this
   * view's own header, since the sheet already has one. */
  compact?: boolean;
}

export function SkillStore({ compact = false }: SkillStoreProps = {}) {
  const [activeTab, setActiveTab] = useState<"browse" | "installed">("browse");
  const [selection, setSelection] = useState<string | null>(null);
  const [installProgress, setInstallProgress] = useState<InstallProgressState | null>(null);

  const addToast = useAppStore((state) => state.addToast);
  const { inventory, loadInstalledSkills } = useInstalledInventory();
  const installedSkills = inventory.kind === "ready" ? inventory.skills : [];
  const inventoryNotice =
    inventory.kind === "failed" ? (
      <LoadError
        title="Couldn’t load installed skills"
        error={inventory.message}
        onRetry={loadInstalledSkills}
      />
    ) : inventory.kind === "loading" ? (
      <p role="status" className="m-auto p-5 text-body text-text-tertiary">
        Loading installed skills…
      </p>
    ) : null;

  const {
    searchQuery,
    setSearchQuery,
    isLoading,
    isLoadingMore,
    hasMore,
    searchResultsWithStatus,
    handleSearch,
    loadMore,
    browseError,
    retry,
  } = useSkillStoreData(installedSkills, addToast);

  const handleInstallStart = (skillName: string) => {
    setInstallProgress({
      isInstalling: true,
      skillName,
      stage: "starting",
      message: "Starting installation…",
    });
  };

  // Review round 3 (B1): clears the modal for `needs-trust` and for a decline at the
  // trust prompt, so the drawer's own trust prompt is not hidden under an endless
  // "Installing…" spinner (`InstallProgressModal` is a fixed, full-screen `Dialog`).
  const handleInstallPaused = () => {
    setInstallProgress(null);
  };

  const handleInstallComplete = (result: {
    success: boolean;
    error?: string;
    skillName?: string;
    warning?: string;
  }) => {
    if (result.success) {
      addToast({
        type: "success",
        title: "Skill Installed",
        message: `Successfully installed ${result.skillName || "skill"}`,
      });
      if (result.warning) {
        addToast({
          type: "warning",
          title: `Installed ${result.skillName || "skill"}`,
          message: result.warning,
        });
      }
      // Refresh installed skills - `searchResultsWithStatus` re-derives from it.
      void loadInstalledSkills();
    } else {
      addToast({
        type: "error",
        title: "Installation Failed",
        message: result.error || "Unknown error",
      });
    }
    setInstallProgress(null);
  };

  const handleRemoveComplete = () => {
    addToast({
      type: "success",
      title: "Skill Removed",
      message: "Successfully removed skill",
    });
    // Refresh installed skills - `installedSkillsWithStatus`/`searchResultsWithStatus` re-derive from it.
    void loadInstalledSkills();
    setSelection(null);
  };

  const catalogMatches = new Map(
    searchResultsWithStatus.flatMap((skill) =>
      skill.is_installed ? [[skill.name, skill] as const] : [],
    ),
  );
  const installedSkillsWithStatus: SkillWithStatus[] = installedSkills.map((skill) => {
    const found = catalogMatches.get(skill.name);
    if (found) return found;

    // Extract GitHub owner/repo from source_url (more reliable) or source
    const topSource = extractGitHubRepo(skill.source, skill.source_url);

    return {
      id: topSource ? `${topSource}/${skill.name}` : skill.name,
      name: skill.name,
      installs: 0,
      author: null,
      description: null,
      tags: null,
      is_installed: true,
      installed_info: skill,
      top_source: topSource ?? null,
    };
  });

  const selectedSkill =
    (activeTab === "installed"
      ? installedSkillsWithStatus.find((skill) => skill.name === selection)
      : searchResultsWithStatus.find((skill) => skill.id === selection)) ?? null;

  const tabsPadding = compact ? "px-5" : "px-7";

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-bg-primary">
      {!compact && (
        <div className="border-b border-border px-7 pt-6 pb-4">
          <h2 className="m-0 mb-1 text-pretty text-balance text-title font-semibold text-text-primary">
            skills.sh
          </h2>
          <p className="m-0 text-pretty text-body text-text-tertiary">
            <a
              href="https://skills.sh"
              target="_blank"
              rel="noopener noreferrer"
              className="text-text-tertiary no-underline transition-colors hover:text-accent hover:underline"
            >
              Discover and install skills
            </a>
          </p>
        </div>
      )}

      <Tabs
        value={activeTab}
        onValueChange={(tab) => {
          setActiveTab(tab);
          setSelection(null);
        }}
        className="flex flex-1 flex-col gap-0 overflow-hidden"
      >
        <SkillStoreTabList
          installedCount={inventory.kind === "ready" ? installedSkills.length : null}
          tabsPadding={tabsPadding}
        />

        <TabsContent value="browse" className="flex flex-1 flex-col overflow-hidden">
          <div className={`flex items-center gap-4 border-b border-border py-4 ${tabsPadding}`}>
            <SkillSearchBar
              value={searchQuery}
              onChange={setSearchQuery}
              onSearch={handleSearch}
              isLoading={isLoading}
            />
            {!browseError && !isLoading && (
              <div className="ml-auto flex items-center gap-4">
                <span className="text-small tabular-nums text-text-tertiary">
                  {searchResultsWithStatus.length} skill
                  {searchResultsWithStatus.length !== 1 ? "s" : ""}
                </span>
              </div>
            )}
          </div>

          {browseError ? (
            <LoadError
              title="Browsing isn't available right now"
              error={browseError}
              onRetry={retry}
            />
          ) : (
            <div className={`flex flex-1 overflow-hidden ${compact ? "flex-col" : ""}`}>
              <SkillBrowser
                skills={searchResultsWithStatus}
                selectedSkill={selectedSkill}
                onSelectSkill={(skill) => setSelection(skill.id)}
                mode={{ kind: "browse", isLoading, isLoadingMore, hasMore, onLoadMore: loadMore }}
                emptyMessage={
                  searchQuery ? "No skills found matching your search" : "Loading popular skills…"
                }
              />
            </div>
          )}
        </TabsContent>

        <TabsContent value="installed" className="flex flex-1 flex-col overflow-hidden">
          {inventoryNotice ?? (
            <>
              <div className={`flex items-center gap-4 border-b border-border py-4 ${tabsPadding}`}>
                <span className="text-small tabular-nums text-text-tertiary">
                  {installedSkills.length} installed skill{installedSkills.length !== 1 ? "s" : ""}
                </span>
              </div>

              <div className={`flex flex-1 overflow-hidden ${compact ? "flex-col" : ""}`}>
                <SkillBrowser
                  skills={installedSkillsWithStatus}
                  selectedSkill={selectedSkill}
                  onSelectSkill={(skill) => setSelection(skill.name)}
                  mode={{ kind: "installed" }}
                  emptyMessage="No skills installed yet"
                />
              </div>
            </>
          )}
        </TabsContent>
      </Tabs>

      <Drawer open={selectedSkill !== null} onOpenChange={(open) => !open && setSelection(null)}>
        {selectedSkill && (
          <SkillDetailPanel
            skill={selectedSkill}
            inventoryNotice={inventoryNotice}
            onClose={() => setSelection(null)}
            onInstallStart={handleInstallStart}
            onInstallPaused={handleInstallPaused}
            onInstallComplete={handleInstallComplete}
            onUpdateComplete={loadInstalledSkills}
            onRemoveComplete={handleRemoveComplete}
          />
        )}
      </Drawer>

      {installProgress && (
        <InstallProgressModal progress={installProgress} onClose={() => setInstallProgress(null)} />
      )}
    </div>
  );
}

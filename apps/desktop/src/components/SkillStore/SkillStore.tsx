// ============================================================================
// SkillStore - Main skill discovery and management view
// ============================================================================

import { useState } from "react";
import { Button, Drawer, Tabs, TabsContent, TabsList, TabsTrigger } from "@skill-studio/ui";
import { SkillSearchBar } from "./SkillSearchBar";
import { SkillBrowser } from "./SkillBrowser";
import { SkillDetailPanel } from "./SkillDetailPanel";
import { InstallProgressModal } from "./InstallProgressModal";
import type {
  PaginatedSkillsResponse,
  SkillSearchResult,
  InstalledSkill,
  SkillWithStatus,
  InstallProgressState,
} from "@skill-studio/lib";
import { useLoad } from "../../hooks/useLoad";
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
  const getInstalledSkills = useAppStore((state) => state.getInstalledSkills);
  // The host reads the tracked project list, so a change to it scans again.
  const scan = useLoad(`installed:${projects.join("\n")}`, getInstalledSkills, {
    keepPreviousData: true,
  });
  let inventory: InstalledInventory = { kind: "loading" };
  if (scan.error !== null) {
    // A retry after a failure shows as loading; a rescan after a good scan keeps the list up.
    if (!scan.isLoading) inventory = { kind: "failed", message: scan.error };
  } else if (scan.data) {
    inventory = { kind: "ready", skills: scan.data };
  }

  return { inventory, loadInstalledSkills: scan.reload };
}

/** The pages of popular skills loaded after the first, and the first page they continue. */
interface MorePages {
  first: PaginatedSkillsResponse;
  skills: SkillSearchResult[];
  page: number;
  hasMore: boolean;
}

function useSkillStoreData(
  installedSkills: InstalledSkill[],
  addToast: ReturnType<typeof useAppStore.getState>["addToast"],
) {
  const getPopularSkills = useAppStore((state) => state.getPopularSkills);
  const searchSkills = useAppStore((state) => state.searchSkills);
  // What the input shows, and the debounced query the results are for.
  const [searchQuery, setSearchQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const searching = isSearchQuery(submittedQuery);
  // The results stay up while the next query loads. A failure replaces the results area with
  // `LoadError` (most commonly the Skill Studio server is not running), not a toast, since there
  // is nothing to show behind it.
  const firstPage = useLoad(
    `browse:${submittedQuery}`,
    () => (searching ? searchSkills(submittedQuery, LIMIT) : getPopularSkills(0, LIMIT)),
    { keepPreviousData: true },
  );
  const [morePages, setMorePages] = useState<MorePages | null>(null);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const first = firstPage.data;
  // Later pages count only while the first page they continue is still the one shown.
  const more = morePages !== null && morePages.first === first ? morePages : null;
  const isLoading = firstPage.isLoading;
  // The search API returns everything up to LIMIT in one page, so only popular skills page.
  const hasMore = !isLoading && !searching && (more ? more.hasMore : (first?.has_more ?? false));

  // Every result merged with the current installed status - recomputed whenever either input
  // changes, so an install or remove that refreshes `installedSkills` shows up here.
  const searchResultsWithStatus = mergeWithInstalledStatus(
    [...(first?.skills ?? []), ...(more?.skills ?? [])],
    installedSkills,
  );

  const handleSearch = (query: string) => {
    setSearchQuery(query);
    setSubmittedQuery(query);
  };

  // A Promise chain, not a try/finally statement, so the compiler can still
  // optimize this component (it doesn't support `finally` clauses yet).
  const loadMore = () => {
    if (isLoadingMore || !hasMore || !first) return;
    setIsLoadingMore(true);
    const nextPage = (more?.page ?? 0) + 1;

    return getPopularSkills(nextPage, LIMIT)
      .then((response) => {
        setMorePages({
          first,
          skills: [...(more?.skills ?? []), ...response.skills],
          page: nextPage,
          hasMore: response.has_more,
        });
      })
      .catch((err) => {
        addToast({
          type: "error",
          title: "Load More Failed",
          message: err instanceof Error ? err.message : "Failed to load more skills",
        });
      })
      .finally(() => setIsLoadingMore(false));
  };

  return {
    searchQuery,
    setSearchQuery,
    isLoading,
    isLoadingMore,
    hasMore,
    // Cleared while a new attempt runs, so "Try again" shows the loading state.
    browseError: isLoading ? null : firstPage.error,
    retry: firstPage.reload,
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
      loadInstalledSkills();
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
    loadInstalledSkills();
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

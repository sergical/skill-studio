// ============================================================================
// SkillStore - Main skill discovery and management view
// ============================================================================

import { useRef, useState } from "react";
import { Button, Drawer } from "@skill-studio/ui";
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

/** A search returns at most `LIMIT` skills, so a full page may not be all of them. */
function searchResultCount(count: number): string {
  if (count >= LIMIT) return `First ${LIMIT} results`;
  return `${count} result${count === 1 ? "" : "s"}`;
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
  // Counts searches and retries, so a "Load more" answer that arrives after one is dropped.
  const browseRequest = useRef(0);
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
    browseRequest.current += 1;
    setIsLoadingMore(false);
    setSearchQuery(query);
    setSubmittedQuery(query);
  };

  const retry = () => {
    browseRequest.current += 1;
    setIsLoadingMore(false);
    firstPage.reload();
  };

  // A Promise chain, not a try/finally statement, so the compiler can still
  // optimize this component (it doesn't support `finally` clauses yet).
  const loadMore = () => {
    if (isLoadingMore || !hasMore || !first) return;
    const request = browseRequest.current;
    setIsLoadingMore(true);
    const nextPage = (more?.page ?? 0) + 1;

    return getPopularSkills(nextPage, LIMIT)
      .then((response) => {
        if (browseRequest.current !== request) return;
        setMorePages({
          first,
          skills: [...(more?.skills ?? []), ...response.skills],
          page: nextPage,
          hasMore: response.has_more,
        });
      })
      .catch((err) => {
        if (browseRequest.current !== request) return;
        addToast({
          type: "error",
          title: "Couldn’t load more skills",
          message: err instanceof Error ? err.message : "The skills.sh request failed.",
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
    // Cleared while a new attempt runs, so "Try again" shows the loading state.
    browseError: isLoading ? null : firstPage.error,
    retry,
    searchResultsWithStatus,
    handleSearch,
    loadMore,
  };
}

/** The Add skill sheet's skills.sh tab: browse or search the catalog, each result marked when it is installed here. */
export function SkillStore() {
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
      message: "Starting…",
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
    const name = result.skillName || "the skill";
    if (result.success) {
      addToast({
        type: result.warning ? "warning" : "success",
        title: `Installed ${name}`,
        message: result.warning,
      });
      // Refresh installed skills - `searchResultsWithStatus` re-derives from it.
      loadInstalledSkills();
    } else {
      addToast({
        type: "error",
        title: `Couldn’t install ${name}`,
        message: result.error || "Unknown error",
      });
    }
    setInstallProgress(null);
  };

  const handleRemoveComplete = () => {
    addToast({ type: "success", title: `Removed ${selectedSkill?.name ?? "the skill"}` });
    // Refresh installed skills - `searchResultsWithStatus` re-derives from it.
    loadInstalledSkills();
    setSelection(null);
  };

  const selectedSkill = searchResultsWithStatus.find((skill) => skill.id === selection) ?? null;

  return (
    <div className="flex flex-1 flex-col overflow-hidden bg-bg-primary">
      <div className="flex items-center gap-4 border-b border-border px-5 py-3">
        <SkillSearchBar
          value={searchQuery}
          onChange={setSearchQuery}
          onSearch={handleSearch}
          isLoading={isLoading}
        />
        {!browseError && !isLoading && isSearchQuery(searchQuery) && (
          <span className="ml-auto shrink-0 text-small tabular-nums text-text-tertiary">
            {searchResultCount(searchResultsWithStatus.length)}
          </span>
        )}
      </div>

      {browseError ? (
        <LoadError title="Browsing isn't available right now" error={browseError} onRetry={retry} />
      ) : (
        <div className="flex flex-1 flex-col overflow-hidden">
          <SkillBrowser
            skills={searchResultsWithStatus}
            selectedSkill={selectedSkill}
            onSelectSkill={(skill) => setSelection(skill.id)}
            isLoading={isLoading}
            isLoadingMore={isLoadingMore}
            hasMore={hasMore}
            onLoadMore={loadMore}
            emptyMessage={
              isSearchQuery(searchQuery)
                ? `No skills.sh skill matches “${searchQuery.trim()}”`
                : "skills.sh listed no skills"
            }
          />
        </div>
      )}

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

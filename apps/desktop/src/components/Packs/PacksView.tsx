// ============================================================================
// PacksView - The Packs place: one row per source that ships installed
// skills, from the core's `pack_list`, grouped by the tool that installed it.
// Each change (add, update, remove) opens PackPlanDialog, which shows the plan
// before it runs anything. Adding skills from a repo goes through the Add
// Skill sheet, the one add flow.
// ============================================================================

import { useState } from "react";
import type { ReactNode } from "react";
import { Plus, RefreshCw } from "lucide-react";
import { Button } from "@skill-studio/ui";
import type { Pack, PackReport } from "@skill-studio/lib";
import { PageSection } from "../Shell/PageSection";
import { PageShell } from "../Shell/PageShell";
import { ROW_CLASS } from "../SkillList/skill-row-format";
import { useLoad } from "../../hooks/useLoad";
import { useAppStore } from "../../store/appStore";
import type { PackIntent } from "../../store/appStore";
import { CopyTextButton } from "../ui/CopyTextButton";
import { checkResult, plural } from "./pack-check-result";
import { PackPlanDialog } from "./PackPlanDialog";

/** Groups in page order: the tools Skill Studio runs first, then the ones that change elsewhere. */
const KIND_ORDER = ["skills_sh", "dotagents", "npm", "plugin"] satisfies Pack["kind"][];

const KIND_TITLE = {
  skills_sh: "skills.sh",
  dotagents: "dotagents",
  npm: "npm",
  plugin: "Plugins",
} satisfies Record<Pack["kind"], string>;

/** Why Skill Studio does not change this group's packs; absent when it does. */
const OWNER_NOTE = {
  skills_sh: null,
  dotagents: null,
  npm: "Update the npm package, then run npx skills sync",
  plugin: "The agent updates and removes these",
} satisfies Record<Pack["kind"], string | null>;

/** Source, details, actions: every row of a group shares the columns through `subgrid`. */
const PACK_LIST_GRID =
  "grid [grid-template-columns:minmax(13rem,1fr)_fit-content(14rem)_max-content] gap-x-4";
const PACK_ROW_GRID = "col-span-full grid-cols-subgrid px-3";

const isManagedElsewhere = (pack: Pack) => OWNER_NOTE[pack.kind] !== null;

/** The project folders a pack reaches, by name; empty when it is installed globally only. */
function projectNames(pack: Pack): string[] {
  return pack.scopes.flatMap((scope) =>
    scope.scope === "project" ? [scope.project.split("/").pop() ?? scope.project] : [],
  );
}

/** What the row says about the pack, from the structured report, never its one-line summary. */
function PackDetails({ pack }: { pack: Pack }) {
  const skillCount = new Set(pack.skills.map((skill) => skill.name)).size;
  const projects = projectNames(pack);
  const global = pack.scopes.some((scope) => scope.scope === "global");
  const where = projects.length === 0 ? null : [...(global ? ["Global"] : []), ...projects];
  const parts: ReactNode[] = [
    <span key="count" className="shrink-0">
      {plural(skillCount, "skill")}
    </span>,
  ];
  if (where) {
    parts.push(
      <span key="where" className="min-w-0 truncate" title={where.join(", ")}>
        {where.join(", ")}
      </span>,
    );
  }
  if (pack.edited_count > 0) {
    parts.push(
      <span key="edited" className="shrink-0 text-warning">
        {pack.edited_count} edited
      </span>,
    );
  }
  const { remote } = pack;
  if (remote.state === "checked" && pack.behind_count > 0) {
    parts.push(
      <span key="behind" className="shrink-0 text-text-primary">
        {pack.behind_count} behind
      </span>,
    );
  } else if (remote.state === "pinned_to_commit") {
    parts.push(
      <span key="pinned" className="shrink-0">
        Pinned to a commit
      </span>,
    );
  } else if (remote.state === "failed") {
    parts.push(
      <span key="failed" className="min-w-0 truncate text-error" title={remote.reason}>
        Check failed: {remote.reason}
      </span>,
    );
  }
  const { original } = pack;
  if (original) {
    parts.push(
      <span key="moved" className="min-w-0 truncate" title={original.source}>
        Moved from {original.source}
      </span>,
    );
    if (original.remote.state === "failed") {
      parts.push(
        <span
          key="original-failed"
          className="min-w-0 truncate text-error"
          title={original.remote.reason}
        >
          Original check failed: {original.remote.reason}
        </span>,
      );
    }
    if (original.differs.length > 0) {
      parts.push(
        <span key="differs" className="shrink-0" title={original.differs.join(", ")}>
          {original.differs.length} differ from the original
        </span>,
      );
    }
    if (original.not_in_source.length > 0) {
      parts.push(
        <span key="not-in-source" className="shrink-0" title={original.not_in_source.join(", ")}>
          {original.not_in_source.length} only in the original
        </span>,
      );
    }
  }
  return (
    <span className="flex min-w-0 items-baseline gap-1.5 text-small text-text-secondary tabular-nums">
      {parts.flatMap((part, index) =>
        index === 0
          ? [part]
          : [
              <span
                key={`dot-${index}`}
                aria-hidden="true"
                className="shrink-0 text-text-quaternary"
              >
                ·
              </span>,
              part,
            ],
      )}
    </span>
  );
}

/** Row actions that only matter on the row in hand show on hover or keyboard focus. */
const REVEAL = "opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100";

function PackActions({ pack, onAct }: { pack: Pack; onAct: (intent: PackIntent) => void }) {
  if (isManagedElsewhere(pack)) {
    const command = pack.update_commands[0];
    return command ? (
      <span className={REVEAL}>
        <CopyTextButton
          text={command}
          label="Copy update command"
          variant="ghost"
          namesText
          className="-mr-2 rounded-sm px-2 text-text-secondary"
        />
      </span>
    ) : null;
  }
  const behind = pack.behind_count > 0;
  return (
    <>
      {pack.new_upstream.length > 0 && (
        <Button
          variant="outline"
          size="sm"
          className="rounded-sm px-2 text-text-secondary"
          onClick={() =>
            onAct({
              kind: "add",
              source: pack.source,
              skills: pack.new_upstream,
              refPin: pack.ref_pin,
            })
          }
        >
          Add {pack.new_upstream.length} new
        </Button>
      )}
      {/* Update stays on screen only once a check shows the pack is behind; Update all covers the rest. */}
      <Button
        variant={behind ? "outline" : "ghost"}
        size="sm"
        className={`rounded-sm px-2 text-text-secondary ${behind ? "" : REVEAL}`}
        onClick={() => onAct({ kind: "update", source: pack.source })}
      >
        Update
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className={`-mr-2 rounded-sm px-2 text-text-secondary ${REVEAL}`}
        onClick={() => onAct({ kind: "remove", source: pack.source })}
      >
        Remove
      </Button>
    </>
  );
}

function PackRow({ pack, onAct }: { pack: Pack; onAct: (intent: PackIntent) => void }) {
  return (
    <li className={`${ROW_CLASS} ${PACK_ROW_GRID}`} aria-label={pack.source}>
      <span className="min-w-0 truncate text-body text-text-primary" title={pack.source}>
        {pack.source}
      </span>
      <PackDetails pack={pack} />
      <span className="flex items-center justify-end gap-1.5">
        <PackActions pack={pack} onAct={onAct} />
      </span>
    </li>
  );
}

type ListState =
  | { kind: "loading" }
  | { kind: "ready"; report: PackReport; remote: boolean }
  | { kind: "error"; message: string };

export function PacksView() {
  const listPacks = useAppStore((s) => s.listPacks);
  const openAddSkillSheet = useAppStore((s) => s.openAddSkillSheet);
  const addSheetOpen = useAppStore((s) => s.addSkillSheet.open);
  const [checking, setChecking] = useState(false);
  const [intent, setIntent] = useState<PackIntent | null>(null);
  // Stays on after the first "Check upstream", so later lists keep asking each source what is new.
  const [remote, setRemote] = useState(false);
  // Each result keeps whether it asked the sources, so the page states a check's result only
  // for a list that made one.
  const packList = useLoad(
    "packs",
    () => listPacks(remote).then((report) => ({ report, remote })),
    {
      onLoaded: () => setChecking(false),
      onFailed: () => setChecking(false),
    },
  );
  const list: ListState =
    packList.error !== null
      ? { kind: "error", message: packList.error }
      : packList.data
        ? { kind: "ready", ...packList.data }
        : { kind: "loading" };
  // The sheet may have added skills from a new source: list again once it closes.
  const [sheetWasOpen, setSheetWasOpen] = useState(addSheetOpen);
  if (sheetWasOpen !== addSheetOpen) {
    setSheetWasOpen(addSheetOpen);
    if (!addSheetOpen) packList.reload();
  }

  const checkUpstream = () => {
    setChecking(true);
    setRemote(true);
    packList.reload();
  };

  const closeDialog = () => {
    setIntent(null);
    packList.reload();
  };

  const packs = list.kind === "ready" ? list.report.packs : [];
  const canUpdateAll = packs.some((pack) => !isManagedElsewhere(pack));
  const groups = KIND_ORDER.map((kind) => ({
    kind,
    packs: packs.filter((pack) => pack.kind === kind),
  })).filter((group) => group.packs.length > 0);

  return (
    <PageShell
      title="Packs"
      actions={
        <>
          {list.kind === "ready" && list.remote && !checking && (
            <span role="status" className="mr-1.5 text-small text-text-secondary tabular-nums">
              {checkResult(packs)}
            </span>
          )}
          {packs.length > 0 && (
            <>
              <Button
                variant="outline"
                size="sm"
                className="rounded-sm px-2 text-text-secondary"
                busy={checking}
                onClick={checkUpstream}
              >
                <RefreshCw size={12} />
                Check upstream
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="rounded-sm px-2 text-text-secondary"
                disabled={!canUpdateAll}
                onClick={() => setIntent({ kind: "update", source: null })}
              >
                Update all
              </Button>
            </>
          )}
          <Button
            variant="outline"
            size="sm"
            className="rounded-sm px-2 text-text-secondary"
            onClick={() => openAddSkillSheet()}
          >
            <Plus size={12} />
            Add from a repo
          </Button>
        </>
      }
    >
      {list.kind === "loading" ? (
        <p className="m-0 text-small text-text-secondary">Loading packs…</p>
      ) : list.kind === "error" ? (
        <p role="alert" className="m-0 text-small text-error">
          Couldn't list packs: {list.message}
        </p>
      ) : packs.length === 0 ? (
        <p className="m-0 text-small text-text-secondary">
          No installed skill comes from a pack yet.
        </p>
      ) : (
        <div className="flex flex-col gap-8">
          {groups.map((group) => (
            <PageSection
              key={group.kind}
              title={
                <>
                  {KIND_TITLE[group.kind]}{" "}
                  <span className="ml-1 font-normal text-text-tertiary tabular-nums">
                    {group.packs.length}
                  </span>
                </>
              }
              meta={OWNER_NOTE[group.kind] ?? undefined}
            >
              <ul
                aria-label={KIND_TITLE[group.kind]}
                className={`m-0 list-none p-0 ${PACK_LIST_GRID}`}
              >
                {group.packs.map((pack) => (
                  <PackRow key={`${pack.kind}:${pack.source}`} pack={pack} onAct={setIntent} />
                ))}
              </ul>
            </PageSection>
          ))}
        </div>
      )}
      {intent && <PackPlanDialog intent={intent} onClose={closeDialog} />}
    </PageShell>
  );
}

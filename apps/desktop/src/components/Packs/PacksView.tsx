// ============================================================================
// PacksView - The Packs place: one row per source that ships installed
// skills, from the core's `pack_list`. Each change (add, update, remove)
// opens PackPlanDialog, which shows the plan before it runs anything. Adding
// skills from a repo goes through the Add Skill sheet, the one add flow.
// ============================================================================

import { useState } from "react";
import type { ReactNode } from "react";
import { Plus, RefreshCw } from "lucide-react";
import { Button } from "@skill-studio/ui";
import type { Pack, PackReport } from "@skill-studio/lib";
import { PageShell } from "../Shell/PageShell";
import { ROW_CLASS } from "../SkillList/skill-row-format";
import { TooltipControl } from "../ui/TooltipControl";
import { useLoad } from "../../hooks/useLoad";
import { useAppStore } from "../../store/appStore";
import type { PackIntent } from "../../store/appStore";
import { PackPlanDialog } from "./PackPlanDialog";

const KIND_LABEL = {
  skills_sh: "skills.sh",
  npm: "npm",
  dotagents: "dotagents",
  plugin: "Plugin",
} satisfies Record<Pack["kind"], string>;

const PACK_ROW_GRID =
  "[grid-template-columns:minmax(0,1fr)_88px_minmax(0,1.4fr)_240px] gap-x-3 px-3";

/** Why the owner tool, not Skill Studio, changes this pack; `null` when Skill Studio can. */
function ownerReason(pack: Pack, action: "update" | "remove"): string | null {
  if (pack.kind === "plugin") {
    return action === "update"
      ? `A plugin. The agent updates it with ${pack.update_commands.join(" or ")}.`
      : "A plugin. Remove it in the agent.";
  }
  if (pack.kind === "npm") {
    return `Comes from the npm package ${pack.source}. ${action === "update" ? "Update" : "Remove"} the package, then run npx skills sync.`;
  }
  return null;
}

function RowButton({
  disabledReason,
  onClick,
  children,
}: {
  disabledReason: string | null;
  onClick: () => void;
  children: ReactNode;
}) {
  const button = (
    <Button
      variant="outline"
      size="sm"
      className="rounded-sm px-2 text-text-secondary"
      onClick={onClick}
      disabled={disabledReason !== null}
    >
      {children}
    </Button>
  );
  if (disabledReason === null) return button;
  return (
    <TooltipControl content={disabledReason}>
      <span className="inline-flex">{button}</span>
    </TooltipControl>
  );
}

function PackRow({ pack, onAct }: { pack: Pack; onAct: (intent: PackIntent) => void }) {
  const skillCount = new Set(pack.skills.map((s) => s.name)).size;
  return (
    <li className={`${ROW_CLASS} ${PACK_ROW_GRID} h-10`} aria-label={pack.source}>
      <span className="min-w-0 truncate text-body text-text-primary">{pack.source}</span>
      <span className="text-caption text-text-secondary">{KIND_LABEL[pack.kind]}</span>
      <span className="min-w-0 truncate text-small text-text-secondary" title={pack.summary}>
        {pack.summary || `${skillCount} skill${skillCount === 1 ? "" : "s"}`}
      </span>
      <span className="flex items-center justify-end gap-1.5">
        {pack.new_upstream.length > 0 && (
          <RowButton
            disabledReason={null}
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
          </RowButton>
        )}
        <RowButton
          disabledReason={ownerReason(pack, "update")}
          onClick={() => onAct({ kind: "update", source: pack.source })}
        >
          Update
        </RowButton>
        <RowButton
          disabledReason={ownerReason(pack, "remove")}
          onClick={() => onAct({ kind: "remove", source: pack.source })}
        >
          Remove
        </RowButton>
      </span>
    </li>
  );
}

type ListState =
  | { kind: "loading" }
  | { kind: "ready"; report: PackReport }
  | { kind: "error"; message: string };

export function PacksView() {
  const listPacks = useAppStore((s) => s.listPacks);
  const openAddSkillSheet = useAppStore((s) => s.openAddSkillSheet);
  const addSheetOpen = useAppStore((s) => s.addSkillSheet.open);
  const [checking, setChecking] = useState(false);
  const [intent, setIntent] = useState<PackIntent | null>(null);
  // Stays on after the first "Check upstream", so later lists keep asking each source what is new.
  const [remote, setRemote] = useState(false);
  const packList = useLoad("packs", () => listPacks(remote), {
    onLoaded: () => setChecking(false),
    onFailed: () => setChecking(false),
  });
  const list: ListState =
    packList.error !== null
      ? { kind: "error", message: packList.error }
      : packList.data
        ? { kind: "ready", report: packList.data }
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
  const canUpdateAll = packs.some((pack) => ownerReason(pack, "update") === null);

  return (
    <PageShell
      title="Packs"
      subtitle="Sources that ship your skills. Each change shows its plan before it runs."
      actions={
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
          No installed skill comes from a pack yet. Add skills with Add from a repo.
        </p>
      ) : (
        <section className="flex flex-col gap-2">
          <p className="m-0 px-3 text-small text-text-secondary">{list.report.summary}</p>
          <ul aria-label="Packs" className="m-0 flex list-none flex-col p-0">
            {packs.map((pack) => (
              <PackRow key={`${pack.kind}:${pack.source}`} pack={pack} onAct={setIntent} />
            ))}
          </ul>
        </section>
      )}
      {intent && <PackPlanDialog intent={intent} onClose={closeDialog} />}
    </PageShell>
  );
}

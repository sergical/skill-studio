// ============================================================================
// PacksView - The Packs place: one row per source that ships installed
// skills, from the core's `pack_list`. Each change (add, update, remove)
// opens PackPlanDialog, which shows the plan before it runs anything.
// ============================================================================

import { useEffect, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import { RefreshCw } from "lucide-react";
import { Button, Input } from "@skill-studio/ui";
import type { Pack, PackReport } from "@skill-studio/lib";
import { PageShell } from "../Shell/PageShell";
import { ROW_CLASS } from "../SkillList/skill-row-format";
import { TooltipControl } from "../ui/TooltipControl";
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
            onClick={() => onAct({ kind: "add", source: pack.source, skills: pack.new_upstream })}
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
  const [list, setList] = useState<ListState>({ kind: "loading" });
  const [checking, setChecking] = useState(false);
  const [intent, setIntent] = useState<PackIntent | null>(null);
  const [addSource, setAddSource] = useState("");
  const [request, setRequest] = useState({ remote: false, serial: 0 });

  useEffect(() => {
    let live = true;
    listPacks(request.remote)
      .then((report) => live && setList({ kind: "ready", report }))
      .catch(
        (err) =>
          live &&
          setList({ kind: "error", message: err instanceof Error ? err.message : String(err) }),
      )
      .finally(() => live && setChecking(false));
    return () => {
      live = false;
    };
  }, [listPacks, request]);

  const checkUpstream = () => {
    setChecking(true);
    setRequest((current) => ({ remote: true, serial: current.serial + 1 }));
  };

  const closeDialog = () => {
    setIntent(null);
    setRequest((current) => ({ ...current, serial: current.serial + 1 }));
  };

  const submitAdd = (event: FormEvent) => {
    event.preventDefault();
    const source = addSource.trim();
    if (source) setIntent({ kind: "add", source, skills: [] });
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
        </>
      }
      toolbar={
        <form className="flex min-w-0 flex-1 items-center gap-2" onSubmit={submitAdd}>
          <Input
            aria-label="Pack source"
            placeholder="owner/repo or git URL"
            value={addSource}
            onChange={(event) => setAddSource(event.target.value)}
            className="max-w-80"
          />
          <Button type="submit" variant="outline" size="sm" disabled={addSource.trim() === ""}>
            Add pack
          </Button>
        </form>
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
          No installed skill comes from a pack yet. Add one by its source above.
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

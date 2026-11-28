// ============================================================================
// HomeAsk - What needs the user as counts that open Skills filtered to them,
// one prompt for all of it to copy into an agent with the Skill Studio MCP
// server (its preview copies line by line), and the update check.
// ============================================================================

import { useId, useState } from "react";
import { Check, ChevronDown, Copy, RefreshCw } from "lucide-react";
import { formatCount } from "@skill-studio/lib";
import { Button, cn } from "@skill-studio/ui";
import { useDelayedCall } from "../../hooks/useDelayedCall";
import { addToast } from "../../lib/toast";
import { PROMPT_CLOSING, PROMPT_OPENING, promptFor } from "./home-asks";
import type { HomeCount, HomeTask } from "./home-asks";
import type { UpdateCheck } from "./useUpdateCheck";

const TONE = { error: "text-error", warning: "text-warning", neutral: "text-text-primary" };

/** Copies text and says so in place for 1.5s; a refused clipboard says so in a toast. */
function useCopy() {
  const [copied, setCopied] = useState<string | null>(null);
  const reset = useDelayedCall();
  const copy = (key: string, text: string) => {
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(key);
        reset.schedule(() => setCopied(null), 1500);
      },
      () => addToast({ type: "error", title: "Couldn't copy to the clipboard" }),
    );
  };
  return { copied, copy };
}

interface HomeAskProps {
  tasks: HomeTask[];
  chips: HomeCount[];
  /** True once a read says no set-up agent has the Skill Studio MCP server. */
  noServer: boolean;
  updateCheck: UpdateCheck;
  openCount: (chip: HomeCount) => void;
  openSettings: () => void;
}

/** The counts, Copy prompt and the update check, in one column on the right of the week. */
export function HomeAsk({
  tasks,
  chips,
  noServer,
  updateCheck,
  openCount,
  openSettings,
}: HomeAskProps) {
  const { copied, copy } = useCopy();
  const [open, setOpen] = useState(false);
  const previewId = useId();
  const prompt = tasks.length ? promptFor(tasks) : "";
  const many = tasks.length > 1;

  return (
    <>
      <div className="flex flex-col items-start gap-2 sm:items-end">
        <div className="flex flex-wrap items-center gap-1.5">
          {tasks.length === 0 && (
            <span className="text-small text-text-secondary">Nothing needs you</span>
          )}
          {chips.map((chip) => (
            <Button key={chip.id} size="sm" variant="outline" onClick={() => openCount(chip)}>
              <span className={cn("font-semibold tabular-nums", TONE[chip.tone])}>
                {formatCount(chip.count)}
              </span>{" "}
              {chip.word}
            </Button>
          ))}
          {prompt && (
            <span className="ml-1 flex">
              <Button
                size="sm"
                className="rounded-r-none"
                onClick={() => copy("all", prompt)}
                aria-label={copied === "all" ? "Prompt copied" : "Copy prompt for your agent"}
              >
                {copied === "all" ? <Check aria-hidden /> : <Copy aria-hidden />}
                {/* Both labels share one cell, so the button keeps its width and the counts stay put. */}
                <span className="grid">
                  <span className={cn("col-start-1 row-start-1", copied === "all" && "invisible")}>
                    Copy prompt
                  </span>
                  <span className={cn("col-start-1 row-start-1", copied !== "all" && "invisible")}>
                    Copied
                  </span>
                </span>
              </Button>
              <Button
                size="icon-sm"
                className="rounded-l-none border-l border-l-black/20"
                aria-label="Show the prompt"
                aria-expanded={open}
                aria-controls={previewId}
                onClick={() => setOpen((v) => !v)}
              >
                <ChevronDown
                  aria-hidden
                  className={cn(
                    "transition-transform duration-150 motion-reduce:transition-none",
                    open && "rotate-180",
                  )}
                />
              </Button>
            </span>
          )}
          {prompt && noServer && (
            <Button
              size="sm"
              variant="outline"
              title="No set-up agent has the Skill Studio MCP server yet, and the prompt needs it."
              onClick={openSettings}
            >
              Connect the MCP server
            </Button>
          )}
        </div>
        <span className="flex items-center gap-1 text-caption text-text-tertiary">
          <span className={updateCheck.failed ? "text-error" : undefined}>
            {updateCheck.caption}
          </span>
          <Button
            variant="ghost"
            size="xs"
            className="-mr-2 text-text-secondary"
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
      </div>

      <span role="status" className="sr-only">
        {copied ? "Prompt copied" : ""}
      </span>
      {updateCheck.checkError && (
        <span role="alert" className="sr-only">
          {updateCheck.checkError}
        </span>
      )}
      {prompt && open && (
        <div
          id={previewId}
          className="order-last flex basis-full flex-col gap-1 rounded-md border border-border-subtle bg-bg-secondary py-2.5 pr-1.5 pl-3 font-mono text-small leading-relaxed select-text"
        >
          {many && <p>{PROMPT_OPENING}</p>}
          <ul aria-label="Prompt lines" className="flex flex-col gap-1">
            {tasks.map((task) => (
              <li key={task.id} className="flex items-start gap-2">
                <span className={cn("min-w-0 flex-1", many && "pl-[2ch] -indent-[2ch]")}>
                  {many ? `- ${task.line}` : promptFor([task])}
                </span>
                {many && (
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    className="text-text-tertiary"
                    aria-label={
                      copied === task.id
                        ? "Copied"
                        : `Copy only this line as a prompt: ${task.line}`
                    }
                    title="Copy only this line as a prompt"
                    onClick={() => copy(task.id, promptFor([task]))}
                  >
                    {copied === task.id ? <Check aria-hidden /> : <Copy aria-hidden />}
                  </Button>
                )}
              </li>
            ))}
          </ul>
          {many && <p>{PROMPT_CLOSING}</p>}
        </div>
      )}
    </>
  );
}

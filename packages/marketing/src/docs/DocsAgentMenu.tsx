import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import * as stylex from "@stylexjs/stylex";
import { ChevronDown, Clipboard, ExternalLink } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@skill-studio/ui";

import { SITE_ORIGIN } from "@skill-studio/lib/agent-setup";
import { docsPageStyles } from "./docs-page-styles";

interface DocsAgentMenuProps {
  markdownPath: string;
}

const COPY_PROMPT_FEEDBACK_MS = 1500;

function DocsAgentMenu({ markdownPath }: DocsAgentMenuProps) {
  const [feedback, setFeedback] = useState("");
  useEffect(() => {
    if (!feedback) return;
    const timer = window.setTimeout(() => setFeedback(""), COPY_PROMPT_FEEDBACK_MS);
    return () => window.clearTimeout(timer);
  }, [feedback]);

  const prompt = `Read ${new URL(markdownPath, SITE_ORIGIN)} and help me follow this Skill Studio guide.`;
  const encodedPrompt = encodeURIComponent(prompt);
  const targets = [
    { name: "ChatGPT", href: `https://chatgpt.com/?hints=search&q=${encodedPrompt}` },
    { name: "Claude", href: `https://claude.ai/new?q=${encodedPrompt}` },
    { name: "Cursor", href: `https://cursor.com/link/prompt?text=${encodedPrompt}` },
  ];
  const colors = stylex.props(docsPageStyles.colors).className ?? "";

  async function copyAgentPrompt() {
    try {
      await navigator.clipboard.writeText(prompt);
      setFeedback("Prompt copied");
    } catch {
      setFeedback("Copy failed");
    }
  }

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger className="docs-action docs-agent-trigger" aria-label="Open in">
          <span>{feedback || "Open in…"}</span>
          <ChevronDown size={14} aria-hidden="true" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" sideOffset={6} className={`docs-agent-menu ${colors}`}>
          {targets.map(({ name, href }) => (
            <DropdownMenuItem
              key={name}
              className="docs-agent-item"
              nativeButton={false}
              render={<a href={href} target="_blank" rel="noopener noreferrer" />}
            >
              {name}
              <ExternalLink size={14} aria-hidden="true" />
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator className="docs-agent-separator" />
          {["Claude Code", "Codex"].map((name) => (
            <DropdownMenuItem
              key={name}
              onClick={() => void copyAgentPrompt()}
              className="docs-agent-item"
            >
              {name}
              <span className="docs-agent-item-detail">
                Copy prompt <Clipboard size={14} aria-hidden="true" />
              </span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <span className="docs-sr-only" role="status">
        {feedback}
      </span>
    </>
  );
}

/** Mounts the shared UI menu inside an otherwise static docs page. */
export function mountDocsAgentMenu(root: HTMLElement, markdownPath: string) {
  createRoot(root).render(<DocsAgentMenu markdownPath={markdownPath} />);
}

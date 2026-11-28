import { useEffect, useState, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import * as stylex from "@stylexjs/stylex";
import { ArrowUpRight, Check, FileText } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@skill-studio/ui";

import { SITE_ORIGIN } from "@skill-studio/lib/agent-setup";
import { AgentIcon } from "../AgentIcon";
import { DocsAgentTriggerContent } from "./DocsAgentTrigger";
import { docsPageStyles } from "./docs-page-styles";

interface DocsAgentMenuProps {
  markdownPath: string;
}

const COPY_PROMPT_FEEDBACK_MS = 1500;

function ChatGptMark() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor" fillRule="evenodd">
      <path d="M9.205 8.658v-2.26c0-.19.072-.333.238-.428l4.543-2.616c.619-.357 1.356-.523 2.117-.523 2.854 0 4.662 2.212 4.662 4.566 0 .167 0 .357-.024.547l-4.71-2.759a.797.797 0 00-.856 0l-5.97 3.473zm10.609 8.8V12.06c0-.333-.143-.57-.429-.737l-5.97-3.473 1.95-1.118a.433.433 0 01.476 0l4.543 2.617c1.309.76 2.189 2.378 2.189 3.948 0 1.808-1.07 3.473-2.76 4.163zM7.802 12.703l-1.95-1.142c-.167-.095-.239-.238-.239-.428V5.899c0-2.545 1.95-4.472 4.591-4.472 1 0 1.927.333 2.712.928L8.23 5.067c-.285.166-.428.404-.428.737v6.898zM12 15.128l-2.795-1.57v-3.33L12 8.658l2.795 1.57v3.33L12 15.128zm1.796 7.23c-1 0-1.927-.332-2.712-.927l4.686-2.712c.285-.166.428-.404.428-.737v-6.898l1.974 1.142c.167.095.238.238.238.428v5.233c0 2.545-1.974 4.472-4.614 4.472zm-5.637-5.303l-4.544-2.617c-1.308-.761-2.188-2.378-2.188-3.948A4.482 4.482 0 014.21 6.327v5.423c0 .333.143.571.428.738l5.947 3.449-1.95 1.118a.432.432 0 01-.476 0zm-.262 3.9c-2.688 0-4.662-2.021-4.662-4.519 0-.19.024-.38.047-.57l4.686 2.71c.286.167.571.167.856 0l5.97-3.448v2.26c0 .19-.07.333-.237.428l-4.543 2.616c-.619.357-1.356.523-2.117.523zm5.899 2.83a5.947 5.947 0 005.827-4.756C22.287 18.339 24 15.84 24 13.296c0-1.665-.713-3.282-1.998-4.448.119-.5.19-.999.19-1.498 0-3.401-2.759-5.947-5.946-5.947-.642 0-1.26.095-1.88.31A5.962 5.962 0 0010.205 0a5.947 5.947 0 00-5.827 4.757C1.713 5.447 0 7.945 0 10.49c0 1.666.713 3.283 1.998 4.448-.119.5-.19 1-.19 1.499 0 3.401 2.759 5.946 5.946 5.946.642 0 1.26-.095 1.88-.309a5.96 5.96 0 004.162 1.713z" />
    </svg>
  );
}

function ClaudeMark() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor">
      <path d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z" />
    </svg>
  );
}

interface MenuEntryProps {
  icon: ReactNode;
  label: string;
  detail: ReactNode;
}

function MenuEntry({ icon, label, detail }: MenuEntryProps) {
  return (
    <>
      {/* AgentIcon labels itself; the item text already names the tool. */}
      <span className="docs-agent-item-icon" aria-hidden="true">
        {icon}
      </span>
      <span className="docs-agent-item-label">{label}</span>
      <span className="docs-agent-item-detail">{detail}</span>
    </>
  );
}

function DocsAgentMenu({ markdownPath }: DocsAgentMenuProps) {
  const [copied, setCopied] = useState<{ agent: string; ok: boolean }>();
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(undefined), COPY_PROMPT_FEEDBACK_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const prompt = `Read ${new URL(markdownPath, SITE_ORIGIN)} and help me follow this Skill Studio guide.`;
  const encodedPrompt = encodeURIComponent(prompt);
  const chats = [
    {
      name: "ChatGPT",
      icon: <ChatGptMark />,
      href: `https://chatgpt.com/?hints=search&q=${encodedPrompt}`,
    },
    { name: "Claude", icon: <ClaudeMark />, href: `https://claude.ai/new?q=${encodedPrompt}` },
    {
      name: "Cursor",
      icon: <AgentIcon agent="cursor" size={16} />,
      href: `https://cursor.com/link/prompt?text=${encodedPrompt}`,
    },
  ];
  const agents = [
    { name: "Claude Code", icon: <AgentIcon agent="claude" size={16} /> },
    { name: "Codex", icon: <AgentIcon agent="codex" size={16} /> },
  ];
  const colors = stylex.props(docsPageStyles.colors).className ?? "";

  async function copyAgentPrompt(agent: string) {
    try {
      await navigator.clipboard.writeText(prompt);
      setCopied({ agent, ok: true });
    } catch {
      setCopied({ agent, ok: false });
    }
  }

  function copyDetail(agent: string) {
    if (copied?.agent !== agent) return "Copy prompt";
    if (!copied.ok) return "Copy failed";
    return (
      <>
        <Check size={14} aria-hidden="true" />
        Copied
      </>
    );
  }

  return (
    <>
      <DropdownMenu modal={false}>
        <DropdownMenuTrigger className="docs-button docs-agent-trigger" aria-label="Open page in">
          <DocsAgentTriggerContent />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" sideOffset={6} className={`docs-agent-menu ${colors}`}>
          <DropdownMenuItem
            className="docs-agent-item"
            nativeButton={false}
            render={<a href={markdownPath} />}
          >
            <MenuEntry icon={<FileText size={16} />} label="View as Markdown" detail={null} />
          </DropdownMenuItem>
          <DropdownMenuSeparator className="docs-agent-separator" />
          {chats.map(({ name, icon, href }) => (
            <DropdownMenuItem
              key={name}
              className="docs-agent-item"
              nativeButton={false}
              render={<a href={href} target="_blank" rel="noopener noreferrer" />}
            >
              <MenuEntry
                icon={icon}
                label={`Open in ${name}`}
                detail={<ArrowUpRight size={14} aria-hidden="true" />}
              />
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator className="docs-agent-separator" />
          {agents.map(({ name, icon }) => (
            // Stays open so the row can show that the prompt was copied.
            <DropdownMenuItem
              key={name}
              closeOnClick={false}
              onClick={() => void copyAgentPrompt(name)}
              className="docs-agent-item"
            >
              <MenuEntry icon={icon} label={name} detail={copyDetail(name)} />
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <span className="docs-sr-only" role="status">
        {copied && (copied.ok ? `${copied.agent} prompt copied` : "Copy failed")}
      </span>
    </>
  );
}

/** Mounts the shared UI menu inside an otherwise static docs page. */
export function mountDocsAgentMenu(root: HTMLElement, markdownPath: string) {
  createRoot(root).render(<DocsAgentMenu markdownPath={markdownPath} />);
}

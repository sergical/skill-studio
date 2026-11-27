// ============================================================================
// CopyTextButton - Copies a command, says "Copied" for a moment, and shows a
// toast when the clipboard refuses. Packs uses it for an agent's update
// command and for a new pack's install line.
// ============================================================================

import { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { addToast } from "../../lib/toast";

interface CopyTextButtonProps {
  text: string;
  label: string;
  variant?: "outline" | "ghost";
  className?: string;
}

export function CopyTextButton({
  text,
  label,
  variant = "outline",
  className = "",
}: CopyTextButtonProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = () => {
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => setCopied(false), 1500);
      },
      () => addToast({ type: "error", title: "Couldn't copy to the clipboard" }),
    );
  };
  return (
    <Button
      variant={variant}
      size="sm"
      className={`rounded-sm px-2 text-text-secondary ${className}`}
      aria-label={`${label}: ${text}`}
      onClick={copy}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      {copied ? "Copied" : label}
    </Button>
  );
}

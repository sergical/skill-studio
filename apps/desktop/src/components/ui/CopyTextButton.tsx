// ============================================================================
// CopyTextButton - Copies a command, says "Copied" for a moment, and shows a
// toast when the clipboard refuses. Packs and Settings copy commands and
// prompts with it.
// ============================================================================

import { useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@skill-studio/ui";
import { useDelayedCall } from "../../hooks/useDelayedCall";
import { addToast } from "../../lib/toast";

interface CopyTextButtonProps {
  text: string;
  label: string;
  variant?: "outline" | "ghost";
  className?: string;
  /** Name the button with the text it copies, for rows that each copy something different. */
  namesText?: boolean;
  /** Hover text, for a button whose label does not show what it copies. */
  title?: string;
}

export function CopyTextButton({
  text,
  label,
  variant = "outline",
  className = "",
  namesText = false,
  title,
}: CopyTextButtonProps) {
  const [copied, setCopied] = useState(false);
  const reset = useDelayedCall();
  const copy = () => {
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        reset.schedule(() => setCopied(false), 1500);
      },
      () => addToast({ type: "error", title: "Couldn't copy to the clipboard" }),
    );
  };
  return (
    <Button
      variant={variant}
      size="sm"
      className={className}
      aria-label={namesText ? `${label}: ${text}` : undefined}
      title={title}
      onClick={copy}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      {copied ? "Copied" : label}
    </Button>
  );
}

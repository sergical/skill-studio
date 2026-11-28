// ============================================================================
// CopyTextButton - Copies a command, says "Copied" for a moment, and shows a
// toast when the clipboard refuses. Packs and Settings copy commands and
// prompts with it.
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
  /** Name the button with the text it copies, for rows that each copy something different. */
  namesText?: boolean;
}

export function CopyTextButton({
  text,
  label,
  variant = "outline",
  className = "",
  namesText = false,
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
      className={className}
      aria-label={namesText ? `${label}: ${text}` : undefined}
      onClick={copy}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      {copied ? "Copied" : label}
    </Button>
  );
}

// ============================================================================
// SettingsCard - the error and copy pieces every Settings section shares, so
// each one reports a failure and confirms a copy the same way.
// ============================================================================

import { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@skill-studio/ui";

interface SettingsErrorProps {
  /** The first line is the sentence to show; the rest is detail behind "Show details". */
  message: string;
}

export function SettingsError({ message }: SettingsErrorProps) {
  const [summary, ...rest] = message.split("\n");
  const detail = rest.join("\n").trim();
  return (
    <div role="alert" className="flex flex-col gap-1 rounded-md bg-error/10 px-3 py-2 text-small">
      <p className="m-0 text-error">{summary}</p>
      {detail && (
        <details className="text-text-secondary">
          <summary className="cursor-pointer select-none text-text-tertiary">Show details</summary>
          <pre className="m-0 mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed">
            {detail}
          </pre>
        </details>
      )}
    </div>
  );
}

interface CopyButtonProps {
  text: string;
  label: string;
}

/** Copies `text` and confirms in place for a moment. */
export function CopyButton({ text, label }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <Button
      variant="outline"
      size="sm"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      {copied ? "Copied" : label}
    </Button>
  );
}
